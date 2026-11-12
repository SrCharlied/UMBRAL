//! Joint visual/gameplay contracts; no GUI or keyboard acceptance.
use umbral::{
    app::advance_perception_frame,
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    scene::{
        ship::{Ship, ShipInput},
        world::World,
    },
    visual::perception_game::{OxygenKeys, OxygenTransition},
};
#[test]
fn visual_reset_preserves_progress_and_explicit_restart_clears_terminal() {
    use umbral::app::{GameplayAction, apply_gameplay_action};
    let mut w = world();
    w.gameplay.oxygen01 = 0.4;
    w.time = 10.0;
    w.gameplay.collected_logs = 2;
    w.gameplay.game_over = true;
    w.set_debug_oxygen(0.05);
    w.reset_ship();
    assert_eq!(w.gameplay.oxygen01, 0.4, "visual reset erased real reserve");
    assert_eq!(w.time, 10.0);
    assert_eq!(w.gameplay.collected_logs, 2);
    assert!(w.gameplay.game_over);
    assert_eq!(w.debug_oxygen01, 0.05);
    apply_gameplay_action(&mut w, GameplayAction::Restart);
    assert_eq!(w.gameplay.oxygen01, 1.0);
    assert_eq!(w.time, 0.0);
    assert_eq!(w.gameplay.collected_logs, 0);
    assert!(!w.gameplay.game_over);
    assert_eq!(w.forced_oxygen, None);
}
#[test]
fn captured_commit_transfers_navigation_and_camera_atomically() {
    let mut w = world();
    w.orbit_assist = true;
    w.gameplay.navigation.assisted_planet = Some(1);
    w.gameplay.navigation.gravity_velocity = Vec3::X * 100.0;
    let before = (
        format!("{:?}", w.ship),
        format!("{:?}", w.camera),
        format!("{:?}", w.gameplay.navigation),
    );
    assert!(
        w.commit_captured_ship_pose(Vec3::new(f64::NAN, 0.0, 0.0), w.ship.orientation)
            .is_err()
    );
    assert_eq!(
        before,
        (
            format!("{:?}", w.ship),
            format!("{:?}", w.camera),
            format!("{:?}", w.gameplay.navigation)
        )
    );
    w.commit_captured_ship_pose(w.ship.position, w.ship.orientation)
        .unwrap();
    assert_eq!(
        w.gameplay.navigation.gravity_velocity,
        Vec3::ZERO,
        "capture retained stale navigation inertia"
    );
    assert_eq!(
        w.gameplay.navigation.assisted_planet, None,
        "assist still owns captured camera"
    );
    let target = w.ship.position - w.ship.orientation.transform_dir(Vec3::Z) * 30.0;
    assert_eq!(w.camera.target, target);
    let camera = w.camera;
    assert!(
        w.commit_captured_ship_pose(Vec3::X, umbral::math::Mat4::scaling(Vec3::splat(2.0)))
            .is_err()
    );
    assert_eq!(format!("{:?}", camera), format!("{:?}", w.camera));
}
#[test]
fn debug_key0_is_smooth_and_elapsed_clamp_and_f5_keep_real_contract() {
    use umbral::app::{GameplayAction, apply_gameplay_action};
    let mut w = world();
    let mut o = OxygenTransition::new(1.0).unwrap();
    w.toggles.freeze_orbits = true;
    let drain = w.gameplay.settings.drain_per_second;
    advance_perception_frame(
        &mut w,
        &mut o,
        OxygenKeys {
            preset: Some(0.05),
            ..Default::default()
        },
        &ShipInput::default(),
        2.0,
    )
    .unwrap();
    assert!((w.gameplay.oxygen01 - (1.0 - drain * 0.1)).abs() < 1e-12);
    assert_eq!(w.time, 0.0);
    assert_eq!(o.shown(), 0.5);
    assert_eq!(w.debug_oxygen01, o.shown());
    assert_eq!(w.oxygen01(), o.shown());
    let shown = o.shown();
    let real = w.gameplay.oxygen01;
    apply_gameplay_action(&mut w, GameplayAction::RealOxygen);
    assert_eq!(
        w.debug_oxygen01, shown,
        "Key0 snapped the published visual instead of preserving smoothing"
    );
    assert_eq!(w.gameplay.oxygen01, real, "Key0 refilled reserve");
    advance_perception_frame(
        &mut w,
        &mut o,
        OxygenKeys::default(),
        &ShipInput::default(),
        0.1,
    )
    .unwrap();
    assert!((o.shown() - (shown + 0.025)).abs() < 1e-12);
    assert_eq!(w.forced_oxygen, None);
    for _ in 0..30 {
        advance_perception_frame(
            &mut w,
            &mut o,
            OxygenKeys::default(),
            &ShipInput::default(),
            0.1,
        )
        .unwrap();
    }
    assert_eq!(w.debug_oxygen01, w.gameplay.oxygen01);
}
fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}
#[test]
fn real_oxygen_publication_after_consumption_and_pickup_does_not_enable_debug() {
    let mut w = world();
    let mut o = OxygenTransition::new(1.0).unwrap();
    for _ in 0..3 {
        advance_perception_frame(
            &mut w,
            &mut o,
            OxygenKeys::default(),
            &ShipInput::default(),
            0.1,
        )
        .unwrap();
        assert_eq!(
            w.forced_oxygen, None,
            "internal shown publication enabled debug override"
        );
        assert_eq!(
            w.debug_oxygen01, w.gameplay.oxygen01,
            "default visual disconnected from real reserve"
        );
        assert_eq!(o.shown(), w.gameplay.oxygen01);
    }
    w.gameplay.oxygen01 = 0.5;
    let p = w.gameplay.resources[0].position(&w.system, w.time).unwrap();
    w.ship = Ship::new(p, -Vec3::Z);
    advance_perception_frame(
        &mut w,
        &mut o,
        OxygenKeys::default(),
        &ShipInput::default(),
        2.0,
    )
    .unwrap();
    assert_eq!(w.gameplay.collected_resources, 1);
    assert_eq!(w.forced_oxygen, None);
    assert_eq!(
        o.shown(),
        w.gameplay.oxygen01,
        "publication omitted final pickup"
    );
    assert_eq!(w.oxygen01(), o.shown());
}
