//! Public render-element wrapper for conservative upstream blur composition.
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::{
    Renderer,
    utils::{CommitCounter, DamageSet, OpaqueRegions},
};
use smithay::utils::{Buffer, Physical, Rectangle, Scale, Transform, user_data::UserDataMap};

pub struct Unculled<'a, E> {
    element: &'a E,
    blur: bool,
}

pub fn prepare<E: Element>(elements: &[E]) -> (Vec<Unculled<'_, E>>, bool) {
    let blur = elements.iter().any(Element::is_framebuffer_effect);
    (
        elements
            .iter()
            .map(|element| Unculled { element, blur })
            .collect(),
        blur,
    )
}

impl<E: Element> Element for Unculled<'_, E> {
    fn id(&self) -> &Id {
        self.element.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.element.current_commit()
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        self.element.src()
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.element.geometry(scale)
    }
    fn transform(&self) -> Transform {
        self.element.transform()
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        if self.blur {
            DamageSet::from_slice(&[self.element.geometry(scale)])
        } else {
            self.element.damage_since(scale, commit)
        }
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        if self.blur {
            OpaqueRegions::default()
        } else {
            self.element.opaque_regions(scale)
        }
    }
    fn alpha(&self) -> f32 {
        self.element.alpha()
    }
    fn kind(&self) -> Kind {
        self.element.kind()
    }
    fn is_framebuffer_effect(&self) -> bool {
        self.element.is_framebuffer_effect()
    }
}

impl<R: Renderer, E: RenderElement<R>> RenderElement<R> for Unculled<'_, E> {
    fn capture_framebuffer(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), R::Error> {
        self.element.capture_framebuffer(frame, src, dst, cache)
    }
    fn draw(
        &self,
        frame: &mut R::Frame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), R::Error> {
        self.element.draw(frame, src, dst, damage, opaque, cache)
    }
    fn underlying_storage(&self, renderer: &mut R) -> Option<UnderlyingStorage<'_>> {
        if self.blur {
            None
        } else {
            self.element.underlying_storage(renderer)
        }
    }
}
