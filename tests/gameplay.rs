use umbral::{
    assets::{obj::load_model, ship_model_path},
    scene::{ship::ShipInput, world::World},
};

#[test]
fn planet_anchored_pickups_follow_orbits_and_are_unique() {
    let mut w = world();
    assert_eq!(w.gameplay.resources.len(), 3);
    assert_eq!(w.gameplay.logs.len(), 3);
    let pickup = &w.gameplay.resources[0];
    let p0 = pickup.position(&w.system, 0.0).unwrap();
    let p1 = pickup.position(&w.system, 5.0).unwrap();
    assert!((p0 - p1).length() > 1.0);
    w.gameplay.oxygen01 = 0.5;
    w.toggles.freeze_orbits = true;
    w.ship.position = p0;
    w.update(&ShipInput::default(), 0.01);
    assert!(w.gameplay.resources[0].collected);
    assert_eq!(w.gameplay.collected_resources, 1);
    assert!(w.gameplay.oxygen01 > 0.64);
    w.update(&ShipInput::default(), 0.01);
    assert_eq!(w.gameplay.collected_resources, 1);
    for i in [2, 0, 1] {
        assert!(!w.gameplay.logs[i].text.is_empty());
        w.ship.position = w.gameplay.logs[i]
            .anchor
            .position(&w.system, w.time)
            .unwrap();
        w.update(&ShipInput::default(), 0.01);
    }
    assert_eq!(w.gameplay.collected_logs, 3);
    assert!(!w.gameplay.objective_complete);
    w.ship.position = umbral::math::Vec3::new(0.0, 0.0, 80.0);
    w.update(&ShipInput::default(), 0.01);
    assert!(w.gameplay.objective_complete);
    w.restart_game();
    assert_eq!(w.gameplay.collected_logs, 0);
    assert_eq!(w.gameplay.resources.len(), 3);
    assert!(w.gameplay.resources.iter().all(|r| !r.collected));
    assert!(w.gameplay.logs.iter().all(|r| !r.anchor.collected));
}

#[test]
fn collisions_separate_contact_and_stop_boost_tunneling() {
    use umbral::{math::Vec3, scene::ship::Ship};
    let mut w = world();
    w.toggles.freeze_orbits = true;
    let body = &w.system.bodies[1];
    let center = body.position(0.0);
    let safe = body.body_radius + w.gameplay.navigation.settings.ship_radius;
    w.ship = Ship::new(center, Vec3::X);
    w.update(&ShipInput::default(), 0.01);
    assert!((w.ship.position - center).length() >= safe - 1e-8);
    w.ship = Ship::new(center - Vec3::X * 100.0, Vec3::X);
    w.ship.speed = 300.0;
    w.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        0.6,
    );
    assert!(w.ship.position.x < center.x);
    assert!((w.ship.position - center).length() >= safe - 1e-8);
}

#[test]
fn orbital_assist_completes_a_circle_and_pilot_can_leave() {
    use umbral::{math::Vec3, scene::ship::Ship};
    let mut w = world();
    w.orbit_assist = true;
    let body = &w.system.bodies[1];
    let radius = body.body_radius + w.gameplay.navigation.settings.orbit_gap;
    w.ship = Ship::new(body.position(0.0) + Vec3::X * radius, Vec3::Z);
    let mut angle = 0.0;
    let mut previous = Vec3::X;
    for _ in 0..1800 {
        w.update(&ShipInput::default(), 1.0 / 120.0);
        let radial = (w.ship.position - w.system.bodies[1].position(w.time)).normalized();
        angle += previous.cross(radial).length().atan2(previous.dot(radial));
        previous = radial;
        let distance = (w.ship.position - w.system.bodies[1].position(w.time)).length();
        assert!((distance - radius).abs() < 0.2, "distance {distance}");
        assert!((w.camera.position - w.ship.position).length() < 24.0);
    }
    assert!(angle > std::f64::consts::TAU);
    assert_eq!(w.gameplay.navigation.assisted_planet, Some(1));
    w.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        1.0,
    );
    assert_eq!(w.gameplay.navigation.assisted_planet, None);
    assert!((w.ship.position - w.system.bodies[1].position(w.time)).length() > radius + 10.0);
}

