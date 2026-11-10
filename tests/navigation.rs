use umbral::{
    gameplay::navigation::NavigationState,
    math::Vec3,
    scene::{
        ship::{Ship, ShipInput, ShipSettings, smoothing},
        solar_system::SolarSystem,
    },
};

fn system() -> SolarSystem {
    let mut s = SolarSystem::demo();
    s.bodies.truncate(2);
    s.bodies[0].orbit.radius = 10000.0;
    s.bodies[1].orbit.radius = 0.0;
    s
}

#[test]
fn turn_integrates_progressive_rate_independently_of_fps() {
    let run = |fps| {
        let mut ship = Ship::new(Vec3::ZERO, -Vec3::Z);
        for _ in 0..fps {
            ship.update(
                &ShipInput {
                    yaw: 1.0,
                    ..ShipInput::default()
                },
                1.0 / fps as f64,
            );
        }
        ship.forward()
    };
    assert!((run(20) - run(144)).length() < 1e-8);
}

#[test]
fn curved_flight_and_braking_are_comparable_across_fps() {
    let run = |fps| {
        let mut ship = Ship::new(Vec3::ZERO, -Vec3::Z);
        for frame in 0..fps * 4 {
            ship.update(
                &ShipInput {
                    thrust: if frame < fps * 2 { 1.0 } else { 0.0 },
                    yaw: 0.6,
                    pitch: 0.2,
                    ..ShipInput::default()
                },
                1.0 / fps as f64,
            );
        }
        ship
    };
    let a = run(20);
    let b = run(144);
    assert!((a.position - b.position).length() < 0.03);
    assert!((a.forward() - b.forward()).length() < 0.001);
    assert!(a.speed > 0.0 && a.speed < 3.0);
}

#[test]
fn invalid_ship_settings_and_constructor_direction_do_not_poison_pose() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -10.0, f64::MAX] {
        let mut ship = Ship::new(Vec3::ZERO, Vec3::ZERO);
        ship.settings = ShipSettings {
            max_speed: value,
            boost_multiplier: value,
            reverse_fraction: value,
            speed_response: value,
            turn_rate: value,
            turn_response: value,
        };
        ship.update(
            &ShipInput {
                thrust: 1.0,
                yaw: 1.0,
                boost: true,
                ..ShipInput::default()
            },
            0.5,
        );
        assert!(ship.position.is_finite() && ship.speed.is_finite());
        assert!((ship.forward().length() - 1.0).abs() < 1e-8);
    }
    assert_eq!(smoothing(-1.0, 1.0), 0.0);
    assert_eq!(smoothing(1.0, f64::NAN), 0.0);
}

#[test]
fn invalid_or_unbounded_dt_is_a_noop_for_motion_and_navigation() {
    let s = system();
    for dt in [0.0, -0.1, f64::NAN, f64::INFINITY, f64::MAX] {
        let mut ship = Ship::new(Vec3::X * 34.0, Vec3::Z);
        let original = ship;
        let mut nav = NavigationState::default();
        nav.settings.gravity_strength = 10.0;
        ship.update(
            &ShipInput {
                thrust: 1.0,
                ..ShipInput::default()
            },
            dt,
        );
        nav.apply_gravity(&mut ship, &s, 0.0, dt);
        nav.assist(&mut ship, &ShipInput::default(), &s, 0.0, 0.0, dt, true);
        assert_eq!(ship.position, original.position, "dt {dt}");
        assert_eq!(ship.orientation, original.orientation);
        assert_eq!(nav.gravity_velocity, Vec3::ZERO);
    }
}

#[test]
fn polar_orbit_keeps_one_plane_and_completes_a_great_circle() {
    let s = system();
    let mut ship = Ship::new(Vec3::Y * 34.0, Vec3::X);
    let mut nav = NavigationState::default();
    let mut angle = 0.0;
    let mut prev = ship.position.normalized();
    let mut prev_forward = None;
    for _ in 0..1800 {
        nav.assist(
            &mut ship,
            &ShipInput::default(),
            &s,
            0.0,
            0.0,
            1.0 / 120.0,
            true,
        );
        let radial = ship.position.normalized();
        angle += prev.cross(radial).length().atan2(prev.dot(radial));
        assert!(
            ship.position.z.abs() < 1e-7,
            "plane changed: {:?}",
            ship.position
        );
        assert!((ship.position.length() - 34.0).abs() < 1e-7);
        if let Some(forward) = prev_forward {
            assert!(ship.forward().dot(forward) > 0.999);
        }
        assert!(ship.forward().dot(radial).abs() < 1e-8);
        prev_forward = Some(ship.forward());
        prev = radial;
    }
    assert!(angle > std::f64::consts::TAU);
}

