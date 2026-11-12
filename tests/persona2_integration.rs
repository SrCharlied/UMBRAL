//! Integracion real con World. Las colocaciones son setup, no pilotaje humano.
use std::f64::consts::TAU;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::framebuffer::Framebuffer,
    scene::{
        ship::{Ship, ShipInput},
        world::{SPACE_COLOR, World},
    },
};

fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

fn idle(w: &mut World, frames: usize, fps: usize) {
    for _ in 0..frames {
        w.update(&ShipInput::default(), 1.0 / fps as f64);
    }
}

fn collect_resource(w: &mut World, index: usize) {
    let p = w.gameplay.resources[index]
        .position(&w.system, w.time)
        .unwrap();
    w.ship = Ship::new(p, -Vec3::Z);
    idle(w, 1, 60);
}

fn collect_log(w: &mut World, index: usize) {
    let p = w.gameplay.logs[index]
        .anchor
        .position(&w.system, w.time)
        .unwrap();
    w.ship = Ship::new(p, -Vec3::Z);
    idle(w, 1, 60);
    assert!(w.gameplay.logs[index].anchor.collected);
    assert_eq!(w.gameplay.last_log, Some(index));
    assert_eq!(w.gameplay.log_events, vec![index]);
}

fn arrive_at_sun(w: &mut World) {
    let sun = &w.system.bodies[0];
    w.ship = Ship::new(
        sun.position(w.time) + Vec3::Z * (sun.body_radius + 20.0),
        -Vec3::Z,
    );
    idle(w, 1, 60);
}

#[test]
fn baseline_fov_route_hits_sun_and_stops_boost() {
    let mut w = world();
    let input = ShipInput {
        thrust: 1.0,
        boost: true,
        ..ShipInput::default()
    };
    let mut contacts = 0;
    for _ in 0..600 {
        w.update(&input, 1.0 / 60.0);
        let sun = &w.system.bodies[0];
        let safe = sun.body_radius + w.gameplay.navigation.settings.ship_radius;
        if (w.ship.position - sun.position(w.time)).length() <= safe + 1e-6 {
            contacts += 1;
            assert_eq!(w.ship.speed, 0.0);
        }
    }
    assert!(
        contacts > 100,
        "expected repeated Sun contact, got {contacts}"
    );
    println!(
        "baseline: contacts={contacts}, speed={}, fov={}",
        w.ship.speed, w.camera.fov_y
    );
}

#[test]
fn world_oxygen_at_30_and_144_fps_including_death() {
    let run = |fps: usize| {
        let mut w = world();
        // Fuera de los cuerpos; simulacion sin congelar orbitas.
        w.ship = Ship::new(Vec3::new(2000.0, 2000.0, 2000.0), Vec3::X);
        idle(&mut w, fps * 210, fps);
        assert!((w.gameplay.oxygen01 - 0.5).abs() < 1e-10);
        assert_eq!(w.oxygen01(), w.gameplay.oxygen01);
        assert!(!w.gameplay.game_over);
        let half = w.gameplay.oxygen01;
        idle(&mut w, fps * 210, fps);
        assert_eq!(w.gameplay.oxygen01, 0.0);
        assert!(w.gameplay.game_over);
        (half, w.gameplay.collected_resources)
    };
    let (slow, fast) = (run(30), run(144));
    assert!((slow.0 - fast.0).abs() < 1e-10);
    assert_eq!((slow.1, fast.1), (0, 0));
}

#[test]
fn world_pickup_recovers_once_and_disappears_from_render_submission() {
    let mut w = world();
    w.gameplay.oxygen01 = 0.5;
    let mut fb = Framebuffer::new(160, 90, SPACE_COLOR).unwrap();
    let before = w.render(&mut fb).objects_submitted;
    collect_resource(&mut w, 0);
    let expected =
        0.5 - w.gameplay.settings.drain_per_second / 60.0 + w.gameplay.settings.pickup_recovery;
    assert!((w.gameplay.oxygen01 - expected).abs() < 1e-12);
    assert_eq!(w.gameplay.resource_events, vec![0]);
    assert_eq!(w.gameplay.collected_resources, 1);
    assert!(w.gameplay.resources[0].collected);
    assert_eq!(w.render(&mut fb).objects_submitted, before - 1);
    collect_resource(&mut w, 0);
    assert_eq!(w.gameplay.collected_resources, 1);
    assert!(w.gameplay.resource_events.is_empty());
    assert!(
        (w.gameplay.oxygen01 - (expected - w.gameplay.settings.drain_per_second / 60.0)).abs()
            < 1e-12
    );
}

