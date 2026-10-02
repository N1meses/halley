use super::*;

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
        let mut saw_partial = false;
        let mut saw_unchanged = false;
        for tick in 0..24 {
            let active = (3..18).contains(&tick);
            let demand = FrameDemand::new(false, active, false);
            assert_eq!(demand.keep_redrawing, active);
            assert!(!demand.force_full_repaint);
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
            let age = if tick < buffers { 0 } else { buffers };
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
                let local = Rectangle::new((12, 6).into(), (24, 17).into());
                assert!(damage.iter().all(|rect| local.contains_rect(*rect)));
                assert!(
                    secondary_damage
                        .iter()
                        .all(|rect| local.contains_rect(*rect))
                );
                saw_partial |= !damage.is_empty() && !secondary_damage.is_empty();
                saw_unchanged |= damage.is_empty() && secondary_damage.is_empty();
            }
        }
        assert!(saw_partial && saw_unchanged);
    }
}