#[test]
fn gravity_does_not_build_hidden_momentum_during_assistance() {
    let s = system();
    let mut nav = NavigationState::default();
    nav.settings.gravity_strength = 100.0;
    let mut ship = Ship::new(Vec3::X * 34.0, Vec3::Z);
    for _ in 0..600 {
        nav.apply_gravity(&mut ship, &s, 0.0, 1.0 / 60.0);
        nav.assist(
            &mut ship,
            &ShipInput::default(),
            &s,
            0.0,
            0.0,
            1.0 / 60.0,
            true,
        );
    }
    assert_eq!(nav.gravity_velocity, Vec3::ZERO);
    let before = ship.position;
    ship.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        1.0 / 60.0,
    );
    nav.apply_gravity(&mut ship, &s, 0.0, 1.0 / 60.0);
    nav.assist(
        &mut ship,
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        &s,
        0.0,
        0.0,
        1.0 / 60.0,
        true,
    );
    assert_eq!(nav.assisted_planet, None);
    assert!((ship.position - before).length() < 0.2);
}

#[test]
fn invalid_controls_release_assistance_and_invalid_settings_stay_safe() {
    let s = system();
    let mut ship = Ship::new(Vec3::X * 34.0, Vec3::Z);
    let mut nav = NavigationState::default();
    nav.assist(
        &mut ship,
        &ShipInput {
            yaw: f64::NAN,
            ..ShipInput::default()
        },
        &s,
        0.0,
        0.0,
        0.1,
        true,
    );
    assert_eq!(nav.assisted_planet, None);
    nav.settings.orbit_angular_speed = f64::INFINITY;
    nav.settings.orbit_gap = f64::NAN;
    nav.settings.orbit_response = -1.0;
    nav.settings.ship_radius = f64::NAN;
    nav.settings.gravity_strength = f64::MAX;
    nav.settings.max_gravity_acceleration = f64::INFINITY;
    nav.apply_gravity(&mut ship, &s, 0.0, 0.1);
    nav.assist(&mut ship, &ShipInput::default(), &s, 0.0, 0.0, 0.1, true);
    assert!(ship.position.is_finite() && ship.forward().is_finite());
    assert!(nav.gravity_velocity.is_finite());
}

#[test]
fn exact_surface_contact_cannot_tunnel_to_opposite_side() {
    let s = system();
    let mut nav = NavigationState::default();
    let start = -Vec3::X * 16.0;
    let mut ship = Ship::new(Vec3::X * 100.0, Vec3::X);
    ship.speed = 300.0;
    nav.gravity_velocity = Vec3::X * 10.0;
    nav.resolve_collisions(&mut ship, start, &s, 0.0, 0.0);
    assert!(ship.position.x <= -16.0);
    assert_eq!(ship.speed, 0.0);
    assert_eq!(nav.gravity_velocity, Vec3::ZERO);
}

#[test]
fn swept_collision_chooses_first_hit_regardless_of_body_order() {
    let mut s = system();
    let mut later = s.bodies[1].clone();
    later.orbit.radius = 50.0;
    later.phase = 0.0;
    s.bodies.insert(1, later);
    let mut ship = Ship::new(Vec3::X * 100.0, Vec3::X);
    let mut nav = NavigationState::default();
    nav.resolve_collisions(&mut ship, -Vec3::X * 100.0, &s, 0.0, 0.0);
    assert!(ship.position.x <= -16.0, "{:?}", ship.position);
}

#[test]
fn sweep_uses_relative_motion_and_discrete_resolution_allows_exit() {
    let mut s = system();
    s.bodies[1].orbit.radius = 100.0;
    s.bodies[1].orbit.angular_speed = std::f64::consts::PI;
    s.bodies[1].phase = 0.0;
    let mut nav = NavigationState::default();
    let mut ship = Ship::new(Vec3::ZERO, Vec3::Z);
    nav.resolve_collisions(&mut ship, Vec3::ZERO, &s, 0.0, 1.0);
    assert!((ship.position - s.bodies[1].position(1.0)).length() >= 16.0);
    assert!(
        ship.position.x < -80.0,
        "moving body must push the ship along its swept path"
    );
    let s = system();
    ship.position = Vec3::ZERO;
    nav.resolve_collisions(&mut ship, Vec3::ZERO, &s, 0.0, 0.0);
    assert!(ship.position.length() >= 16.0);
    let start = ship.position;
    ship.position = start * 2.0;
    let end = ship.position;
    nav.resolve_collisions(&mut ship, start, &s, 0.0, 0.0);
    assert_eq!(ship.position, end);
}

