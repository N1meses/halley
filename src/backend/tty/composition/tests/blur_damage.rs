use std::cell::Cell;

use super::*;

#[derive(Clone)]
struct CachedBlur {
    id: Id,
    rect: Rectangle<i32, Physical>,
    cache: RasterTexture,
    captures: Rc<Cell<usize>>,
}

impl CachedBlur {
    fn new(id: Id) -> Self {
        Self {
            id,
            rect: Rectangle::from_size((100, 80).into()),
            cache: RasterTexture::new((100, 80)),
            captures: Rc::new(Cell::new(0)),
        }
    }
}

impl Element for CachedBlur {
    fn id(&self) -> &Id {
        &self.id
    }
    fn current_commit(&self) -> CommitCounter {
        CommitCounter::from(0)
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_size((100.0, 80.0).into())
    }
    fn geometry(&self, _: Scale<f64>) -> Rectangle<i32, Physical> {
        self.rect
    }
    fn is_framebuffer_effect(&self) -> bool {
        true
    }
}

impl RenderElement<RasterRenderer> for CachedBlur {
    fn capture_framebuffer(
        &self,
        frame: &mut RasterFrame,
        _: Rectangle<f64, Buffer>,
        _: Rectangle<i32, Physical>,
        _: &UserDataMap,
    ) -> Result<(), std::io::Error> {
        self.captures.set(self.captures.get() + 1);
        // A real spatial dependency, rather than a damage-only mock: every
        // cached pixel averages neighboring pixels behind this stack position.
        for y in 0..80 {
            for x in 0..100 {
                let mut sum = [0.0; 4];
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let p = frame
                            .target
                            .pixel((x + dx).clamp(0, 99), (y + dy).clamp(0, 79));
                        for (sum, value) in sum.iter_mut().zip([p.r(), p.g(), p.b(), p.a()]) {
                            *sum += value;
                        }
                    }
                }
                self.cache.paint(
                    Rectangle::new((x, y).into(), (1, 1).into()),
                    Color32F::new(sum[0] / 9.0, sum[1] / 9.0, sum[2] / 9.0, sum[3] / 9.0),
                    false,
                );
            }
        }
        Ok(())
    }
    fn draw(
        &self,
        frame: &mut RasterFrame,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        _: &[Rectangle<i32, Physical>],
        _: Option<&UserDataMap>,
    ) -> Result<(), std::io::Error> {
        frame.render_texture_from_to(&self.cache, src, dst, damage, &[], Transform::Normal, 1.0)
    }
}

enum TestElement {
    Solid(SolidColorRenderElement),
    Blur(CachedBlur),
}

impl Element for TestElement {
    fn id(&self) -> &Id {
        match self {
            Self::Solid(s) => s.id(),
            Self::Blur(b) => b.id(),
        }
    }
    fn current_commit(&self) -> CommitCounter {
        match self {
            Self::Solid(s) => s.current_commit(),
            Self::Blur(b) => b.current_commit(),
        }
    }
    fn src(&self) -> Rectangle<f64, Buffer> {
        match self {
            Self::Solid(s) => s.src(),
            Self::Blur(b) => b.src(),
        }
    }
    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        match self {
            Self::Solid(s) => s.geometry(scale),
            Self::Blur(b) => b.geometry(scale),
        }
    }
    fn damage_since(
        &self,
        scale: Scale<f64>,
        commit: Option<CommitCounter>,
    ) -> DamageSet<i32, Physical> {
        match self {
            Self::Solid(s) => s.damage_since(scale, commit),
            Self::Blur(b) => b.damage_since(scale, commit),
        }
    }
    fn opaque_regions(&self, scale: Scale<f64>) -> OpaqueRegions<i32, Physical> {
        match self {
            Self::Solid(s) => s.opaque_regions(scale),
            Self::Blur(b) => b.opaque_regions(scale),
        }
    }
    fn is_framebuffer_effect(&self) -> bool {
        matches!(self, Self::Blur(_))
    }
}

impl RenderElement<RasterRenderer> for TestElement {
    fn capture_framebuffer(
        &self,
        frame: &mut RasterFrame,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        cache: &UserDataMap,
    ) -> Result<(), std::io::Error> {
        match self {
            Self::Blur(b) => b.capture_framebuffer(frame, src, dst, cache),
            Self::Solid(_) => unreachable!(),
        }
    }
    fn draw(
        &self,
        frame: &mut RasterFrame,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque: &[Rectangle<i32, Physical>],
        cache: Option<&UserDataMap>,
    ) -> Result<(), std::io::Error> {
        match self {
            Self::Blur(b) => b.draw(frame, src, dst, damage, opaque, cache),
            Self::Solid(s) => <SolidColorRenderElement as RenderElement<RasterRenderer>>::draw(
                s, frame, src, dst, damage, opaque, cache,
            ),
        }
    }
}

fn assert_pixels_match_full_repaint(
    tracker: &mut OutputDamageTracker,
    renderer: &mut RasterRenderer,
    target: &mut RasterTexture,
    scene: &[TestElement],
) -> Vec<Rectangle<i32, Physical>> {
    let damage = tracker
        .render_output(renderer, target, 1, scene, Color32F::BLACK)
        .unwrap()
        .damage
        .cloned()
        .unwrap_or_default();
    let full_scene: Vec<TestElement> = scene
        .iter()
        .map(|e| match e {
            TestElement::Solid(s) => TestElement::Solid(s.clone()),
            TestElement::Blur(b) => TestElement::Blur(CachedBlur::new(b.id.clone())),
        })
        .collect();
    let mut full = RasterTexture::new((100, 80));
    OutputDamageTracker::new((100, 80), 1.0, Transform::Normal)
        .render_output(renderer, &mut full, 0, &full_scene, Color32F::BLACK)
        .unwrap();
    assert_eq!(*target.pixels.borrow(), *full.pixels.borrow());
    damage
}