#[test]
fn last_log_persists_but_reset_clears_it_and_debug_is_only_visual() {
    let mut w = world();
    w.toggles.freeze_orbits = true;
    w.ship.position = w.gameplay.logs[1]
        .anchor
        .position(&w.system, w.time)
        .unwrap();
    w.update(&ShipInput::default(), 0.01);
    assert_eq!(w.gameplay.last_log, Some(1));
    w.update(&ShipInput::default(), 0.01);
    assert!(w.gameplay.log_events.is_empty());
    assert_eq!(w.gameplay.last_log, Some(1));
    w.set_debug_oxygen(0.0);
    let real = w.gameplay.oxygen01;
    w.update(&ShipInput::default(), 1.0);
    assert_eq!(w.oxygen01(), 0.0);
    assert!(!w.gameplay.game_over);
    assert!(w.gameplay.oxygen01 < real && w.gameplay.oxygen01 > 0.9);
    w.clear_debug_oxygen();
    assert_eq!(w.oxygen01(), w.gameplay.oxygen01);
    w.debug_oxygen01 = 0.25;
    w.update(&ShipInput::default(), 0.01);
    assert_eq!(w.oxygen01(), 0.25);
    w.restart_game();
    assert_eq!(w.gameplay.last_log, None);
    assert_eq!(w.forced_oxygen, None);
}

#[test]
fn gravity_is_opt_in_bounded_and_changes_real_trajectory() {
    use umbral::{math::Vec3, scene::ship::Ship};
    let run = |strength| {
        let mut w = world();
        w.toggles.freeze_orbits = true;
        w.gameplay.navigation.settings.gravity_strength = strength;
        let center = w.system.bodies[1].position(0.0);
        w.ship = Ship::new(center + Vec3::X * 60.0, Vec3::Z);
        let initial = w.ship.position;
        for _ in 0..120 {
            w.update(&ShipInput::default(), 1.0 / 120.0);
        }
        assert!(w.ship.position.is_finite());
        (w.ship.position - initial).length()
    };
    assert_eq!(world().gameplay.navigation.settings.gravity_strength, 0.0);
    assert_eq!(run(0.0), 0.0);
    assert!(run(100.0) > 0.1);
}

#[test]
fn ship_distance_is_frame_exact_and_invalid_controls_stay_finite() {
    use umbral::{math::Vec3, scene::ship::Ship};
    let run = |fps: usize| {
        let mut s = Ship::new(Vec3::ZERO, -Vec3::Z);
        for _ in 0..fps * 3 {
            s.update(
                &ShipInput {
                    thrust: 1.0,
                    ..ShipInput::default()
                },
                1.0 / fps as f64,
            );
        }
        s.position
    };
    assert!((run(30) - run(144)).length() < 1e-8);
    let mut s = Ship::new(Vec3::ZERO, -Vec3::Z);
    s.update(
        &ShipInput {
            thrust: f64::NAN,
            yaw: f64::INFINITY,
            pitch: 99.0,
            roll: -99.0,
            boost: false,
        },
        0.1,
    );
    assert!(s.position.is_finite() && s.forward().is_finite() && s.speed.is_finite());
}

fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

#[test]
fn oxygen_lasts_seven_minutes_and_death_freezes_until_reset() {
    let mut w = world();
    w.update(&ShipInput::default(), 210.0);
    assert!((w.oxygen01() - 0.5).abs() < 1e-9);
    w.update(&ShipInput::default(), 210.0);
    assert_eq!(w.oxygen01(), 0.0);
    assert!(w.gameplay.game_over);
    let position = w.ship.position;
    w.update(
        &ShipInput {
            thrust: 1.0,
            ..ShipInput::default()
        },
        1.0,
    );
    assert_eq!(w.ship.position, position);
    w.restart_game();
    assert_eq!(w.oxygen01(), 1.0);
    assert!(!w.gameplay.game_over);
}

#[test]
fn invalid_gameplay_settings_and_oxygen_cannot_poison_or_expand_pickups() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let system = SolarSystem::demo();
    let mut state = GameplayState::default();
    state.settings.drain_per_second = f64::INFINITY;
    state.tick(1.0);
    assert!(state.oxygen01 > 0.9 && state.oxygen01 <= 1.0);
    state.settings.pickup_radius = f64::INFINITY;
    state.collect_near(Vec3::new(10000.0, 0.0, 0.0), &system, 0.0);
    assert_eq!(state.collected_resources, 0);
    assert_eq!(state.collected_logs, 0);
    state.settings.pickup_recovery = f64::NAN;
    state.oxygen01 = 0.5;
    let position = state.resources[0].position(&system, 0.0).unwrap();
    state.collect_near(position, &system, 0.0);
    assert!(state.oxygen01.is_finite() && (0.0..=1.0).contains(&state.oxygen01));
    state.oxygen01 = f64::NAN;
    state.tick(1.0);
    assert_eq!(state.oxygen01, 0.0);
    assert!(state.game_over);
    state.collect_near(position, &system, 0.0);
    assert_eq!(state.oxygen01, 0.0);
}

