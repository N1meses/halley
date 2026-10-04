use smithay::utils::{Physical, Rectangle, Size};

use super::{BlurPatch, level_size};

type Rect = Rectangle<i32, Physical>;

#[derive(Debug)]
pub(super) struct BlurPass {
    pub source: Size<i32, Physical>,
    pub target: Size<i32, Physical>,
    pub up: bool,
    pub regions: Vec<Rect>,
}

#[derive(Debug)]
pub(super) struct BlurPlan {
    pub capture: Vec<Rect>,
    pub passes: Vec<BlurPass>,
}

fn passes(size: Size<i32, Physical>, levels: u32) -> Vec<BlurPass> {
    let mut result = Vec::new();
    let mut source = size;
    for level in 0..levels {
        let target = level_size(size, level);
        result.push(BlurPass {
            source,
            target,
            up: false,
            regions: Vec::new(),
        });
        source = target;
    }
    for level in (0..levels).rev() {
        let target = if level == 0 {
            size
        } else {
            level_size(size, level - 1)
        };
        result.push(BlurPass {
            source,
            target,
            up: true,
            regions: Vec::new(),
        });
        source = target;
    }
    result
}

pub(super) fn padding(size: Size<i32, Physical>, levels: u32, offset: f32) -> i32 {
    // Bound every texture lookup in original-output pixels, including linear
    // filtering and rounding at odd pyramid dimensions. This is deliberately
    // conservative; the per-pass plan below uses actual dimension ratios.
    let radius: f64 = passes(size, levels)
        .iter()
        .map(|p| {
            let reach = f64::from(offset) * if p.up { 1.0 } else { 0.5 } + 2.0;
            reach
                * (f64::from(size.w) / f64::from(p.source.w))
                    .max(f64::from(size.h) / f64::from(p.source.h))
        })
        .sum();
    radius.ceil() as i32 + 2
}

pub(super) fn expand(rect: Rect, amount: i32) -> Rect {
    Rectangle::new(
        (rect.loc.x - amount, rect.loc.y - amount).into(),
        (rect.size.w + amount * 2, rect.size.h + amount * 2).into(),
    )
}

fn unique_regions(rects: impl IntoIterator<Item = Rect>) -> Vec<Rect> {
    // Rectangles may overlap: overwrite-only filter passes remain correct.
    // Remove containment to avoid repeating large captures for small patches.
    let mut result: Vec<Rect> = Vec::new();
    for rect in rects {
        if rect.is_empty() || result.iter().any(|r| r.contains_rect(rect)) {
            continue;
        }
        result.retain(|r| !rect.contains_rect(*r));
        result.push(rect);
    }
    result
}

fn source_region(
    rect: Rect,
    source: Size<i32, Physical>,
    target: Size<i32, Physical>,
    reach: f64,
) -> Rect {
    let axis = |start: i32, length: i32, source: i32, target: i32| {
        let ratio = f64::from(source) / f64::from(target);
        let low = (f64::from(start) * ratio - reach - 1.0).floor().max(0.0) as i32;
        let high = (f64::from(start + length) * ratio + reach + 1.0)
            .ceil()
            .min(f64::from(source)) as i32;
        (low, (high - low).max(0))
    };
    let (x, w) = axis(rect.loc.x, rect.size.w, source.w, target.w);
    let (y, h) = axis(rect.loc.y, rect.size.h, source.h, target.h);
    Rectangle::new((x, y).into(), (w, h).into())
}

pub(super) fn plan_for_outputs(
    size: Size<i32, Physical>,
    levels: u32,
    offset: f32,
    outputs: Vec<Rect>,
) -> BlurPlan {
    let mut passes = passes(size, levels);
    let mut required = unique_regions(outputs);
    // Walk backwards from pixels actually displayed. Every scratch pixel a
    // pass can sample is regenerated in its predecessor, so scratch textures
    // can persist and be shared across different blur stack positions safely.
    for pass in passes.iter_mut().rev() {
        pass.regions = required;
        let reach = f64::from(offset) * if pass.up { 1.0 } else { 0.5 };
        required = unique_regions(
            pass.regions
                .iter()
                .map(|r| source_region(*r, pass.source, pass.target, reach)),
        );
    }
    BlurPlan {
        capture: required,
        passes,
    }
}

pub(super) fn plan(
    size: Size<i32, Physical>,
    levels: u32,
    offset: f32,
    patches: &[BlurPatch],
    changed: &[Rect],
) -> BlurPlan {
    let bounds = Rectangle::from_size(size);
    let radius = padding(size, levels, offset);
    let outputs = unique_regions(changed.iter().flat_map(|damage| {
        let affected = expand(*damage, radius);
        patches
            .iter()
            .filter(|p| p.alpha > 0.0)
            .filter_map(move |p| {
                p.rect
                    .intersection(affected)
                    .and_then(|r| r.intersection(bounds))
            })
    }));
    plan_for_outputs(size, levels, offset, outputs)
}
