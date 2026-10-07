//! Convert the output-local logical scene into framebuffer pixels exactly once.
//! Layout, hit testing, and the Field camera continue using logical coordinates.

use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform, user_data::UserDataMap};

use super::scene::SceneElement;

pub fn scale_rect(rect: Rectangle<i32, Physical>, scale: f64) -> Rectangle<i32, Physical> {
    let x = (rect.loc.x as f64 * scale).round() as i32;
    let y = (rect.loc.y as f64 * scale).round() as i32;
    let right = ((rect.loc.x + rect.size.w) as f64 * scale).round() as i32;
    let bottom = ((rect.loc.y + rect.size.h) as f64 * scale).round() as i32;
    Rectangle::new((x, y).into(), (right - x, bottom - y).into())
}

pub struct DisplayScaledElement {
    pub inner: Box<SceneElement>,
    scale: f64,
    query_scale: f64,
}

pub fn element(mut inner: SceneElement, scale: f64, physical_scene: bool) -> SceneElement {
    if scale == 1.0 {
        return inner;
    }
    let physical = physical_scene || matches!(inner, SceneElement::Cursor(_));
    let mut query_scale = if physical { scale } else { 1.0 };
    let mut scale = if physical { 1.0 } else { scale };
    if let SceneElement::WindowOpen(opening) = &mut inner {
        opening.apply_display_scale(scale);
        scale = 1.0;
        query_scale = 1.0;
    }
    SceneElement::DisplayScaled(DisplayScaledElement {
        inner: Box::new(inner),
        scale,
        query_scale,
    })
}

impl Element for DisplayScaledElement {
    fn id(&self) -> &Id {
        self.inner.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.inner.current_commit()
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        self.inner.src()
    }
    fn geometry(&self, _: Scale<f64>) -> Rectangle<i32, Physical> {
        scale_rect(self.inner.geometry(self.query_scale.into()), self.scale)
    }
    fn transform(&self) -> Transform {
        self.inner.transform()
    }
    fn alpha(&self) -> f32 {
        self.inner.alpha()
    }
    fn kind(&self) -> Kind {
        if self.scale == 1.0 {
            self.inner.kind()
        } else {
            Kind::Unspecified
        }
    }
    fn is_framebuffer_effect(&self) -> bool {
        self.inner.is_framebuffer_effect()
    }
    fn damage_since(
        &self,
        _: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        let original = self.inner.geometry(self.query_scale.into());
        let target = self.geometry(1.0.into());
        DamageSet::from_slice(
            &self
                .inner
                .damage_since(self.query_scale.into(), commit)
                .iter()
                .map(|rect| {
                    let global = Rectangle::new(rect.loc + original.loc, rect.size);
                    let scaled = global.to_f64().upscale(self.scale).to_i32_up();
                    Rectangle::new(scaled.loc - target.loc, scaled.size)
                })
                .collect::<Vec<_>>(),
        )
    }
    fn opaque_regions(&self, _: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        // Rounding fractional edges inward is required for safe occlusion.
        // Leave scaled content conservative; unchanged 1x content keeps its regions.
        if self.scale != 1.0 {
            return OpaqueRegions::default();
        }
        self.inner.opaque_regions(self.query_scale.into())
    }
}

impl RenderElement<GlesRenderer> for DisplayScaledElement {
    fn capture_framebuffer(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), GlesError> {
        self.inner.capture_framebuffer(frame, src, dst, cache)
    }
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        self.inner.draw(frame, src, dst, damage, opaque, cache)
    }
    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        if self.scale == 1.0 {
            self.inner.underlying_storage(renderer)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_rectangles_share_fractional_pixel_edges() {
        for scale in [0.75, 1.25, 1.5, 2.0] {
            let left = scale_rect(Rectangle::new((-1, 0).into(), (3, 5).into()), scale);
            let right = scale_rect(Rectangle::new((2, 0).into(), (7, 5).into()), scale);
            assert_eq!(left.loc.x + left.size.w, right.loc.x);
        }
    }
}