#[test]
fn world_boost_crossing_collects_resource_between_frame_endpoints() {
    let mut w = world();
    w.toggles.freeze_orbits = true;
    w.gameplay.oxygen01 = 0.5;
    let pickup = w.gameplay.resources[0].position(&w.system, 0.0).unwrap();
    // Trayecto a altura polar: no intersecta el planeta ni el registro desplazado en X.
    w.ship = Ship::new(pickup - Vec3::Z * 20.0, Vec3::Z);
    w.ship.speed = 300.0;
    let start = w.ship.position;
    w.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        0.15,
    );
    assert!((start - pickup).length() > w.gameplay.settings.pickup_radius);
    assert!((w.ship.position - pickup).length() > w.gameplay.settings.pickup_radius);
    assert!((w.ship.position - start).length() > 40.0);
    assert_eq!(
        w.gameplay.collected_resources, 1,
        "World debe conectar el barrido de recoleccion"
    );
    assert_eq!(w.gameplay.resource_events, vec![0]);
    assert!(
        (w.gameplay.oxygen01 - (0.65 - w.gameplay.settings.drain_per_second * 0.15)).abs() < 1e-12
    );
}

#[test]
fn world_all_six_log_orders_require_sun_then_freeze_victory() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut w = world();
        arrive_at_sun(&mut w);
        assert!(!w.gameplay.objective_complete);
        for (count, i) in order.into_iter().enumerate() {
            collect_log(&mut w, i);
            assert_eq!(w.gameplay.collected_logs, count as u32 + 1);
            assert!(!w.gameplay.objective_complete);
        }
        arrive_at_sun(&mut w);
        assert!(w.gameplay.objective_complete);
        assert!(!w.gameplay.game_over);
        let snapshot = (
            w.ship.position,
            w.time,
            w.gameplay.oxygen01,
            w.camera.position,
        );
        w.update(
            &ShipInput {
                thrust: 1.0,
                boost: true,
                ..ShipInput::default()
            },
            1.0,
        );
        assert_eq!(
            snapshot,
            (
                w.ship.position,
                w.time,
                w.gameplay.oxygen01,
                w.camera.position
            )
        );
    }
}

#[test]
fn world_death_and_three_restarts_restore_catalog_and_camera() {
    let mut w = world();
    let initial_camera = w.camera.position;
    let initial_ship = w.ship.position;
    for _ in 0..3 {
        collect_resource(&mut w, 0);
        collect_log(&mut w, 1);
        w.orbit_assist = true;
        w.set_debug_oxygen(0.9);
        w.gameplay.oxygen01 = w.gameplay.settings.drain_per_second / 120.0;
        idle(&mut w, 1, 60);
        assert!(w.gameplay.game_over);
        let snapshot = (w.ship.position, w.time, w.gameplay.oxygen01);
        w.update(
            &ShipInput {
                thrust: 1.0,
                ..ShipInput::default()
            },
            1.0,
        );
        assert_eq!(snapshot, (w.ship.position, w.time, w.gameplay.oxygen01));
        w.restart_game();
        assert_eq!((w.gameplay.resources.len(), w.gameplay.logs.len()), (3, 3));
        assert!(w.gameplay.resources.iter().all(|r| !r.collected));
        assert!(w.gameplay.logs.iter().all(|r| !r.anchor.collected));
        assert_eq!(
            (w.gameplay.collected_resources, w.gameplay.collected_logs),
            (0, 0)
        );
        assert!(w.gameplay.resource_events.is_empty() && w.gameplay.log_events.is_empty());
        assert_eq!(w.gameplay.last_log, None);
        assert!(!w.gameplay.game_over && !w.gameplay.objective_complete && !w.orbit_assist);
        assert_eq!(w.forced_oxygen, None);
        assert_eq!(w.oxygen01(), 1.0);
        assert_eq!(w.time, 0.0);
        assert_eq!(w.ship.position, initial_ship);
        assert_eq!(w.camera.position, initial_camera);
        assert_eq!(w.gameplay.navigation.assisted_planet, None);
        assert_eq!(w.gameplay.navigation.gravity_velocity, Vec3::ZERO);
    }
}

