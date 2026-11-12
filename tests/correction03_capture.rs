//! CORRECTION03 (fixture privado, opt-in): identidad EXACTA y preflight en las APIs COMPARTIDAS de
//! terminal y reinicio. El cuadro compartido no debe devolver `frozen` ni reenlazar en silencio una
//! criatura ajena (misma geometria, otra identidad) que la ruta activa rechaza.
use std::path::PathBuf;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    scene::{ship::ShipInput, world::World},
    visual::{
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
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
/// Misma geometria y textura; solo cambia la identidad (una libreria de materiales extra).
fn foreign(w: &World) -> StaticCreature {
    let mut model = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
    model
        .material_libraries
        .push("correction03-foreign-identity.mtl".into());
    StaticCreature::new(
        model,
        umbral::renderer::texture::Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap(),
        solar_creature_placement(w).unwrap(),
    )
    .unwrap()
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
fn contact(w: &mut World, g: &mut SolarGame, o: &mut OxygenTransition, a: &mut AutoCapture) {
    for i in 0..3000 {
        let ship = ShipInput {
            thrust: if i < 300 { 1.0 } else { 0.0 },
            yaw: 1.0,
            pitch: if i < 300 { 0.0 } else { 1.0 },
            roll: if i < 300 { 0.0 } else { 1.0 },
            boost: i >= 300,
        };
        let s = advance_auto_capture_frame(
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
            DT,
        )
        .unwrap()
        .status
        .unwrap();
        if s.events.contains(&AutoEvent::Contact) {
            return;
        }
    }
    panic!("fixture moving fresh contact not reached");
}
/// Todo lo que un cuadro compartido podria mutar: nave, camara, O2, captura y vista solar.
fn snapshot(w: &World, g: &SolarGame, o: &OxygenTransition, a: &AutoCapture) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        w.ship,
        w.camera,
        (o.shown(), o.target()),
        a.status(),
        g.view(),
        (w.time, w.debug_oxygen01, w.forced_oxygen),
        (w.creature_horizontal_follow(), w.creature_eye_target()),
        a.body_override().map(|b| b.model.vertices.clone())
    )
}
fn all_keys() -> AutoControls {
    AutoControls {
        ship: ShipInput {
            thrust: 1.0,
            yaw: 1.0,
            pitch: 1.0,
            roll: 1.0,
            boost: true,
        },
        oxygen: OxygenKeys {
            preset: Some(1.0),
            increase: true,
            decrease: false,
        },
        solar: SolarKeys {
            reset: true,
            frontal: true,
            oblique: true,
            follow: true,
            target: true,
        },
        emergency: true,
    }
}

#[test]
#[ignore = "private fixture; correction03 shared terminal identity"]
fn c3_shared_terminal_before_first_frame_validates_exact_identity() {
    fixture!(w, g, o, a);
    w.gameplay.game_over = true;
    // Control: la MISMA criatura en terminal antes del primer cuadro congela todo (ni teclas ni X).
    let before = snapshot(&w, &g, &o, &a);
    let out = advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, all_keys(), DT).unwrap();
    assert!(out.status.is_ok());
    assert_eq!(
        snapshot(&w, &g, &o, &a),
        before,
        "terminal shared frame mutated"
    );
    // Criatura ajena de geometria identica: la ruta activa la rechaza; el cuadro congelado tambien.
    let alien = foreign(&w);
    let _foreign = SolarGame::start(&mut w, alien).unwrap();
    w.gameplay.game_over = true;
    let before = snapshot(&w, &g, &o, &a);
    assert!(
        advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, all_keys(), DT).is_err(),
        "shared terminal returned frozen over a foreign bound identity"
    );
    assert_eq!(snapshot(&w, &g, &o, &a), before, "rejected frame mutated");
    w.clear_creature();
    assert!(
        advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, all_keys(), DT).is_err(),
        "shared terminal accepted a missing creature"
    );
}

#[test]
#[ignore = "private fixture; correction03 shared terminal identity while captured"]
fn c3_shared_terminal_with_capture_rejects_foreign_identity_atomically() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    let alien = foreign(&w);
    let _foreign = SolarGame::start(&mut w, alien).unwrap();
    w.gameplay.game_over = true;
    let before = snapshot(&w, &g, &o, &a);
    assert!(
        advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, all_keys(), DT).is_err(),
        "shared terminal returned frozen over a foreign identity with a live capture"
    );
    assert_eq!(snapshot(&w, &g, &o, &a), before);
}

#[test]
#[ignore = "private fixture; correction03 shared restart identity"]
fn c3_shared_restart_does_not_silently_rebind_a_foreign_creature() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    let alien = foreign(&w);
    let _foreign = SolarGame::start(&mut w, alien).unwrap();
    w.restart_game();
    let before = snapshot(&w, &g, &o, &a);
    assert!(
        advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, AutoControls::default(), DT)
            .is_err(),
        "shared restart silently re-bound a foreign creature"
    );
    assert_eq!(
        snapshot(&w, &g, &o, &a),
        before,
        "rejected restart applied the solar reset or replaced the episode"
    );
    assert!(
        a.flight_locked(),
        "rejected restart dropped the bound episode"
    );
}

#[test]
#[ignore = "private fixture; correction03 shared restart control"]
fn c3_shared_restart_with_the_same_creature_still_rebinds_cleanly() {
    fixture!(w, g, o, a);
    contact(&mut w, &mut g, &mut o, &mut a);
    w.restart_game();
    let out =
        advance_auto_capture_frame(&mut w, &mut g, &mut o, &mut a, AutoControls::default(), DT)
            .unwrap();
    let s = out.status.unwrap();
    assert_eq!(s.phase, AutoPhase::Pursuit);
    assert_eq!(s.contact_pose, None);
    assert!(!a.flight_locked());
}