#[test]
fn capture_range_is_respected_even_with_a_larger_target_orbit() {
    let s = system();
    let mut ship = Ship::new(Vec3::X * 214.0, Vec3::Z);
    let mut nav = NavigationState::default();
    nav.settings.orbit_gap = 300.0;
    nav.settings.orbit_capture_gap = 100.0;
    nav.assist(&mut ship, &ShipInput::default(), &s, 0.0, 0.0, 0.1, true);
    assert_eq!(nav.assisted_planet, None);
    assert_eq!(ship.position, Vec3::X * 214.0);
}

#[test]
fn nearest_capture_manual_exit_and_safe_radius_keep_settings() {
    let mut s = system();
    let mut other = s.bodies[1].clone();
    other.orbit.radius = 200.0;
    other.phase = 0.0;
    s.bodies.push(other);
    for input in [
        ShipInput {
            thrust: -1.0,
            ..ShipInput::default()
        },
        ShipInput {
            yaw: 1.0,
            ..ShipInput::default()
        },
        ShipInput {
            pitch: 1.0,
            ..ShipInput::default()
        },
        ShipInput {
            roll: 1.0,
            ..ShipInput::default()
        },
        ShipInput {
            boost: true,
            ..ShipInput::default()
        },
    ] {
        let mut nav = NavigationState::default();
        nav.settings.orbit_gap = -10.0;
        let mut ship = Ship::new(Vec3::X * 200.0, Vec3::Z);
        ship.settings.turn_rate = 0.75;
        nav.assist(&mut ship, &ShipInput::default(), &s, 0.0, 0.0, 0.1, true);
        assert_eq!(nav.assisted_planet, Some(2));
        assert!((ship.position - Vec3::X * 200.0).length() >= 17.0 - 1e-8);
        assert_eq!(ship.settings.turn_rate, 0.75);
        let position = ship.position;
        nav.assist(&mut ship, &input, &s, 0.0, 0.0, 0.1, true);
        assert_eq!(nav.assisted_planet, None);
        assert_eq!(ship.position, position);
    }
}

#[test]
fn gravity_is_bounded_and_disabled_values_remove_old_momentum() {
    let s = system();
    let mut nav = NavigationState::default();
    assert_eq!(nav.settings.gravity_strength, 0.0);
    nav.settings.gravity_strength = f64::MAX;
    nav.settings.max_gravity_acceleration = 3.0;
    let mut ship = Ship::new(Vec3::X * 34.0, Vec3::Z);
    nav.apply_gravity(&mut ship, &s, 0.0, 0.5);
    assert!(nav.gravity_velocity.length() <= 1.5 + 1e-8);
    assert!((ship.position - Vec3::X * 34.0).length() <= 0.375 + 1e-8);
    for disabled in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        nav.settings.gravity_strength = disabled;
        nav.gravity_velocity = Vec3::X * 20.0;
        let start = ship.position;
        nav.apply_gravity(&mut ship, &s, 0.0, 0.5);
        assert_eq!(ship.position, start);
        assert_eq!(nav.gravity_velocity, Vec3::ZERO);
    }
}

#[test]
fn real_boost_step_cannot_cross_a_moving_planet() {
    let mut s = system();
    s.bodies[1].orbit.radius = 100.0;
    s.bodies[1].phase = 0.0;
    s.bodies[1].orbit.angular_speed = 0.1;
    let mut ship = Ship::new(Vec3::ZERO, Vec3::X);
    ship.speed = 300.0;
    let start = ship.position;
    ship.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        0.6,
    );
    assert!(ship.position.x > 170.0);
    let mut nav = NavigationState::default();
    nav.resolve_collisions(&mut ship, start, &s, 0.0, 0.6);
    let relative = ship.position - s.bodies[1].position(0.6);
    assert!(relative.x < 0.0 && relative.length() >= 16.0);
    assert_eq!(ship.speed, 0.0);
}

#[test]
fn slight_initial_penetration_does_not_allow_crossing_the_whole_body() {
    let s = system();
    let mut nav = NavigationState::default();
    let start = -Vec3::X * (16.0 - 1e-8);
    let mut ship = Ship::new(Vec3::X * 100.0, Vec3::X);
    ship.speed = 300.0;
    nav.resolve_collisions(&mut ship, start, &s, 0.0, 0.0);
    assert!(ship.position.x <= -16.0);
    assert_eq!(ship.speed, 0.0);
    // El mismo inicio ligeramente penetrado puede salir por su lado sin bloquearse.
    ship.position = -Vec3::X * 30.0;
    let end = ship.position;
    nav.resolve_collisions(&mut ship, start, &s, 0.0, 0.0);
    assert_eq!(ship.position, end);
}