#[test]
fn oxygen_is_frame_independent_and_invalid_time_is_ignored() {
    use umbral::gameplay::GameplayState;
    let run = |fps: usize| {
        let mut state = GameplayState::default();
        for _ in 0..fps * 210 {
            state.tick(1.0 / fps as f64);
        }
        state.oxygen01
    };
    assert!((run(30) - 0.5).abs() < 1e-10);
    assert!((run(144) - run(30)).abs() < 1e-10);
    let mut state = GameplayState::default();
    for dt in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        state.tick(dt);
    }
    assert_eq!(state.oxygen01, 1.0);
    assert!((300.0..=600.0).contains(&(1.0 / state.settings.drain_per_second)));
}

#[test]
fn all_six_log_orders_require_sun_arrival_and_victory_is_terminal() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let system = SolarSystem::demo();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut state = GameplayState::default();
        state.collect_near(Vec3::ZERO, &system, 0.0);
        assert!(!state.objective_complete);
        for (count, index) in order.into_iter().enumerate() {
            let position = state.logs[index].anchor.position(&system, 0.0).unwrap();
            assert!(state.logs[index].text.len() > 80);
            state.collect_near(position, &system, 0.0);
            assert_eq!(state.collected_logs, count as u32 + 1);
            assert_eq!(state.last_log, Some(index));
            state.collect_near(position, &system, 0.0);
            assert!(state.log_events.is_empty());
            assert_eq!(state.last_log, Some(index));
            assert!(!state.objective_complete);
        }
        state.settings.sun_arrival_gap = f64::INFINITY;
        state.collect_near(Vec3::new(10000.0, 0.0, 0.0), &system, 0.0);
        assert!(!state.objective_complete);
        state.collect_near(Vec3::new(0.0, 0.0, 80.0), &system, 0.0);
        assert!(state.objective_complete && !state.game_over);
        let oxygen = state.oxygen01;
        let resource = state.resources[0].position(&system, 0.0).unwrap();
        state.tick(10000.0);
        state.collect_near(resource, &system, 0.0);
        assert_eq!(state.oxygen01, oxygen);
        assert_eq!(state.collected_resources, 0);
        assert!(!state.game_over);
    }
}

#[test]
fn swept_pickups_catch_boost_crossings_once_and_keep_near_compatible() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let system = SolarSystem::demo();
    let mut state = GameplayState {
        oxygen01: 0.5,
        ..GameplayState::default()
    };
    let position = state.resources[0].position(&system, 0.0).unwrap();
    let start = position - Vec3::Z * 100.0;
    let end = position + Vec3::Z * 100.0;
    state.collect_along(start, end, &system, 0.0, 0.0);
    assert_eq!(state.resource_events, vec![0]);
    assert_eq!(state.collected_resources, 1);
    assert_eq!(state.oxygen01, 0.65);
    state.collect_along(end, start, &system, 0.0, 0.0);
    assert!(state.resource_events.is_empty());
    assert_eq!(state.collected_resources, 1);
    assert_eq!(state.oxygen01, 0.65);
    let position = state.logs[0].anchor.position(&system, 0.0).unwrap();
    state.collect_along(
        position - Vec3::Z * 100.0,
        position + Vec3::Z * 100.0,
        &system,
        0.0,
        0.0,
    );
    assert_eq!(state.log_events, vec![0]);
    state.collect_near(position, &system, 0.0);
    assert!(state.log_events.is_empty());
    assert_eq!(state.last_log, Some(0));
}

#[test]
fn swept_pickups_use_relative_planet_motion_and_reject_invalid_paths() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let system = SolarSystem::demo();
    let mut state = GameplayState::default();
    let p0 = state.resources[0].position(&system, 0.0).unwrap();
    let p1 = state.resources[0].position(&system, 5.0).unwrap();
    let mid = (p0 + p1) * 0.5;
    assert!((p0 - mid).length() > state.settings.pickup_radius);
    state.collect_along(mid, mid, &system, 0.0, 5.0);
    assert_eq!(state.collected_resources, 1);
    let mut state = GameplayState::default();
    for (start, end, t0, t1) in [
        (p0, p0, f64::NAN, 0.0),
        (p0, p0, 0.0, f64::INFINITY),
        (p0, p0, 1.0, 0.0),
        (p0, Vec3::new(f64::NAN, 0.0, 0.0), 0.0, 0.0),
    ] {
        state.collect_along(start, end, &system, t0, t1);
        assert_eq!(state.collected_resources, 0);
        assert!(!state.objective_complete);
    }
    assert!(state.resources[0].position(&system, f64::NAN).is_none());
    state.resources[0].local_offset.x = f64::NAN;
    assert!(state.resources[0].position(&system, 0.0).is_none());
}

