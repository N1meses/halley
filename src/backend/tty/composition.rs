use smithay::backend::renderer::damage::{Error, OutputDamageTracker};
use smithay::backend::renderer::element::texture::{TextureRenderBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{Kind, RenderElement, RenderElementStates};
use smithay::backend::renderer::{Bind, Color32F, Renderer, Texture};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Size, Transform};

/// A secondary output's retained scene on the primary GPU. The intermediate
/// texture has age one; the output GPU's swapchain tracks its own buffer ages.
pub(super) struct ComposedFrame<T: Texture> {
    buffer: TextureRenderBuffer<T>,
    damage: OutputDamageTracker,
    size: Size<i32, Buffer>,
    scale: Scale<f64>,
    output_transform: Transform,
}

impl<T: Texture + Clone> ComposedFrame<T> {
    pub fn new<R: Renderer<TextureId = T>>(
        renderer: &R,
        texture: T,
        scale: Scale<f64>,
        output_transform: Transform,
    ) -> Self {
        let size = texture.size();
        Self {
            // Keep empty opaque regions: PrimaryGpuTextureElement draws through
            // the underlying GLES frame, so MultiFrame records transfer damage
            // through its clear operation rather than its texture draw operation.
            buffer: TextureRenderBuffer::from_texture(
                renderer,
                texture,
                1,
                Transform::Normal,
                Some(Vec::new()),
            ),
            damage: OutputDamageTracker::new((size.w, size.h), scale, Transform::Normal),
            size,
            scale,
            output_transform,
        }
    }

    pub fn matches(
        &self,
        size: Size<i32, Buffer>,
        scale: Scale<f64>,
        transform: Transform,
    ) -> bool {
        self.size == size && self.scale == scale && self.output_transform == transform
    }

    pub fn render<R, E>(
        &mut self,
        renderer: &mut R,
        elements: &[E],
        clear: Color32F,
        force_full_repaint: bool,
    ) -> Result<RenderElementStates, Error<R::Error>>
    where
        R: Renderer<TextureId = T> + Bind<T>,
        E: RenderElement<R>,
    {
        let (elements, blur) = crate::render::conservative::prepare(elements);
        let force_full_repaint = force_full_repaint || blur;
        let mut states = RenderElementStates::default();
        let size = self.size.to_logical(1, Transform::Normal);
        let damage_tracker = &mut self.damage;
        self.buffer.render().draw(|texture| {
            let mut target = renderer.bind(texture).map_err(Error::Rendering)?;
            let result = damage_tracker.render_output(
                renderer,
                &mut target,
                usize::from(!force_full_repaint),
                &elements,
                clear,
            )?;
            states = result.states;
            // Scene coordinates already include output scale and rotation. This
            // texture is in that physical space, so its damage uses scale one
            // and no additional transform. Publish it only after rendering succeeds.
            Ok::<_, Error<R::Error>>(
                result
                    .damage
                    .into_iter()
                    .flatten()
                    .map(|rect| rect.to_logical(1).to_buffer(1, Transform::Normal, &size))
                    .collect(),
            )
        })?;
        Ok(states)
    }

    pub fn element(
        &self,
        logical_size: Size<i32, smithay::utils::Logical>,
    ) -> TextureRenderElement<T> {
        TextureRenderElement::from_texture_render_buffer(
            (0.0, 0.0),
            &self.buffer,
            Some(1.0),
            Some(
                Rectangle::<i32, Physical>::from_size((self.size.w, self.size.h).into())
                    .to_f64()
                    .to_logical(1.0),
            ),
            Some(logical_size),
            Kind::Unspecified,
        )
    }
}

#[cfg(test)]
mod tests;
