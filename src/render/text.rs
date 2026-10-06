use std::collections::HashMap;
use std::error::Error;
use std::time::{Duration, Instant};

use halley_ui::{Font as UiFont, TextSystem};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{Element, Id, Kind, RenderElement, UnderlyingStorage};
use smithay::backend::renderer::gles::{GlesError, GlesFrame, GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::{CommitCounter, DamageSet, OpaqueRegions};
use smithay::backend::renderer::{ContextId, ImportMem, Renderer};
use smithay::utils::user_data::UserDataMap;
use smithay::utils::{Buffer, Logical, Physical, Point, Rectangle, Scale, Transform};

const TEXT_CACHE_TTL: Duration = Duration::from_secs(30);
struct TextTexture {
    texture: GlesTexture,
    size: smithay::utils::Size<i32, Buffer>,
    last_used: Instant,
}

#[derive(Default)]
struct TextElementSizing {
    font_size_px: Option<u16>,
    destination_size: Option<smithay::utils::Size<i32, Physical>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TextKey {
    text: String,
    family: String,
    size_px: u16,
    rgb: [u8; 3],
}

/// Shared renderer for compositor-owned text.
///
/// The live `font.size` setting is the default authority. A small number of
/// explicitly configurable widgets may request a per-element size; the cache
/// key keeps those rasterizations independent from global typography.
pub struct UiTextRenderer {
    context: Option<ContextId<GlesTexture>>,
    text_system: TextSystem,
    font: halley_config::Font,
    text: HashMap<TextKey, TextTexture>,
    /// Stable element identities, keyed by what the label *is* rather than by
    /// its call site, so the nineteen `element()` callers need not each invent
    /// and thread a slot name.
    ///
    /// The key is a hash of the [`TextKey`] plus how many times that exact
    /// label has already been emitted this frame. Two labels that collide
    /// would merely share an identity and so damage each other's rectangles —
    /// extra repaint, never a stale one.
    ids: super::ids::ElementIds<(u64, u32)>,
    occurrences: HashMap<u64, u32>,
    label_layouts: HashMap<(i32, i32), halley_ui::layout::PaddedLabelLayout>,
}

fn text_key_hash(key: &TextKey) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

impl Default for UiTextRenderer {
    fn default() -> Self {
        Self::new(&halley_config::Font::default())
    }
}

impl UiTextRenderer {
    pub fn new(font: &halley_config::Font) -> Self {
        Self {
            context: None,
            text_system: TextSystem::new(),
            font: font.clone(),
            text: HashMap::new(),
            ids: super::ids::ElementIds::default(),
            occurrences: HashMap::new(),
            label_layouts: HashMap::new(),
        }
    }

    pub fn font_size(&self) -> u16 {
        configured_font_size(&self.font)
    }

    /// Resets per-frame label numbering and ages out unused identities.
    pub fn begin_scene(&mut self) {
        self.occurrences.clear();
        self.ids.advance();
    }

    /// Replace global typography atomically and report whether a frame must
    /// be redrawn. Textures are configuration-specific and cannot survive a
    /// font change.
    pub fn reload_font(&mut self, font: &halley_config::Font) -> bool {
        if self.font == *font {
            return false;
        }
        self.font = font.clone();
        self.text.clear();
        true
    }

    pub fn measure(
        &mut self,
        renderer: &mut GlesRenderer,
        text: &str,
        rgb: [u8; 3],
    ) -> Result<Option<smithay::utils::Size<i32, Buffer>>, Box<dyn Error>> {
        self.measure_with_size(renderer, text, rgb, None)
    }

    pub fn measure_at_size(
        &mut self,
        renderer: &mut GlesRenderer,
        text: &str,
        rgb: [u8; 3],
        size_px: u16,
    ) -> Result<Option<smithay::utils::Size<i32, Buffer>>, Box<dyn Error>> {
        self.measure_with_size(renderer, text, rgb, Some(size_px))
    }

    fn measure_with_size(
        &mut self,
        renderer: &mut GlesRenderer,
        text: &str,
        rgb: [u8; 3],
        size_px: Option<u16>,
    ) -> Result<Option<smithay::utils::Size<i32, Buffer>>, Box<dyn Error>> {
        let Some(key) = self.prepare_key(renderer, text, rgb, size_px)? else {
            return Ok(None);
        };
        let entry = self.text.get_mut(&key).expect("text entry prepared");
        entry.last_used = Instant::now();
        Ok(Some(entry.size))
    }

    /// Taffy uses the existing glyph texture's measurement without another raster.
    pub fn padded_label_layout(
        &mut self,
        size: smithay::utils::Size<i32, Buffer>,
    ) -> Result<halley_ui::layout::PaddedLabelLayout, Box<dyn Error>> {
        let key = (size.w, size.h);
        if let Some(layout) = self.label_layouts.get(&key) {
            return Ok(*layout);
        }
        let layout = halley_ui::layout::PaddedLabelLayout::new(
            halley_ui::Size::new(size.w as f32, size.h as f32),
            halley_ui::Insets::xy(16.0, 8.0),
        )?;
        if self.label_layouts.len() >= 256 {
            self.label_layouts.clear();
        }
        self.label_layouts.insert(key, layout);
        Ok(layout)
    }

    pub fn element(
        &mut self,
        renderer: &mut GlesRenderer,
        origin: Point<i32, Physical>,
        text: &str,
        rgb: [u8; 3],
        alpha: f32,
    ) -> Result<Option<PreparedUiText>, Box<dyn Error>> {
        self.element_with_options(
            renderer,
            origin,
            text,
            rgb,
            alpha,
            TextElementSizing::default(),
        )
    }

    pub fn element_at_size(
        &mut self,
        renderer: &mut GlesRenderer,
        origin: Point<i32, Physical>,
        text: &str,
        rgb: [u8; 3],
        alpha: f32,
        size_px: u16,
    ) -> Result<Option<PreparedUiText>, Box<dyn Error>> {
        self.element_with_options(
            renderer,
            origin,
            text,
            rgb,
            alpha,
            TextElementSizing {
                font_size_px: Some(size_px),
                ..TextElementSizing::default()
            },
        )
    }

    pub fn element_scaled(
        &mut self,
        renderer: &mut GlesRenderer,
        destination: Rectangle<i32, Physical>,
        text: &str,
        rgb: [u8; 3],
        alpha: f32,
    ) -> Result<Option<PreparedUiText>, Box<dyn Error>> {
        self.element_with_options(
            renderer,
            destination.loc,
            text,
            rgb,
            alpha,
            TextElementSizing {
                destination_size: Some(destination.size),
                ..TextElementSizing::default()
            },
        )
    }

    pub fn element_scaled_at_size(
        &mut self,
        renderer: &mut GlesRenderer,
        destination: Rectangle<i32, Physical>,
        text: &str,
        rgb: [u8; 3],
        alpha: f32,
        size_px: u16,
    ) -> Result<Option<PreparedUiText>, Box<dyn Error>> {
        self.element_with_options(
            renderer,
            destination.loc,
            text,
            rgb,
            alpha,
            TextElementSizing {
                font_size_px: Some(size_px),
                destination_size: Some(destination.size),
            },
        )
    }

    fn element_with_options(
        &mut self,
        renderer: &mut GlesRenderer,
        origin: Point<i32, Physical>,
        text: &str,
        rgb: [u8; 3],
        alpha: f32,
        sizing: TextElementSizing,
    ) -> Result<Option<PreparedUiText>, Box<dyn Error>> {
        if alpha <= 0.001 {
            return Ok(None);
        }
        let Some(key) = self.prepare_key(renderer, text, rgb, sizing.font_size_px)? else {
            return Ok(None);
        };
        let context = self.context.as_ref().expect("context prepared").clone();
        let hash = text_key_hash(&key);
        let occurrence = {
            let seen = self.occurrences.entry(hash).or_insert(0);
            let index = *seen;
            *seen += 1;
            index
        };
        let id = self.ids.id((hash, occurrence));
        let entry = self.text.get_mut(&key).expect("text entry prepared");
        entry.last_used = Instant::now();
        let source = Rectangle::<f64, Logical>::new(
            (0.0, 0.0).into(),
            (entry.size.w as f64, entry.size.h as f64).into(),
        );
        let base = TextureRenderElement::from_static_texture(
            id,
            context,
            origin.to_f64(),
            entry.texture.clone(),
            1,
            Transform::Normal,
            Some(alpha.clamp(0.0, 1.0)),
            Some(source),
            // Most callers keep the texture at its rasterized physical size;
            // titlebars may instead provide a camera-scaled destination.
            Some(match sizing.destination_size {
                Some(size) => size.to_logical(1),
                None => entry.size.to_logical(1, Transform::Normal),
            }),
            None,
            Kind::Unspecified,
        );
        Ok(Some(PreparedUiText {
            element: UiTextElement { base },
        }))
    }

    fn prepare_key(
        &mut self,
        renderer: &mut GlesRenderer,
        text: &str,
        rgb: [u8; 3],
        size_px: Option<u16>,
    ) -> Result<Option<TextKey>, Box<dyn Error>> {
        if text.is_empty() {
            return Ok(None);
        }
        self.ensure_context(renderer);
        let now = Instant::now();
        self.text
            .retain(|_, entry| now.saturating_duration_since(entry.last_used) < TEXT_CACHE_TTL);
        let key = TextKey {
            text: text.to_string(),
            family: normalized_family(&self.font),
            size_px: effective_font_size(&self.font, size_px),
            rgb,
        };
        if !self.text.contains_key(&key) {
            let Some(raster) = self.text_system.raster_uncached(
                &UiFont {
                    family: key.family.clone(),
                    size: key.size_px,
                },
                &key.text,
                key.rgb,
            ) else {
                return Ok(None);
            };
            let size = (raster.width, raster.height);
            let texture =
                renderer.import_memory(&raster.pixels, Fourcc::Abgr8888, size.into(), false)?;
            self.text.insert(
                key.clone(),
                TextTexture {
                    texture,
                    size: size.into(),
                    last_used: now,
                },
            );
        }
        Ok(Some(key))
    }

    fn ensure_context(&mut self, renderer: &GlesRenderer) {
        let context = renderer.context_id();
        if self.context.as_ref() == Some(&context) {
            return;
        }
        self.context = Some(context);
        self.text.clear();
    }
}

pub struct PreparedUiText {
    pub element: UiTextElement,
}

#[derive(Debug)]
pub struct UiTextElement {
    base: TextureRenderElement<GlesTexture>,
}

fn normalized_family(font: &halley_config::Font) -> String {
    let family = font.family.trim();
    if family.is_empty() {
        halley_config::Font::default().family
    } else {
        family.to_string()
    }
}

fn configured_font_size(font: &halley_config::Font) -> u16 {
    font.size.max(1)
}

fn effective_font_size(font: &halley_config::Font, override_px: Option<u16>) -> u16 {
    override_px
        .unwrap_or_else(|| configured_font_size(font))
        .max(1)
}

impl Element for UiTextElement {
    fn id(&self) -> &Id {
        self.base.id()
    }
    fn current_commit(&self) -> CommitCounter {
        self.base.current_commit()
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        self.base.geometry(scale)
    }
    fn transform(&self) -> Transform {
        self.base.transform()
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        self.base.src()
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        self.base.damage_since(scale, commit)
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        self.base.opaque_regions(scale)
    }
    fn alpha(&self) -> f32 {
        self.base.alpha()
    }
    fn kind(&self) -> Kind {
        self.base.kind()
    }
}

impl RenderElement<GlesRenderer> for UiTextElement {
    fn draw(
        &self,
        frame: &mut GlesFrame<'_, '_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), GlesError> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::draw(
            &self.base,
            frame,
            src,
            dst,
            damage,
            opaque_regions,
            cache,
        )
    }

    fn underlying_storage(&self, renderer: &mut GlesRenderer) -> Option<UnderlyingStorage<'_>> {
        <TextureRenderElement<GlesTexture> as RenderElement<GlesRenderer>>::underlying_storage(
            &self.base, renderer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_size_is_used_without_per_overlay_offsets() {
        let font = halley_config::Font {
            family: "monospace".to_string(),
            size: 11,
        };
        assert_eq!(configured_font_size(&font), 11);
        assert_eq!(
            configured_font_size(&halley_config::Font {
                family: "monospace".to_string(),
                size: 0,
            }),
            1
        );
        assert_eq!(effective_font_size(&font, None), 11);
        assert_eq!(effective_font_size(&font, Some(18)), 18);
        assert_eq!(effective_font_size(&font, Some(0)), 1);
    }

    #[test]
    fn font_reload_only_invalidates_on_change() {
        let mut renderer = UiTextRenderer::default();
        assert!(!renderer.reload_font(&halley_config::Font::default()));
        assert!(renderer.reload_font(&halley_config::Font {
            family: "sans-serif".to_string(),
            size: 12,
        }));
    }
}