#[test]
fn death_blocks_resources_logs_and_sun_then_reset_clears_all_progress() {
    use umbral::math::Vec3;
    let mut w = world();
    w.gameplay.tick(420.0);
    let resource = w.gameplay.resources[0].position(&w.system, 0.0).unwrap();
    let log = w.gameplay.logs[0].anchor.position(&w.system, 0.0).unwrap();
    w.gameplay.collect_near(resource, &w.system, 0.0);
    w.gameplay.collect_near(log, &w.system, 0.0);
    w.gameplay.collect_along(resource, log, &w.system, 0.0, 0.0);
    w.gameplay.collect_near(Vec3::ZERO, &w.system, 0.0);
    assert_eq!(w.gameplay.collected_resources, 0);
    assert_eq!(w.gameplay.collected_logs, 0);
    assert!(!w.gameplay.objective_complete);
    assert_eq!(w.gameplay.oxygen01, 0.0);
    for _ in 0..3 {
        w.restart_game();
        assert_eq!(w.gameplay.oxygen01, 1.0);
        assert_eq!(w.gameplay.resources.len(), 3);
        assert_eq!(w.gameplay.logs.len(), 3);
        assert!(w.gameplay.resource_events.is_empty() && w.gameplay.log_events.is_empty());
        assert_eq!(w.gameplay.last_log, None);
        assert!(!w.gameplay.game_over && !w.gameplay.objective_complete);
        assert!(w.gameplay.resources.iter().all(|r| !r.collected));
        assert!(w.gameplay.logs.iter().all(|log| !log.anchor.collected));
    }
}

#[test]
fn public_nan_state_is_repaired_even_with_invalid_dt_and_death_has_priority() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let mut state = GameplayState {
        oxygen01: f64::NAN,
        ..GameplayState::default()
    };
    state.settings.drain_per_second = f64::NAN;
    state.tick(f64::NAN);
    assert_eq!(state.oxygen01, 0.0);
    assert!(state.game_over);
    assert!(state.settings.drain_per_second.is_finite());
    state.objective_complete = true;
    state.oxygen01 = 0.8;
    state.tick(0.0);
    assert!(state.game_over && !state.objective_complete);
    assert_eq!(state.oxygen01, 0.0);
    state.collect_near(Vec3::ZERO, &SolarSystem::demo(), 0.0);
    assert!(state.game_over && !state.objective_complete);
}

#[test]
fn negative_settings_clamp_and_recovery_caps_at_full_oxygen() {
    use umbral::{gameplay::GameplayState, math::Vec3, scene::solar_system::SolarSystem};
    let system = SolarSystem::demo();
    let mut state = GameplayState {
        oxygen01: 4.0,
        ..GameplayState::default()
    };
    state.settings.drain_per_second = -1.0;
    state.settings.pickup_recovery = -0.5;
    state.settings.pickup_radius = -10.0;
    state.settings.sun_arrival_gap = -30.0;
    state.tick(10.0);
    assert_eq!(state.oxygen01, 1.0);
    assert_eq!(state.settings.drain_per_second, 0.0);
    assert_eq!(state.settings.pickup_recovery, 0.0);
    assert_eq!(state.settings.pickup_radius, 0.0);
    assert_eq!(state.settings.sun_arrival_gap, 0.0);
    state.oxygen01 = 0.9;
    state.settings.pickup_recovery = 100.0;
    let position = state.resources[0].position(&system, 0.0).unwrap();
    state.collect_near(position, &system, 0.0);
    assert_eq!(state.oxygen01, 1.0);
    assert_eq!(state.collected_resources, 1);
    state.collect_near(position, &system, 0.0);
    assert_eq!(state.collected_resources, 1);
    state.collect_near(Vec3::new(f64::INFINITY, 0.0, 0.0), &system, 0.0);
    assert!(state.resource_events.is_empty());
}
