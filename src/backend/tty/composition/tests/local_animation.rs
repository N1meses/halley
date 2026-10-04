use super::*;
use std::time::Duration;

use crate::shell::overlay::OverlayManager;

#[test]
fn local_fade_motion_and_removal_match_full_pixels_on_both_gpu_paths_with_buffer_ages() {
    for buffers in 1..=3 {
        let mut renderer = RasterRenderer::default();
        let (mut frame, _) = compose(&mut renderer);
        let mut primary_tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
        let mut secondary_tracker = OutputDamageTracker::new((100, 80), 1.0, Transform::Normal);
        let mut primary: Vec<_> = (0..buffers)
            .map(|_| RasterTexture::new((100, 80)))
            .collect();
        let mut secondary: Vec<_> = (0..buffers)
            .map(|_| RasterTexture::new((100, 80)))
            .collect();
        let background = solid(&Id::new(), (0, 0, 100, 80), 0, RED);
        let card = Id::new();
        let mut saw_repaint = false;
        let mut saw_unchanged = false;
        for tick in 0..24 {
            let active = (3..18).contains(&tick);
            let demand = FrameDemand::new(false, active, false);
            assert_eq!(demand.keep_redrawing, active);
            assert_eq!(demand.force_full_repaint, active);
            let mut scene = Vec::new();
            if active {
                let alpha = ((tick - 2).min(18 - tick) as f32 / 5.0).min(1.0);
                // Keep the identity and commit fixed: geometry/alpha tracking
                // must still catch the fade and the notification's slide.
                scene.push(solid(
                    &card,
                    (12, 6 + (alpha * 5.0) as i32, 24, 12),
                    0,
                    Color32F::new(0.0, alpha, 0.0, alpha),
                ));
            }
            scene.push(background.clone());
            let age = if tick < buffers || demand.force_full_repaint {
                0
            } else {
                buffers
            };
            let damage = primary_tracker
                .render_output(
                    &mut renderer,
                    &mut primary[tick % buffers],
                    age,
                    &scene,
                    Color32F::BLACK,
                )
                .unwrap()
                .damage
                .cloned()
                .unwrap_or_default();
            frame
                .render(
                    &mut renderer,
                    &scene,
                    Color32F::BLACK,
                    demand.force_full_repaint,
                )
                .unwrap();
            let secondary_damage = present(
                &mut renderer,
                &mut secondary_tracker,
                &mut secondary[tick % buffers],
                &frame,
                age,
            );
            let mut reference = RasterTexture::new((100, 80));
            OutputDamageTracker::new((100, 80), 1.0, Transform::Normal)
                .render_output(&mut renderer, &mut reference, 0, &scene, Color32F::BLACK)
                .unwrap();
            assert_eq!(
                *primary[tick % buffers].pixels.borrow(),
                *reference.pixels.borrow()
            );
            assert_eq!(
                *secondary[tick % buffers].pixels.borrow(),
                *reference.pixels.borrow()
            );
            if tick >= buffers {
                if demand.force_full_repaint {
                    assert!(damage.iter().any(|rect| rect.size.w * rect.size.h == 8000));
                    assert!(
                        secondary_damage
                            .iter()
                            .any(|rect| rect.size.w * rect.size.h == 8000)
                    );
                }
                saw_repaint |= !damage.is_empty() && !secondary_damage.is_empty();
                saw_unchanged |= damage.is_empty() && secondary_damage.is_empty();
            }
        }
        assert!(saw_repaint && saw_unchanged);
    }
}

#[test]
fn notifications_do_not_keep_the_other_output_redrawing_or_damage_its_reused_buffers() {
    let mut renderer = RasterRenderer::default();
    let mut overlays = OverlayManager::default();
    let mut frames: Vec<_> = (0..2).map(|_| compose(&mut renderer).0).collect();
    let mut primary_trackers: Vec<_> = (0..2)
        .map(|_| OutputDamageTracker::new((100, 80), 1.0, Transform::Normal))
        .collect();
    let mut secondary_trackers: Vec<_> = (0..2)
        .map(|_| OutputDamageTracker::new((100, 80), 1.0, Transform::Normal))
        .collect();
    let mut primary: Vec<Vec<_>> = (0..2)
        .map(|_| (0..3).map(|_| RasterTexture::new((100, 80))).collect())
        .collect();
    let mut secondary = primary
        .iter()
        .map(|_| {
            (0..3)
                .map(|_| RasterTexture::new((100, 80)))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let background: Vec<_> = (0..2)
        .map(|_| solid(&Id::new(), (0, 0, 100, 80), 0, RED))
        .collect();
    let cards: [Id; 2] = std::array::from_fn(|_| Id::new());
    let mut saw_fade = [false; 2];
    for tick in 0..100 {
        let now = Duration::from_millis(tick as u64 * 15);
        if tick == 0 || tick == 42 {
            let owner = if tick == 0 { "DP-1" } else { "DP-2" };
            overlays.show_config_error(owner.into(), 240, now);
        }
        overlays.wakeup(now);
        for (index, output) in ["DP-1", "DP-2"].into_iter().enumerate() {
            let demand = FrameDemand::new(false, overlays.animating_on_output(output, now), false);
            assert_eq!(
                demand.force_full_repaint,
                overlays.animating_on_output(output, now)
            );
            let snapshot = overlays.snapshot(output, now);
            let mut scene = Vec::new();
            if let Some(notification) = snapshot.notification {
                let alpha = notification.mix;
                scene.push(solid(
                    &cards[index],
                    (12, 6, 24, 12),
                    0,
                    Color32F::new(0.0, alpha, 0.0, alpha),
                ));
            }
            scene.push(background[index].clone());
            let age = if tick < 3 || demand.force_full_repaint {
                0
            } else {
                3
            };
            let damage = primary_trackers[index]
                .render_output(
                    &mut renderer,
                    &mut primary[index][tick % 3],
                    age,
                    &scene,
                    Color32F::BLACK,
                )
                .unwrap()
                .damage
                .cloned()
                .unwrap_or_default();
            frames[index]
                .render(
                    &mut renderer,
                    &scene,
                    Color32F::BLACK,
                    demand.force_full_repaint,
                )
                .unwrap();
            let secondary_damage = present(
                &mut renderer,
                &mut secondary_trackers[index],
                &mut secondary[index][tick % 3],
                &frames[index],
                age,
            );
            let mut reference = RasterTexture::new((100, 80));
            OutputDamageTracker::new((100, 80), 1.0, Transform::Normal)
                .render_output(&mut renderer, &mut reference, 0, &scene, Color32F::BLACK)
                .unwrap();
            assert_eq!(
                *primary[index][tick % 3].pixels.borrow(),
                *reference.pixels.borrow()
            );
            assert_eq!(
                *secondary[index][tick % 3].pixels.borrow(),
                *reference.pixels.borrow()
            );
            let unrelated = (index == 1 && tick < 42) || (index == 0 && tick >= 42);
            if unrelated {
                assert!(!demand.keep_redrawing, "unrelated {output} tick={tick}");
                if tick >= 3 {
                    assert!(
                        damage.is_empty() && secondary_damage.is_empty(),
                        "unrelated {output} tick={tick}"
                    );
                }
            } else if demand.keep_redrawing && tick >= 3 {
                assert!(damage.iter().any(|r| r.size.w * r.size.h == 100 * 80));
                assert!(
                    secondary_damage
                        .iter()
                        .any(|r| r.size.w * r.size.h == 100 * 80)
                );
                saw_fade[index] |= !damage.is_empty();
            }
        }
    }
    assert!(saw_fade.into_iter().all(|saw| saw));
}
