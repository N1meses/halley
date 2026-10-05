//! Opening shaders consume the complete live window scene, not a separate
//! approximation of its client and decorations. Framebuffer effects capture
//! the real backdrop before the scene is rendered into a reusable texture.
use std::cell::RefCell;
use std::rc::Rc;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::{
    Element, Id, Kind, RenderElement, RenderElementPresentationState, RenderElementState,
    RenderElementStates, UnderlyingStorage,
};
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesRenderer, GlesTexProgram, GlesTexture, Uniform,
};
use smithay::backend::renderer::utils::{CommitCounter, OpaqueRegions};
use smithay::backend::renderer::{
    Bind, Color32F, ContextId, Frame, FrameContext, Offscreen, Renderer, Texture,
};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform, user_data::UserDataMap};

use super::scene::SceneElement;
use super::window_shader::{expanded_area, shader_mapping};

pub type OpenBuffer = RefCell<Option<(ContextId<GlesTexture>, GlesTexture)>>;

pub struct WindowOpenElement {
    id: Id,
    elements: Vec<SceneElement>,
    bounds: Rectangle<i32, Physical>,
    area: Rectangle<i32, Physical>,
    program: GlesTexProgram,
    progress: f32,
    clamped_progress: f32,
    random_seed: f32,
    buffer: Rc<OpenBuffer>,
}

pub fn scene_bounds(elements: &[SceneElement]) -> Option<Rectangle<i32, Physical>> {
    elements
        .iter()
        .filter_map(|element| {
            // A blur's capture dependency covers the output, but its displayed
            // pixels only cover its patches. Do not turn the opening into an
            // animation of the entire monitor.
            let rect = match element {
                SceneElement::BackdropBlur(blur) => blur.visible_geometry(),
                _ => element.geometry(Scale::from(1.0)),
            };
            (!rect.is_empty()).then_some(rect)
        })
        .reduce(|a, b| a.merge(b))
}

/// Attribute a presented opening snapshot to the live elements drawn into it.
/// Without these states, Smithay treats the client as hidden and throttles its
/// frame callbacks even though its pixels are visible through the shader.
pub fn extend_render_states(elements: &[SceneElement], states: &mut RenderElementStates) {
    for element in elements {
        let SceneElement::WindowOpen(opening) = element else {
            continue;
        };
        let children = opening.elements.iter().filter_map(|child| {
            let visible = child.geometry(1.0.into()).intersection(opening.bounds)?;
            let area = (visible.size.w as usize).saturating_mul(visible.size.h as usize);
            (area > 0).then(|| (child.id().clone(), area))
        });
        extend_snapshot_states(&opening.id, children, states);
    }
}

fn extend_snapshot_states(
    snapshot: &Id,
    children: impl Iterator<Item = (Id, usize)>,
    states: &mut RenderElementStates,
) {
    let Some(parent) = states
        .element_render_state(snapshot.clone())
        .filter(|state| {
            state.visible_area > 0
                && state.presentation_state != RenderElementPresentationState::Skipped
        })
    else {
        return;
    };
    for (id, area) in children {
        let composed = RenderElementState {
            visible_area: area.min(parent.visible_area),
            // Client pixels were composed into a texture, never scanned out
            // directly. Do not advertise zero-copy presentation or a scanout
            // DMA-BUF tranche for this offscreen composition.
            presentation_state: RenderElementPresentationState::Rendering { reason: None },
            needs_capture: false,
        };
        states
            .states
            .entry(id)
            .and_modify(|existing| {
                if existing.presentation_state == RenderElementPresentationState::Skipped {
                    *existing = composed;
                }
            })
            .or_insert(composed);
    }
}

impl WindowOpenElement {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Id,
        elements: Vec<SceneElement>,
        bounds: Rectangle<i32, Physical>,
        program: GlesTexProgram,
        progress: f32,
        clamped_progress: f32,
        random_seed: f32,
        buffer: Rc<OpenBuffer>,
    ) -> Self {
        Self {
            id,
            elements,
            bounds,
            area: expanded_area(bounds),
            program,
            progress,
            clamped_progress,
            random_seed,
            buffer,
        }
    }
}

impl Element for WindowOpenElement {
    fn id(&self) -> &Id {
        &self.id
    }
    fn current_commit(&self) -> CommitCounter {
        // Opening is already an active repaint reason. Include progress and
        // live child commits so output damage also follows client repaints.
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.progress.to_bits().hash(&mut hash);
        self.clamped_progress.to_bits().hash(&mut hash);
        self.random_seed.to_bits().hash(&mut hash);
        self.bounds.loc.x.hash(&mut hash);
        self.bounds.loc.y.hash(&mut hash);
        self.bounds.size.w.hash(&mut hash);
        self.bounds.size.h.hash(&mut hash);
        for element in &self.elements {
            element.id().hash(&mut hash);
            let geometry = element.geometry(1.0.into());
            geometry.loc.x.hash(&mut hash);
            geometry.loc.y.hash(&mut hash);
            geometry.size.w.hash(&mut hash);
            geometry.size.h.hash(&mut hash);
            element
                .current_commit()
                .distance(Some(CommitCounter::default()))
                .hash(&mut hash);
        }
        CommitCounter::from(hash.finish() as usize)
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size(
            self.bounds
                .size
                .to_logical(1)
                .to_f64()
                .to_buffer(1.0, Transform::Normal),
        )
    }
    fn geometry(&self, _: Scale<f64>) -> Rectangle<i32, Physical> {
        self.area
    }
    fn opaque_regions(&self, _: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        OpaqueRegions::default()
    }
    fn kind(&self) -> Kind {
        Kind::Unspecified
    }
    fn is_framebuffer_effect(&self) -> bool {
        self.elements.iter().any(Element::is_framebuffer_effect)
    }
}

