//! Real private capture fixture + gameplay reconciliation (explicit opt-in).
use std::path::PathBuf;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    gameplay::Collectible,
    math::{Mat4, Vec3},
    scene::{ship::ShipInput, world::World},
    visual::{
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, solar_creature_placement},
        static_creature::StaticCreature,
        tentacle_auto_capture::*,
        tentacle_grasp_closure::ClosureClip,
        tentacle_grasp_preview::{GraspPreview, PreviewInputs, arm_rest, load_preview_inputs},
        tentacle_solar_game::TentacleCaptureRequest,
        tentacle_spatial_pursuit::SpatialPursuit,
    },
};
const DT: f64 = 1.0 / 60.0;
fn env(key: &str) -> PathBuf {
    std::env::var_os(key).expect(key).into()
}
fn game() -> (World, SolarGame, OxygenTransition) {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    let c = StaticCreature::load(
        &env("UMBRAL_CAPTURE_CREATURE_OBJ"),
        &env("UMBRAL_CREATURE_TEXTURE"),
        solar_creature_placement(&w).unwrap(),
    )
    .unwrap();
    let g = SolarGame::start(&mut w, c).unwrap();
    (w, g, OxygenTransition::new(1.0).unwrap())
}
macro_rules! fixture {
    ($w:ident,$g:ident,$o:ident,$a:ident) => {
        let (mut $w, mut $g, mut $o) = game();
        let request = TentacleCaptureRequest {
            capture_dir: env("UMBRAL_CAPTURE_PACKAGE_DIR"),
            growth_package: env("UMBRAL_GROWTH_PACKAGE"),
            correspondence: env("UMBRAL_ARM_CORRESPONDENCE"),
            start_time: None,
        };
        let PreviewInputs { debug, package } = load_preview_inputs(&request).unwrap();
        let mut p = SpatialPursuit::new(debug);
        p.bind(&$w).unwrap();
        p.step(&$w, 0.0).unwrap();
        let rest = arm_rest(&$w, &p).unwrap();
        let clip = ClosureClip::new(&package, &rest).unwrap();
        let mut $a = AutoCapture::new(GraspPreview::new(&clip, p).unwrap()).unwrap();
    };
}
fn step(
    w: &mut World,
    g: &mut SolarGame,
    o: &mut OxygenTransition,
    a: &mut AutoCapture,
    c: AutoControls,
) -> AutoStatus {
    advance_auto_capture_frame(w, g, o, a, c, DT)
        .unwrap()
        .status
        .unwrap()
}
fn contact(w: &mut World, g: &mut SolarGame, o: &mut OxygenTransition, a: &mut AutoCapture) {
    for i in 0..3000 {
        let ship = ShipInput {
            thrust: if i < 300 { 1.0 } else { 0.0 },
            yaw: 1.0,
            pitch: if i < 300 { 0.0 } else { 1.0 },
            roll: if i < 300 { 0.0 } else { 1.0 },
            boost: i >= 300,
        };
        let s = step(
            w,
            g,
            o,
            a,
            AutoControls {
                ship,
                oxygen: OxygenKeys {
                    preset: (i == 0).then_some(0.05),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        if s.events.contains(&AutoEvent::Contact) {
            assert!(a.flight_locked());
            return;
        }
        assert_eq!(s.phase, AutoPhase::Pursuit);
    }
    panic!("fixture moving fresh contact not reached");
}
fn anchor_at(w: &World, p: Vec3, id: usize) -> Collectible {
    Collectible {
        id,
        planet_index: 0,
        local_offset: Mat4::rotation_y(-w.system.bodies[0].rotation_angle(w.time)).transform_dir(p),
        collected: false,
    }
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn captured_pose_excludes_gravity_assist_collision_and_provisional_pickups() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    let pin = w.ship.position;
    // An adversarial new planet surrounds the pin. Neutral-input physics would separate it.
    w.system.bodies[1].orbit.radius = pin.x.hypot(pin.z);
    w.system.bodies[1].phase = pin.z.atan2(pin.x) - w.system.bodies[1].orbit.angular_speed * w.time;
    w.system.bodies[1].body_radius = pin.y.abs() + 80.0;
    w.orbit_assist = true;
    w.gameplay.navigation.settings.gravity_strength = 30.0;
    w.gameplay.navigation.gravity_velocity = Vec3::X * 6000.0;
    w.gameplay.settings.pickup_radius = 0.5;
    let mut provisional = w.ship;
    let mut nav = w.gameplay.navigation.clone();
    nav.apply_gravity(&mut provisional, &w.system, w.time + DT, DT);
    nav.assist(
        &mut provisional,
        &ShipInput::default(),
        &w.system,
        w.time,
        w.time + DT,
        DT,
        true,
    );
    nav.resolve_collisions(&mut provisional, pin, &w.system, w.time, w.time + DT);
    assert!(
        (provisional.position - pin).length() > 2.0,
        "adversarial fixture did not move provisional pose"
    );
    w.gameplay.resources = vec![anchor_at(&w, provisional.position, 99)];
    let before = w.gameplay.oxygen01;
    let s = step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    assert_eq!(
        w.gameplay.collected_resources, 0,
        "pickup from discarded provisional physics pose"
    );
    assert_eq!(w.ship.position, pin);
    assert_eq!(s.phase, AutoPhase::Closing);
    assert!(w.gameplay.oxygen01 < before);
    assert_eq!(w.gameplay.navigation.gravity_velocity, Vec3::ZERO);
    assert_eq!(w.gameplay.navigation.assisted_planet, None);
}

#[test]
#[ignore = "private fixture; joint runtime"]
fn authoritative_drag_path_collects_once_and_can_complete_objective() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    while a.status().phase != AutoPhase::Dragging {
        step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    }
    for _ in 0..20 {
        step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    }
    w.toggles.freeze_orbits = true;
    let start = w.ship.position;
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    let delta = w.ship.position - start;
    assert!(delta.length() > 0.01);
    let marker = w.ship.position + delta * 0.5;
    w.gameplay.settings.pickup_radius = delta.length() * 0.4;
    w.gameplay.resources = vec![anchor_at(&w, marker, 77)];
    let prior = w.ship.position;
    let reserve = w.gameplay.oxygen01;
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    let segment = w.ship.position - prior;
    let t = ((marker - prior).dot(segment) / segment.dot(segment)).clamp(0.0, 1.0);
    assert!(
        (marker - (prior + segment * t)).length() < w.gameplay.settings.pickup_radius,
        "fixture marker outside committed carry path"
    );
    assert!((marker - prior).length() > w.gameplay.settings.pickup_radius);
    assert_eq!(
        w.gameplay.resource_events,
        vec![77],
        "final committed carry trajectory was not collected"
    );
    assert!(w.gameplay.oxygen01 > reserve);
    assert_eq!(
        w.forced_oxygen,
        Some(0.05),
        "pickup disabled explicit visual override"
    );
    assert!(
        a.flight_locked(),
        "low visual override must not promise release from a capsule"
    );
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    assert_eq!(w.gameplay.collected_resources, 1);
    assert!(w.gameplay.resource_events.is_empty());
    w.gameplay.collected_logs = 3;
    w.gameplay.settings.sun_arrival_gap = w.ship.position.length() + 1.0;
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    assert!(
        w.gameplay.objective_complete,
        "objective ignored authoritative captured pose"
    );
    let snapshot = a.status();
    let ship = w.ship.position;
    step(
        &mut w,
        &mut g,
        &mut o,
        &mut a,
        AutoControls {
            emergency: true,
            ..Default::default()
        },
    );
    assert_eq!(a.status(), snapshot);
    assert_eq!(w.ship.position, ship);
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn full_restart_invalidates_capture_even_from_terminal_and_resets_visual_clock() {
    use umbral::app::{GameplayAction, apply_gameplay_action};
    for terminal in [false, true] {
        fixture!(w, g, o, a);
        contact(&mut w, &mut g, &mut o, &mut a);
        w.gameplay.collected_logs = 2;
        w.gameplay.game_over = terminal;
        apply_gameplay_action(&mut w, GameplayAction::Restart);
        step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
        assert!(
            !a.flight_locked(),
            "full restart retained old capture ownership"
        );
        assert_eq!(a.status().phase, AutoPhase::Pursuit);
        assert_eq!(a.status().contact_pose, None);
        assert_eq!(w.gameplay.collected_logs, 0);
        assert!(!w.gameplay.game_over);
        assert_eq!(o.target(), w.gameplay.oxygen01);
        assert!(
            o.shown() > 0.99,
            "full restart retained old visual override transition"
        );
        assert_eq!(w.forced_oxygen, None);
    }
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn beacons_render_once_in_world_and_eye_visible_solar_compositor() {
    use umbral::renderer::framebuffer::Framebuffer;
    let (mut w, g, _) = game();
    w.set_debug_oxygen(0.05);
    w.snap_creature_reveal();
    assert!(w.solar_reveal_alphas().unwrap().eye > 0.0);
    let mut fb = Framebuffer::new(160, 90, 0).unwrap();
    let world_before = w.render(&mut fb);
    let solar_before = g.render(&w, &mut fb).unwrap();
    for r in &mut w.gameplay.resources {
        r.collected = true;
    }
    for r in &mut w.gameplay.logs {
        r.anchor.collected = true;
    }
    let world_after = w.render(&mut fb);
    let solar_after = g.render(&w, &mut fb).unwrap();
    assert_eq!(
        world_before.objects_submitted - world_after.objects_submitted,
        6
    );
    assert_eq!(
        solar_before.objects_submitted - solar_after.objects_submitted,
        6,
        "solar compositor omitted gameplay beacons"
    );
    assert_eq!(
        world_before.triangles_submitted - world_after.triangles_submitted,
        solar_before.triangles_submitted - solar_after.triangles_submitted
    );
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn captured_real_oxygen_pickup_publishes_final_reserve_with_one_smoothing_step() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    w.gameplay.oxygen01 = 0.05;
    w.clear_debug_oxygen();
    o = OxygenTransition::new(0.05).unwrap();
    let marker = anchor_at(&w, w.ship.position, 88);
    w.gameplay.resources = vec![marker];
    let before = o.shown();
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    assert_eq!(w.gameplay.resource_events, vec![88]);
    assert_eq!(
        o.target(),
        w.gameplay.oxygen01,
        "transition target omitted authoritative pickup"
    );
    assert!(
        (o.shown() - (before + DT * 0.25)).abs() < 1e-12,
        "pickup smoothing advanced zero or twice"
    );
    assert_eq!(w.debug_oxygen01, o.shown());
    assert_eq!(w.oxygen01(), o.shown());
    assert_eq!(w.forced_oxygen, None);
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn direct_release_clears_navigation_without_teleport_and_reject_restores_navigation() {
    for emergency in [false, true] {
        fixture!(w, g, o, a);
        contact(&mut w, &mut g, &mut o, &mut a);
        reach_held(&mut w, &mut g, &mut o, &mut a);
        w.orbit_assist = true;
        w.gameplay.navigation.assisted_planet = Some(1);
        w.gameplay.navigation.gravity_velocity = Vec3::X * 999.0;
        let nav = format!("{:?}", w.gameplay.navigation);
        let before = a.status();
        let camera = format!("{:?}", w.camera);
        assert!(a.frame(&mut w, f64::NAN, false).is_err());
        assert_eq!(format!("{:?}", w.gameplay.navigation), nav);
        assert_eq!(format!("{:?}", w.camera), camera);
        assert_eq!(a.status(), before);
        if !emergency {
            w.set_debug_oxygen(0.5);
        }
        let pose = (w.ship.position, w.ship.orientation);
        a.frame(&mut w, DT, emergency).unwrap();
        assert!(!a.flight_locked());
        assert_eq!((w.ship.position, w.ship.orientation), pose);
        assert_eq!(w.ship.speed, 0.0);
        assert_eq!(
            w.gameplay.navigation.gravity_velocity,
            Vec3::ZERO,
            "release retained stale navigation velocity"
        );
        assert_eq!(w.gameplay.navigation.assisted_planet, None);
    }
}
#[test]
#[ignore = "private fixture; correction lifecycle"]
fn correction_newgame_resets_nested_lifetime_without_synthetic_frame() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    assert!(a.carry().status().preview.frame > 10);
    for _ in 0..3 {
        w.gameplay.game_over = true;
        w.restart_game();
        step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
        let s = a.status();
        assert_eq!(s.frame, 1);
        assert_eq!(
            s.carry.frame, 1,
            "new game retained carry lifetime / synthetic X frame"
        );
        assert_eq!(
            s.carry.preview.frame, 1,
            "new game retained preview lifetime"
        );
        assert_eq!(
            s.carry.preview.quiet_frames, 0,
            "new game retained previous transforms"
        );
        assert!(!s.carry.preview.matrices_stable);
        assert_eq!(s.contact_pose, None);
    }
}

#[test]
#[ignore = "private fixture; correction route state"]
fn correction_restart_restores_solar_g_h_and_body_state() {
    use umbral::visual::{creature_eye_comparison::EyeTarget, solar_game::SolarKeys};
    fixture!(w, g, o, a);
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    let initial = w.creature_horizontal_follow().unwrap();
    g.apply(
        &mut w,
        SolarKeys {
            oblique: true,
            target: true,
            ..Default::default()
        },
    )
    .unwrap();
    w.update_creature_eye_follow(1.0);
    g.apply(
        &mut w,
        SolarKeys {
            follow: true,
            ..Default::default()
        },
    )
    .unwrap();
    w.restart_game();
    step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
    assert_eq!(
        w.creature_eye_target(),
        Some(EyeTarget::Camera),
        "newgame retained G target"
    );
    assert_eq!(
        format!("{:?}", w.creature_horizontal_follow().unwrap()),
        format!("{initial:?}"),
        "newgame retained H/yaw/velocity"
    );
}

#[test]
#[ignore = "private fixture; correction final oxygen reaction"]
fn correction_authoritative_pickup_crosses_threshold_and_reacts_same_frame() {
    for dragging in [false, true] {
        fixture!(w, g, o, a);
        contact(&mut w, &mut g, &mut o, &mut a);
        if dragging {
            while a.status().phase != AutoPhase::Dragging {
                step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
            }
        }
        w.toggles.freeze_orbits = true;
        w.gameplay.oxygen01 = 0.149;
        w.clear_debug_oxygen();
        o = OxygenTransition::new(0.149).unwrap();
        w.gameplay.settings.pickup_radius = 1000.0;
        w.gameplay.resources = vec![anchor_at(&w, w.ship.position, 188)];
        let before = a.status().frame;
        let s = step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
        assert_eq!(w.gameplay.resource_events, vec![188]);
        assert!(o.shown() > 0.15, "fixture did not cross threshold");
        assert!((o.shown() - (0.149 + DT * 0.25)).abs() < 1e-12);
        assert_eq!(s.frame, before + 1, "reaction simulated another frame");
        assert!(
            !a.flight_locked(),
            "authoritative pickup recovery lagged one frame"
        );
        assert_eq!(s.phase, AutoPhase::Observation);
        assert_eq!(
            s.events
                .iter()
                .filter(|e| matches!(e, AutoEvent::Released(ReleaseCause::OxygenRecovered)))
                .count(),
            1
        );
        assert_eq!(w.gameplay.navigation.gravity_velocity, Vec3::ZERO);
    }
}

#[test]
#[ignore = "private fixture; correction terminal direct API"]
fn correction_direct_auto_terminal_freezes_and_keeps_preflight_rejection() {
    for held in [false, true] {
        fixture!(w, g, o, a);
        contact(&mut w, &mut g, &mut o, &mut a);
        if held {
            reach_held(&mut w, &mut g, &mut o, &mut a);
        }
        w.gameplay.game_over = true;
        let status = a.status();
        let pose = (w.ship.position, w.ship.orientation);
        let mesh = a.body_override().unwrap().model.vertices.clone();
        a.frame(&mut w, DT, true).unwrap();
        assert_eq!(
            a.status(),
            status,
            "direct terminal advanced capture or processed X"
        );
        assert_eq!((w.ship.position, w.ship.orientation), pose);
        assert_eq!(a.body_override().unwrap().model.vertices, mesh);
        assert!(a.frame(&mut w, f64::NAN, true).is_err());
        w.clear_creature();
        assert!(
            a.frame(&mut w, DT, true).is_err(),
            "terminal bypassed missing model rejection"
        );
        assert_eq!(a.status(), status);
    }
}

#[test]
#[ignore = "private fixture; correction direct restart"]
fn correction_direct_newgame_discards_capture_and_nested_clocks() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    w.gameplay.game_over = true;
    w.restart_game();
    let s = a.frame(&mut w, 0.0, false).unwrap();
    assert_eq!(
        s.phase,
        AutoPhase::Pursuit,
        "direct newgame retained capture episode"
    );
    assert_eq!(s.frame, 1);
    assert_eq!(s.carry.frame, 1);
    assert_eq!(s.carry.preview.frame, 1);
    assert_eq!(s.contact_pose, None);
    assert_eq!(w.ship.position, Vec3::new(0.0, 0.0, 298.0));
}

#[test]
#[ignore = "private fixture; correction nonauto terminal"]
fn correction_nonauto_terminal_freezes_body_reveal_and_clock() {
    let (mut w, mut g, _) = game();
    use umbral::visual::solar_game::SolarKeys;
    w.set_debug_oxygen(0.05);
    g.apply(
        &mut w,
        SolarKeys {
            oblique: true,
            ..Default::default()
        },
    )
    .unwrap();
    umbral::app::advance_frame(&mut w, &ShipInput::default(), 0.1);
    w.gameplay.game_over = true;
    let body = w.creature_render_transform().unwrap().matrix();
    let reveal = w.solar_reveal_alphas().unwrap();
    let gaze = format!("{:?}", w.creature_eye_follow());
    let clock = w.time;
    umbral::app::advance_frame(&mut w, &ShipInput::default(), 0.1);
    assert_eq!(
        w.creature_render_transform().unwrap().matrix(),
        body,
        "terminal nonauto advanced body"
    );
    assert_eq!(format!("{:?}", w.creature_eye_follow()), gaze);
    assert_eq!(
        format!("{:?}", w.solar_reveal_alphas().unwrap()),
        format!("{reveal:?}")
    );
    assert_eq!(w.time, clock);
}

#[test]
#[ignore = "private fixture; correction terminal identity"]
fn correction_terminal_pursuit_must_not_accept_foreign_equal_geometry_identity() {
    let (mut w, _g, _o) = game();
    let request = TentacleCaptureRequest {
        capture_dir: env("UMBRAL_CAPTURE_PACKAGE_DIR"),
        growth_package: env("UMBRAL_GROWTH_PACKAGE"),
        correspondence: env("UMBRAL_ARM_CORRESPONDENCE"),
        start_time: None,
    };
    let PreviewInputs { debug, package } = load_preview_inputs(&request).unwrap();
    let mut p = SpatialPursuit::new(debug);
    p.bind(&w).unwrap();
    p.step(&w, 0.0).unwrap();
    let rest = arm_rest(&w, &p).unwrap();
    let clip = ClosureClip::new(&package, &rest).unwrap();
    let mut a = AutoCapture::new(GraspPreview::new(&clip, p).unwrap()).unwrap();
    let mut foreign_model = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
    foreign_model
        .material_libraries
        .push("correction-foreign-identity.mtl".into());
    let foreign = StaticCreature::new(
        foreign_model,
        umbral::renderer::texture::Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap(),
        solar_creature_placement(&w).unwrap(),
    )
    .unwrap();
    let _foreign_game = SolarGame::start(&mut w, foreign).unwrap();
    let rejected = a.frame(&mut w, DT, false).unwrap();
    assert!(
        rejected.carry.preview.events.iter().any(|e| matches!(
            e,
            umbral::visual::tentacle_grasp_preview::PreviewEvent::PursuitStepRejected(_)
        )),
        "adversarial identity did not trigger original step guard"
    );
    w.gameplay.game_over = true;
    let before = a.status();
    let result = a.frame(&mut w, DT, false);
    assert!(
        result.is_err(),
        "terminal bypassed exact bound identity: active path rejected but frozen path accepted"
    );
    assert_eq!(a.status(), before);
}

fn reach_held(w: &mut World, g: &mut SolarGame, o: &mut OxygenTransition, a: &mut AutoCapture) {
    for _ in 0..1400 {
        let s = step(w, g, o, a, AutoControls::default());
        if s.phase == AutoPhase::Held {
            return;
        }
        assert!(a.flight_locked(), "fixture lost capture: {s:?}");
    }
    panic!("fixture did not reach Held");
}
#[test]
#[ignore = "private fixture; joint runtime"]
fn real_death_freezes_closing_and_held_capture_pose_mesh_and_clock_even_on_x() {
    for held in [false, true] {
        fixture!(w, g, o, a);
        contact(&mut w, &mut g, &mut o, &mut a);
        if held {
            reach_held(&mut w, &mut g, &mut o, &mut a);
        }
        let before = a.status();
        let mesh = a.body_override().unwrap().model.vertices.clone();
        let ship = format!("{:?}", w.ship);
        let camera = format!("{:?}", w.camera);
        let time = w.time;
        w.gameplay.oxygen01 = w.gameplay.settings.drain_per_second * DT * 0.5;
        step(&mut w, &mut g, &mut o, &mut a, AutoControls::default());
        assert!(w.gameplay.game_over);
        assert_eq!(a.status(), before, "terminal advanced capture/carry clock");
        for _ in 0..3 {
            step(
                &mut w,
                &mut g,
                &mut o,
                &mut a,
                AutoControls {
                    emergency: true,
                    ..Default::default()
                },
            );
            assert_eq!(a.status(), before);
            assert_eq!(a.body_override().unwrap().model.vertices, mesh);
            assert_eq!(format!("{:?}", w.ship), ship);
            assert_eq!(format!("{:?}", w.camera), camera);
            assert_eq!(w.time, time);
            assert!(a.flight_locked());
        }
    }
}
