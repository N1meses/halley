use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::{Bind, Color32F, ExportMem, Offscreen, Texture};
use smithay::output::{Mode, PhysicalProperties, Subpixel};
use smithay::utils::Transform;

use super::*;
use crate::render::effects::backdrop_blur::BackdropBlurRenderer;
use crate::render::effects::shadow::ShadowRenderer;
use crate::render::node::NodeRenderer;
use crate::render::text::UiTextRenderer;
use crate::session::tty::FrameDemand;
use crate::shell::overlay::{OverlayManager, OverlaySnapshot};

const SIZE: (i32, i32) = (641, 481);

fn output(name: &str) -> Output {
    let output = Output::new(
        name.into(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "test".into(),
            model: "test".into(),
            serial_number: name.into(),
        },
    );
    output.change_current_state(
        Some(Mode {
            size: SIZE.into(),
            refresh: 60_000,
        }),
        Some(Transform::Normal),
        Some(smithay::output::Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output
}

struct SceneResources {
    nodes: NodeRenderer,
    text: UiTextRenderer,
    blur: BackdropBlurRenderer,
    shadow: ShadowRenderer,
    background: [Id; 3],
}

impl SceneResources {
    fn new() -> Self {
        Self {
            nodes: NodeRenderer::default(),
            text: UiTextRenderer::new(&halley_config::Font::default()),
            blur: BackdropBlurRenderer::default(),
            shadow: ShadowRenderer::default(),
            background: std::array::from_fn(|_| Id::new()),
        }
    }

    fn scene(
        &mut self,
        renderer: &mut GlesRenderer,
        output: &Output,
        snapshot: OverlaySnapshot,
        config: &halley_config::Overlays,
        label_mix: Option<f32>,
    ) -> Vec<SceneElement> {
        self.nodes.begin_scene(&output.name());
        self.text.begin_scene();
        self.blur.begin_scene(&output.name());
        let mut elements = crate::render::overlays::shell::elements(
            renderer,
            Rectangle::from_size(SIZE.into()),
            snapshot,
            config,
            &mut self.nodes,
            &mut self.text,
        )
        .unwrap();
        if let Some(hover_mix) = label_mix {
            elements.extend(
                nodes::landmark_label_elements_avoiding(
                    renderer,
                    &mut self.nodes,
                    &mut self.text,
                    nodes::LandmarkLabel {
                        center: (220, 220),
                        marker_side: 64,
                        output_size: SIZE,
                        text: "Work cluster",
                        shape: halley_config::NodeShape::Squircle,
                        fill: (0.15, 0.18, 0.22),
                        ring: (0.6, 0.3, 0.1),
                        hover_mix,
                        alpha: 0.9,
                    },
                    &[],
                )
                .unwrap(),
            );
        }
        append_compositor_overlay_blur(
            renderer,
            output,
            OverlayEffectStyle {
                output_size: SIZE.into(),
                identity: "animation-test",
                // Remove procedural noise so the pixel comparison isolates
                // stale fade/geometry/shadow damage, with one rounding step.
                blur: halley_config::Blur {
                    noise: 0.0,
                    ..Default::default()
                },
                shadow: halley_config::Shadows::default().overlay,
            },
            &mut self.blur,
            &mut self.shadow,
            &mut elements,
        )
        .unwrap();
        for (id, rect, color) in [
            (
                &self.background[0],
                Rectangle::new((210, 0).into(), (100, SIZE.1).into()),
                Color32F::new(0.1, 0.7, 0.2, 1.0),
            ),
            (
                &self.background[1],
                Rectangle::new((0, 320).into(), (SIZE.0, 45).into()),
                Color32F::new(0.8, 0.2, 0.1, 1.0),
            ),
            (
                &self.background[2],
                Rectangle::from_size(SIZE.into()),
                Color32F::new(0.1, 0.2, 0.4, 1.0),
            ),
        ] {
            elements.push(SceneElement::Border(crate::render::solid_color_element(
                id.clone(),
                rect,
                color,
            )));
        }
        elements
    }
}

fn read_pixels(
    renderer: &mut GlesRenderer,
    target: &mut GlesTexture,
    tracker: &mut OutputDamageTracker,
    scene: &[SceneElement],
    age: usize,
) -> (Vec<u8>, Vec<Rectangle<i32, Physical>>) {
    let size = target.size();
    let mut fbo = renderer.bind(target).unwrap();
    let result = tracker
        .render_output(renderer, &mut fbo, age, scene, Color32F::BLACK)
        .unwrap();
    result.sync.wait().unwrap();
    let damage = result.damage.cloned().unwrap_or_default();
    let mapping = renderer
        .copy_framebuffer(&fbo, Rectangle::from_size(size), Fourcc::Abgr8888)
        .unwrap();
    (renderer.map_texture(&mapping).unwrap().to_vec(), damage)
}

#[test]
#[ignore = "requires surfaceless GLES; run with LIBGL_ALWAYS_SOFTWARE=1 and --ignored"]
fn local_animation_pixels_match_full_repaint_with_blur_shadows_and_reused_buffers() {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay) }.unwrap();
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
    let actual_output = output("actual");
    let reference_output = output("reference");
    let mut comparisons = 0;
    for buffers in 1..=3 {
        for case in ["notification", "zoom", "cluster-label"] {
            let config = halley_config::Overlays {
                zoom_indicator: halley_config::ZoomIndicator {
                    hold_duration_ms: 120,
                    fade_duration_ms: 180,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut overlays = OverlayManager::default();
            if case == "notification" {
                overlays.show_config_error("actual".into(), 300, Duration::ZERO);
            } else if case == "zoom" {
                overlays.show_zoom_indicator(
                    "actual",
                    0.75,
                    &config.zoom_indicator,
                    Duration::ZERO,
                );
            }
            let clusters = crate::clusters::ClusterSystem::new(
                halley_config::Clusters::default(),
                halley_config::ClusterAnimation::default(),
            );
            let mut actual_resources = SceneResources::new();
            let mut reference_resources = SceneResources::new();
            let mut targets: Vec<GlesTexture> = (0..buffers)
                .map(|_| {
                    renderer
                        .create_buffer(Fourcc::Abgr8888, SIZE.into())
                        .unwrap()
                })
                .collect();
            let mut reference = renderer
                .create_buffer(Fourcc::Abgr8888, SIZE.into())
                .unwrap();
            let mut tracker = OutputDamageTracker::new(SIZE, 1.0, Transform::Normal);
            let mut saw_partial = false;
            let mut saw_settled = false;
            let mut saw_visible = false;
            let ticks = if case == "cluster-label" { 150 } else { 42 };
            for tick in 0..ticks {
                let now = Duration::from_millis(tick as u64 * 15);
                overlays.wakeup(now);
                let label_mix = (case == "cluster-label").then(|| {
                    clusters.label_hover_mix(halley_core::cluster::ClusterId::new(1), tick < 90)
                });
                let demand = FrameDemand::new(
                    false,
                    overlays.animating_on_output("actual", now) || label_mix.is_some(),
                    false,
                );
                assert!(!demand.force_full_repaint);
                let snapshot = overlays.snapshot("actual", now);
                let scene = actual_resources.scene(
                    &mut renderer,
                    &actual_output,
                    snapshot.clone(),
                    &config,
                    label_mix,
                );
                saw_visible |= scene
                    .iter()
                    .any(|e| matches!(e, SceneElement::UiText(_)) && e.alpha() > 0.1);
                let age = if tick < buffers { 0 } else { buffers };
                let (actual, damage) = read_pixels(
                    &mut renderer,
                    &mut targets[tick % buffers],
                    &mut tracker,
                    &scene,
                    age,
                );
                // Separate effect caches prevent the reference repaint from
                // refreshing or repairing the incrementally rendered scene.
                let reference_scene = reference_resources.scene(
                    &mut renderer,
                    &reference_output,
                    snapshot,
                    &config,
                    label_mix,
                );
                let (expected, _) = read_pixels(
                    &mut renderer,
                    &mut reference,
                    &mut OutputDamageTracker::new(SIZE, 1.0, Transform::Normal),
                    &reference_scene,
                    0,
                );
                let max_error = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    max_error <= 1,
                    "case={case} buffers={buffers} tick={tick} max_error={max_error}"
                );
                if tick >= buffers {
                    assert!(
                        damage.iter().all(|r| r.size.w * r.size.h < SIZE.0 * SIZE.1),
                        "local {case} must not repaint the entire output"
                    );
                    saw_partial |= !damage.is_empty();
                    saw_settled |= damage.is_empty();
                }
                comparisons += 1;
            }
            assert!(
                saw_visible && saw_partial && saw_settled,
                "case={case} buffers={buffers}"
            );
        }
    }
    eprintln!(
        "{comparisons} local-animation GLES comparisons passed, buffer ages 1-3, max permitted channel error=1/255"
    );
}

#[test]
#[ignore = "requires surfaceless GLES; run with LIBGL_ALWAYS_SOFTWARE=1 and --ignored"]
fn screenshot_preview_pixels_and_fade_match_full_repaint() {
    use halley_config::NotificationPosition::*;
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay) }.unwrap();
    let context = EGLContext::new(&display).unwrap();
    let mut renderer = unsafe { GlesRenderer::new(context) }.unwrap();
    let actual_output = output("actual");
    let reference_output = output("reference");
    let directory =
        std::env::temp_dir().join(format!("halley-preview-pixels-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("test.png");
    let pixels = [30, 160, 210, 255].repeat(160 * 90);
    image::save_buffer(&path, &pixels, 160, 90, image::ColorType::Rgba8).unwrap();
    let preview = crate::capture::preview::prepare(path.clone(), 160, 90, pixels).unwrap();
    let mut comparisons = 0;
    for position in [
        TopLeft,
        TopCenter,
        TopRight,
        BottomLeft,
        BottomCenter,
        BottomRight,
    ] {
        for buffers in 1..=3 {
            let config = halley_config::Overlays {
                notifications: halley_config::Notifications {
                    position,
                    offset_x: -12,
                    offset_y: 16,
                    success_duration_ms: 300,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut overlays = OverlayManager::default();
            overlays.show_screenshot_saved("actual".into(), &directory, 300, Duration::ZERO);
            overlays.attach_screenshot_preview(Some(preview.clone()));
            let mut actual_resources = SceneResources::new();
            let mut reference_resources = SceneResources::new();
            let font = halley_config::Font {
                size: 16,
                ..Default::default()
            };
            actual_resources.text.reload_font(&font);
            reference_resources.text.reload_font(&font);
            let mut targets: Vec<GlesTexture> = (0..buffers)
                .map(|_| {
                    renderer
                        .create_buffer(Fourcc::Abgr8888, SIZE.into())
                        .unwrap()
                })
                .collect();
            let mut reference = renderer
                .create_buffer(Fourcc::Abgr8888, SIZE.into())
                .unwrap();
            let mut tracker = OutputDamageTracker::new(SIZE, 1.0, Transform::Normal);
            for tick in 0..42 {
                let now = Duration::from_millis(tick as u64 * 15);
                overlays.wakeup(now);
                let snapshot = overlays.snapshot("actual", now);
                let scene = actual_resources.scene(
                    &mut renderer,
                    &actual_output,
                    snapshot.clone(),
                    &config,
                    None,
                );
                let age = if tick < buffers { 0 } else { buffers };
                let (actual, _) = read_pixels(
                    &mut renderer,
                    &mut targets[tick % buffers],
                    &mut tracker,
                    &scene,
                    age,
                );
                if tick == 12 {
                    let layout = crate::shell::screenshot::layout(
                        Rectangle::from_size(SIZE.into()),
                        config.notifications,
                        1.0,
                        font.size,
                    );
                    let x = layout.preview.loc.x + layout.preview.size.w / 2;
                    let y = layout.preview.loc.y + layout.preview.size.h / 2;
                    let start = ((y * SIZE.0 + x) * 4) as usize;
                    assert_eq!(
                        &actual[start..start + 4],
                        &[30, 160, 210, 255],
                        "preview channels and position"
                    );
                    if position == TopRight
                        && buffers == 1
                        && let Ok(path) = std::env::var("HALLEY_SCREENSHOT_TEST_RENDER")
                    {
                        image::save_buffer(
                            path,
                            &actual,
                            SIZE.0 as u32,
                            SIZE.1 as u32,
                            image::ColorType::Rgba8,
                        )
                        .unwrap();
                    }
                }
                let reference_scene = reference_resources.scene(
                    &mut renderer,
                    &reference_output,
                    snapshot,
                    &config,
                    None,
                );
                let (expected, _) = read_pixels(
                    &mut renderer,
                    &mut reference,
                    &mut OutputDamageTracker::new(SIZE, 1.0, Transform::Normal),
                    &reference_scene,
                    0,
                );
                let max_error = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                assert!(
                    max_error <= 1,
                    "position={position:?} buffers={buffers} tick={tick} max_error={max_error}"
                );
                comparisons += 1;
            }
            assert!(
                overlays
                    .snapshot("actual", Duration::from_millis(600))
                    .notification
                    .is_none()
            );
        }
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
    eprintln!("screenshot preview: {comparisons} rendered frame comparisons");
}