impl RenderElement<GlesRenderer> for WindowOpenElement {
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _: Rectangle<f64, Buffer>,
        _: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        for (index, element) in self.elements.iter().enumerate().rev() {
            if let SceneElement::BackdropBlur(blur) = element {
                // The ordinary scene draws its shadow before the blur captures
                // the backdrop. Preserve that order inside the captured blur
                // without drawing a detached rectangular shadow to the output.
                blur.capture_with_background(frame, &self.elements[index + 1..], cache)?;
            } else if element.is_framebuffer_effect() {
                element.capture_framebuffer(
                    frame,
                    element.src(),
                    element.geometry(1.0.into()),
                    cache,
                )?;
            }
        }
        Ok(())
    }

    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        _: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _: &[Rectangle<i32, Physical>],
        _: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        let mut buffer = self.buffer.borrow_mut();
        {
            let mut guard = FrameContext::renderer(frame);
            let renderer = guard.as_mut();
            let context = renderer.context_id();
            let size = self
                .bounds
                .size
                .to_logical(1)
                .to_buffer(1, Transform::Normal);
            if !buffer.as_ref().is_some_and(|(old_context, texture)| {
                *old_context == context && texture.size() == size
            }) {
                *buffer = Some((
                    context,
                    <GlesRenderer as Offscreen<GlesTexture>>::create_buffer(
                        renderer,
                        Fourcc::Abgr8888,
                        size,
                    )?,
                ));
            }
            let (_, texture) = buffer.as_mut().expect("allocated opening texture");
            let mut target = renderer.bind(texture)?;
            let mut snapshot = renderer.render(&mut target, self.bounds.size, Transform::Normal)?;
            snapshot.clear(
                Color32F::TRANSPARENT,
                &[Rectangle::from_size(self.bounds.size)],
            )?;
            // Child framebuffer effects were captured from the output above;
            // do not recapture the transparent offscreen framebuffer.
            for element in self.elements.iter().rev() {
                let mut geometry = element.geometry(1.0.into());
                geometry.loc -= self.bounds.loc;
                let Some(visible) = geometry.intersection(Rectangle::from_size(self.bounds.size))
                else {
                    continue;
                };
                element.draw(
                    &mut snapshot,
                    element.src(),
                    geometry,
                    &[Rectangle::new(visible.loc - geometry.loc, visible.size)],
                    &[],
                    None,
                )?;
            }
            let _ = snapshot.finish()?;
        }
        let (_, texture) = buffer.as_ref().expect("rendered opening texture");
        let mapping = shader_mapping(self.bounds, self.area);
        frame.render_texture_from_to(
            texture,
            self.src(),
            dst,
            damage,
            &[],
            Transform::Normal,
            1.0,
            Some(&self.program),
            &[
                Uniform::new("halley_progress", self.progress),
                Uniform::new("halley_clamped_progress", self.clamped_progress),
                Uniform::new("halley_random_seed", self.random_seed),
                Uniform::new("halley_input_scale", mapping.input_scale),
                Uniform::new("halley_input_offset", mapping.input_offset),
                Uniform::new("halley_tex_scale", mapping.tex_scale),
                Uniform::new("halley_tex_offset", mapping.tex_offset),
                Uniform::new("halley_geo_size", mapping.geo_size),
            ],
        )
    }
    fn underlying_storage(&self, _: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presented_opening_keeps_client_and_subsurface_visible_as_composed_pixels() {
        let snapshot = Id::new();
        let client = Id::new();
        let subsurface = Id::new();
        let mut states = RenderElementStates::default();
        states.states.insert(
            snapshot.clone(),
            RenderElementState {
                visible_area: 100,
                presentation_state: RenderElementPresentationState::Rendering { reason: None },
                needs_capture: true,
            },
        );
        states.states.insert(
            client.clone(),
            RenderElementState {
                visible_area: 0,
                presentation_state: RenderElementPresentationState::Skipped,
                needs_capture: false,
            },
        );
        extend_snapshot_states(
            &snapshot,
            [(client.clone(), 200), (subsurface.clone(), 40)].into_iter(),
            &mut states,
        );
        assert!(states.element_was_presented(client.clone()));
        assert!(states.element_was_presented(subsurface.clone()));
        let state = states.element_render_state(client).unwrap();
        assert_eq!(state.visible_area, 100);
        assert_eq!(
            state.presentation_state,
            RenderElementPresentationState::Rendering { reason: None }
        );
        assert!(!state.needs_capture);
        assert_eq!(
            states
                .element_render_state(subsurface)
                .unwrap()
                .visible_area,
            40
        );
    }

    #[test]
    fn hidden_opening_does_not_make_captured_clients_visible() {
        let snapshot = Id::new();
        let client = Id::new();
        let mut states = RenderElementStates::default();
        extend_snapshot_states(&snapshot, [(client.clone(), 200)].into_iter(), &mut states);
        assert!(!states.element_was_presented(client.clone()));
        states.states.insert(
            snapshot.clone(),
            RenderElementState {
                visible_area: 0,
                presentation_state: RenderElementPresentationState::Skipped,
                needs_capture: false,
            },
        );
        extend_snapshot_states(&snapshot, [(client.clone(), 200)].into_iter(), &mut states);
        assert!(!states.element_was_presented(client));
    }
}
