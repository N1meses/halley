//! Shared screenshot-card geometry for rendering and input, in output coordinates.
use smithay::utils::{Logical, Point, Rectangle, Size};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenshotAction {
    Copy,
    Open,
    Dismiss,
}

pub struct ScreenshotLayout {
    pub card: Rectangle<i32, Logical>,
    pub preview: Rectangle<i32, Logical>,
    pub title: Rectangle<i32, Logical>,
    pub subtitle: Rectangle<i32, Logical>,
    pub copy: Rectangle<i32, Logical>,
    pub open: Rectangle<i32, Logical>,
    pub dismiss: Rectangle<i32, Logical>,
}

/// Positive offsets move right/down, regardless of the selected anchor.
pub fn notification_rect(
    output: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
    config: halley_config::Notifications,
    mix: f32,
) -> Rectangle<i32, Logical> {
    use halley_config::NotificationPosition::*;
    let margin = 24;
    let slide = ((1.0 - mix) * 8.0).round() as i32;
    let x = match config.position {
        TopLeft | BottomLeft => margin,
        TopCenter | BottomCenter => (output.size.w - size.w) / 2,
        TopRight | BottomRight => output.size.w - size.w - margin,
    };
    let y = match config.position {
        TopLeft | TopCenter | TopRight => margin - slide,
        BottomLeft | BottomCenter | BottomRight => output.size.h - size.h - margin + slide,
    };
    Rectangle::new(
        (
            output
                .loc
                .x
                .saturating_add(x)
                .saturating_add(config.offset_x),
            output
                .loc
                .y
                .saturating_add(y)
                .saturating_add(config.offset_y),
        )
            .into(),
        size,
    )
}

pub fn layout(
    output: Rectangle<i32, Logical>,
    config: halley_config::Notifications,
    mix: f32,
    font_size: u16,
) -> ScreenshotLayout {
    let width = (output.size.w - 48).clamp(80, 360);
    let padding = 16;
    let inner = width - padding * 2;
    let line = (f32::from(font_size) * 1.25).ceil() as i32;
    let button_height = line + 16;
    let preview_height = (output.size.h - (line * 2 + button_height + 112)).clamp(24, 164);
    let height = preview_height + line * 2 + button_height + padding * 2 + 28;
    let card = notification_rect(output, (width, height).into(), config, mix);
    let rect = |x, y, w, h| Rectangle::new(card.loc + Point::from((x, y)), (w, h).into());
    let title_y = padding + preview_height + 12;
    let buttons_y = title_y + line * 2 + 16;
    let button_width = (inner - 8) / 2;
    ScreenshotLayout {
        card,
        preview: rect(padding, padding, inner, preview_height),
        title: rect(padding, title_y, inner - 28, line),
        subtitle: rect(padding, title_y + line + 4, inner, line),
        copy: rect(padding, buttons_y, button_width, button_height),
        open: rect(
            padding + button_width + 8,
            buttons_y,
            button_width,
            button_height,
        ),
        dismiss: rect(width - padding - 24, title_y - 4, 24, line + 8),
    }
}

impl ScreenshotLayout {
    pub fn action_at(&self, point: Point<f64, Logical>) -> Option<ScreenshotAction> {
        [
            (&self.copy, ScreenshotAction::Copy),
            (&self.open, ScreenshotAction::Open),
            (&self.dismiss, ScreenshotAction::Dismiss),
        ]
        .into_iter()
        .find_map(|(rect, action)| rect.to_f64().contains(point).then_some(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_shift_every_anchor_and_its_click_targets() {
        use halley_config::NotificationPosition::*;
        for position in [
            TopLeft,
            TopCenter,
            TopRight,
            BottomLeft,
            BottomCenter,
            BottomRight,
        ] {
            let output = Rectangle::new((1280, -100).into(), (1920, 1080).into());
            let config = halley_config::Notifications {
                position,
                ..Default::default()
            };
            let baseline = layout(output, config, 1.0, 16);
            let moved = layout(
                output,
                halley_config::Notifications {
                    offset_x: -37,
                    offset_y: 48,
                    ..config
                },
                1.0,
                16,
            );
            assert_eq!(moved.card.loc - baseline.card.loc, Point::from((-37, 48)));
            for (rect, action) in [
                (moved.copy, ScreenshotAction::Copy),
                (moved.open, ScreenshotAction::Open),
                (moved.dismiss, ScreenshotAction::Dismiss),
            ] {
                let center = rect.loc.to_f64()
                    + Point::from((rect.size.w as f64 / 2.0, rect.size.h as f64 / 2.0));
                assert_eq!(moved.action_at(center), Some(action));
            }
        }
    }
}