#[test]
fn foreground_replacement_motion_and_removal_reuse_backdrop_pixels() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    let cursor = Id::new();
    for (tick, foreground) in [
        Some(solid(&cursor, (5, 5, 8, 8), 0, WHITE)),
        Some(solid(&cursor, (25, 5, 8, 8), 0, WHITE)),
        Some(solid(&Id::new(), (25, 5, 8, 8), 0, GREEN)),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        let mut scene = Vec::new();
        if let Some(foreground) = foreground {
            scene.push(TestElement::Solid(foreground));
        }
        scene.push(TestElement::Blur(blur.clone()));
        scene.push(TestElement::Solid(background.clone()));
        let damage =
            assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
        assert_eq!(
            blur.captures.get(),
            1,
            "foreground frame {tick} recaptured blur"
        );
        if tick != 0 {
            assert!(damage.iter().all(|r| r.size.w * r.size.h < 8000));
        }
    }
}

#[test]
fn background_removal_and_crossing_the_effect_refresh_the_cache() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    let moving = solid(&Id::new(), (20, 20, 20, 20), 0, GREEN);
    for (tick, position) in [1, 0, 1, 2].into_iter().enumerate() {
        let mut scene = vec![TestElement::Blur(blur.clone())];
        if position != 2 {
            scene.insert(position, TestElement::Solid(moving.clone()));
        }
        scene.push(TestElement::Solid(background.clone()));
        assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
        assert_eq!(blur.captures.get(), tick + 1);
    }
}

#[test]
fn reordering_surfaces_behind_the_effect_updates_blurred_pixels() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let a = solid(&Id::new(), (10, 10, 35, 35), 0, GREEN);
    let b = solid(&Id::new(), (20, 20, 35, 35), 0, WHITE);
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    for reverse in [false, true, false] {
        let (a, b) = if reverse { (&b, &a) } else { (&a, &b) };
        let scene = [
            TestElement::Blur(blur.clone()),
            TestElement::Solid(a.clone()),
            TestElement::Solid(b.clone()),
            TestElement::Solid(background.clone()),
        ];
        assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
    }
    assert_eq!(blur.captures.get(), 3);
}

#[test]
fn fully_occluded_effect_recaptures_when_revealed() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    let cover = solid(&Id::new(), (0, 0, 100, 80), 0, WHITE);
    for covered in [false, true, false] {
        let mut scene = vec![
            TestElement::Blur(blur.clone()),
            TestElement::Solid(background.clone()),
        ];
        if covered {
            scene.insert(0, TestElement::Solid(cover.clone()));
        }
        assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
    }
    assert_eq!(blur.captures.get(), 2);
}

#[test]
fn opacity_changes_behind_the_effect_invalidate_even_without_a_new_commit() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let content = Id::new();
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    for color in [GREEN, Color32F::new(0.0, 0.5, 0.0, 0.5), GREEN] {
        let scene = [
            TestElement::Blur(blur.clone()),
            TestElement::Solid(solid(&content, (10, 10, 20, 20), 0, color)),
            TestElement::Solid(background.clone()),
        ];
        assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
    }
    assert_eq!(blur.captures.get(), 3);
}

#[test]
fn a_foreground_cover_cannot_cull_background_pixels_needed_by_blur() {
    let mut renderer = RasterRenderer::default();
    let mut tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
    let mut target = RasterTexture::new((100, 80));
    let blur = CachedBlur::new(Id::new());
    let content = Id::new();
    let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
    let cover = solid(&Id::new(), (20, 20, 10, 10), 0, WHITE);
    for (tick, color) in [GREEN, WHITE, GREEN].into_iter().enumerate() {
        let scene = [
            TestElement::Solid(cover.clone()),
            TestElement::Blur(blur.clone()),
            TestElement::Solid(solid(&content, (20, 20, 10, 10), tick, color)),
            TestElement::Solid(background.clone()),
        ];
        assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
        let expected = Color32F::new(
            (RED.r() * 6.0 + color.r() * 3.0) / 9.0,
            (RED.g() * 6.0 + color.g() * 3.0) / 9.0,
            (RED.b() * 6.0 + color.b() * 3.0) / 9.0,
            1.0,
        );
        assert_eq!(
            target.pixel(19, 24),
            expected,
            "hidden content contributes to neighboring blur"
        );
    }
    assert_eq!(blur.captures.get(), 3);
    let scene = [
        TestElement::Solid(solid(&Id::new(), (20, 20, 10, 10), 0, WHITE)),
        TestElement::Blur(blur.clone()),
        TestElement::Solid(solid(&content, (20, 20, 10, 10), 2, GREEN)),
        TestElement::Solid(background),
    ];
    assert_pixels_match_full_repaint(&mut tracker, &mut renderer, &mut target, &scene);
    assert_eq!(
        blur.captures.get(),
        3,
        "foreground replacement keeps hidden background cache valid"
    );
}