#[test]
fn world_assist_circles_moving_planet_with_camera_at_both_frame_rates() {
    for fps in [30, 144] {
        let mut w = world();
        let radius = w.system.bodies[1].body_radius + w.gameplay.navigation.settings.orbit_gap;
        let initial_center = w.system.bodies[1].position(0.0);
        w.ship = Ship::new(initial_center + Vec3::X * radius, -Vec3::Z);
        w.orbit_assist = true;
        let mut previous = Vec3::X;
        let mut swept = 0.0;
        for _ in 0..fps * 15 {
            idle(&mut w, 1, fps);
            let offset = w.ship.position - w.system.bodies[1].position(w.time);
            let radial = offset.normalized();
            // Angulo firmado sobre Y: exige circulo real y no oscilacion acumulada.
            swept += previous.cross(radial).y.atan2(previous.dot(radial));
            previous = radial;
            assert!((offset.length() - radius).abs() < 1e-8);
            assert_eq!(w.gameplay.navigation.assisted_planet, Some(1));
            assert!(
                ((w.camera.position - w.ship.position).length() - 22.0_f64.hypot(6.0)).abs() < 1e-8
            );
            assert!(w.camera.forward().is_finite());
            let toward_planet =
                (w.system.bodies[1].position(w.time) - w.camera.position).normalized();
            let alignment = w.camera.forward().dot(toward_planet);
            assert!(
                alignment > 0.99,
                "fps={fps}, camera/planet alignment={alignment}"
            );
            assert!((w.camera.up.length() - 1.0).abs() < 1e-8);
            assert!(w.camera.up.cross(w.camera.forward()).length() > 0.1);
        }
        assert!(swept > TAU, "fps={fps}, swept={swept}");
        assert!((w.system.bodies[1].position(w.time) - initial_center).length() > 100.0);
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
}

#[test]
fn world_large_step_death_precedes_collection_and_victory_stays_terminal() {
    let mut w = world();
    for i in 0..3 {
        collect_log(&mut w, i);
    }
    let sun = &w.system.bodies[0];
    w.ship = Ship::new(Vec3::Z * (sun.body_radius + 20.0), -Vec3::Z);
    let before = (w.ship.position, w.camera.position, w.time);
    // El contrato actual consume primero: sin oxigeno no hay movimiento ni victoria.
    w.update(&ShipInput::default(), 421.0);
    assert!(w.gameplay.game_over && !w.gameplay.objective_complete);
    assert_eq!(w.gameplay.oxygen01, 0.0);
    assert_eq!(before, (w.ship.position, w.camera.position, w.time));
    w.restart_game();
    let pickup = w.gameplay.resources[0].position(&w.system, w.time).unwrap();
    w.ship = Ship::new(pickup, -Vec3::Z);
    w.update(&ShipInput::default(), 421.0);
    assert!(w.gameplay.game_over);
    assert_eq!(w.gameplay.collected_resources, 0);
    assert!(!w.gameplay.resources[0].collected);

    w.restart_game();
    for i in 0..3 {
        collect_log(&mut w, i);
    }
    arrive_at_sun(&mut w);
    assert!(w.gameplay.objective_complete);
    let won = (
        w.ship.position,
        w.camera.position,
        w.time,
        w.gameplay.oxygen01,
    );
    w.update(
        &ShipInput {
            thrust: 1.0,
            boost: true,
            ..ShipInput::default()
        },
        10_000.0,
    );
    assert!(w.gameplay.objective_complete && !w.gameplay.game_over);
    assert_eq!(
        won,
        (
            w.ship.position,
            w.camera.position,
            w.time,
            w.gameplay.oxygen01
        )
    );
}

#[test]
fn smoke_three_deterministic_runs_with_frames_and_real_framebuffer() {
    let mut reference = None;
    for run in 0..3 {
        let mut w = world();
        let mut fb = Framebuffer::new(160, 90, SPACE_COLOR).unwrap();
        let mut signature = Vec::new();
        let mut frames = 0;
        // Setup explicito de posiciones; los updates y el render son reales.
        for i in [2, 0, 1] {
            collect_resource(&mut w, i);
            collect_log(&mut w, i);
            frames += 2;
            for _ in 0..12 {
                idle(&mut w, 1, 60);
                frames += 1;
                let stats = w.render(&mut fb);
                assert!(stats.triangles_rasterized > 0 && stats.pixels_written > 0);
                assert!(fb.pixels.iter().any(|&p| p != SPACE_COLOR));
                let hash = fb.pixels.iter().fold(0xcbf29ce484222325u64, |h, p| {
                    (h ^ u64::from(*p)).wrapping_mul(0x100000001b3)
                });
                signature.push((hash, stats));
            }
        }
        arrive_at_sun(&mut w);
        frames += 1;
        let final_stats = w.render(&mut fb);
        assert!(final_stats.pixels_written > 0);
        assert_eq!(
            (w.gameplay.collected_resources, w.gameplay.collected_logs),
            (3, 3)
        );
        assert!(w.gameplay.objective_complete && !w.gameplay.game_over);
        println!(
            "smoke run={run}: frames={frames}, renders={}, first_hash={:016x}, final_pixels={}",
            signature.len() + 1,
            signature[0].0,
            final_stats.pixels_written
        );
        let result = (
            signature,
            fb.pixels,
            final_stats,
            w.time,
            w.gameplay.oxygen01,
        );
        if let Some(expected) = &reference {
            assert_eq!(&result, expected);
        } else {
            reference = Some(result);
        }
    }
}
