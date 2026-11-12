//! Muerte por el monstruo: al llegar el O2 real a 0 la secuencia jala la nave, la captura la
//! hace el tentaculo aprobado y la muerte llega con la nave retenida (o por respaldo).

use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    scene::{
        ship::{Ship, ShipInput},
        world::World,
    },
    visual::{
        monster_death::{
            DEATH_OXYGEN, FALLBACK_DEATH_SECONDS, HOLD_BEFORE_DEATH, MonsterDeath,
            PULL_STOP_DISTANCE, monster_center, pull_input,
        },
        tentacle_auto_capture::AutoPhase,
    },
};

fn world() -> World {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.clear_debug_oxygen();
    w
}

#[test]
fn con_oxigeno_no_pasa_nada_y_el_jugador_conserva_el_control() {
    let mut w = world();
    let mut m = MonsterDeath::new();
    let player = ShipInput {
        thrust: 1.0,
        yaw: 0.5,
        ..ShipInput::default()
    };
    let got = m.before_frame(&mut w, player, AutoPhase::Pursuit);
    assert_eq!((got.thrust, got.yaw), (1.0, 0.5));
    assert!(!m.active() && !w.dying);
    assert!(!m.after_frame(&mut w, AutoPhase::Pursuit, 0.1));
}

#[test]
fn al_llegar_a_cero_empieza_la_secuencia_y_no_se_muere_ahogado() {
    let mut w = world();
    let mut m = MonsterDeath::new();
    w.gameplay.oxygen01 = 0.0; // el cuadro anterior lo dejo en 0 (N o consumo)
    assert!(!m.after_frame(&mut w, AutoPhase::Pursuit, 0.1));
    assert!(m.active() && w.dying);
    assert_eq!(w.gameplay.oxygen01, DEATH_OXYGEN);
    assert_eq!(w.forced_oxygen, Some(DEATH_OXYGEN));
    // El consumo de varios segundos no mata mientras dura la secuencia.
    for _ in 0..50 {
        let input = m.before_frame(&mut w, ShipInput::default(), AutoPhase::Pursuit);
        w.update(&input, 0.1);
        m.after_frame(&mut w, AutoPhase::Pursuit, 0.1);
        assert!(!w.gameplay.game_over);
    }
}

#[test]
fn el_monstruo_jala_la_nave_hasta_su_alcance_desde_cualquier_rumbo() {
    for forward in [-Vec3::Z, Vec3::Z, Vec3::X, Vec3::new(0.3, 0.8, 0.2)] {
        let mut w = world();
        w.ship = Ship::new(Vec3::new(400.0, 250.0, 1600.0), forward);
        let target = monster_center(&w);
        let start = (w.ship.position - target).length();
        for _ in 0..600 {
            let input = pull_input(&w, target);
            w.update(&input, 0.05);
        }
        let end = (w.ship.position - target).length();
        assert!(
            end < start * 0.5 && end < PULL_STOP_DISTANCE + 150.0,
            "rumbo {forward:?}: {start:.0} -> {end:.0}"
        );
    }
}

#[test]
fn con_la_nave_retenida_el_monstruo_mata() {
    let mut w = world();
    let mut m = MonsterDeath::new();
    w.gameplay.oxygen01 = DEATH_OXYGEN;
    m.before_frame(&mut w, ShipInput::default(), AutoPhase::Pursuit);
    // Mientras atrapa y arrastra, el jugador no controla la nave.
    let locked = m.before_frame(
        &mut w,
        ShipInput {
            thrust: 1.0,
            ..ShipInput::default()
        },
        AutoPhase::Dragging,
    );
    assert_eq!(locked.thrust, 0.0);
    let mut killed_at = None;
    for frame in 0..40 {
        if m.after_frame(&mut w, AutoPhase::Held, 0.1) {
            killed_at = Some(frame);
            break;
        }
    }
    let frame = killed_at.expect("debe matar con la nave retenida");
    assert!(((frame + 1) as f64 * 0.1 - HOLD_BEFORE_DEATH).abs() < 0.11);
    assert!(w.gameplay.game_over && w.gameplay.oxygen01 == 0.0 && !w.dying);
}

#[test]
fn si_la_captura_no_llega_la_muerte_llega_igual() {
    let mut w = world();
    let mut m = MonsterDeath::new();
    w.gameplay.oxygen01 = 0.0;
    let mut seconds = 0.0;
    while !m.after_frame(&mut w, AutoPhase::Pursuit, 0.25) {
        seconds += 0.25;
        assert!(seconds <= FALLBACK_DEATH_SECONDS + 0.5);
    }
    assert!(w.gameplay.game_over);
}

#[test]
fn la_partida_nueva_reinicia_la_secuencia() {
    let mut w = world();
    let mut m = MonsterDeath::new();
    w.gameplay.oxygen01 = 0.0;
    m.after_frame(&mut w, AutoPhase::Pursuit, 0.1);
    assert!(m.active());
    m.reset();
    w.restart_game();
    assert!(!m.active() && !w.dying);
    assert_eq!(w.gameplay.oxygen01, 1.0);
}
