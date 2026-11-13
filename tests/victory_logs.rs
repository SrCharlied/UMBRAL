//! Victoria al leer los tres registros (victory_01, pedido de Charlie): en la experiencia por defecto la partida se
//! gana en cuanto se recoge el tercer registro, sin ir al Sol, con el final «EL SECRETO». El juego clasico conserva
//! la regla de siempre (tres registros y llegar al Sol).

use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::ending::{EndingKind, EndingSummary, FULLY_SHOWN, draw_ending, ending_text},
};

fn world(win_on_logs: bool) -> World {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.gameplay.settings.win_on_logs = win_on_logs;
    w
}

/// Recoge el registro `i` en su posicion actual (lejos del Sol).
fn collect_log(w: &mut World, i: usize) {
    let position = w.gameplay.logs[i]
        .anchor
        .position(&w.system, w.time)
        .unwrap();
    let sun = w.system.bodies[0].position(w.time);
    assert!(
        (position - sun).length() > w.system.bodies[0].body_radius + 100.0,
        "el registro {i} esta lejos del Sol"
    );
    let system = w.system.clone();
    w.gameplay.collect_near(position, &system, w.time);
    assert!(w.gameplay.logs[i].anchor.collected);
}

#[test]
fn victory01_default_experience_wins_with_the_third_log() {
    let mut w = world(true);
    collect_log(&mut w, 0);
    collect_log(&mut w, 1);
    assert!(
        !w.gameplay.objective_complete,
        "con dos registros sigue la partida"
    );
    assert_eq!(EndingKind::of(&w.gameplay), None);
    collect_log(&mut w, 2);
    assert!(
        w.gameplay.objective_complete,
        "el tercer registro gana sin ir al Sol"
    );
    assert!(!w.gameplay.game_over);
    assert_eq!(EndingKind::of(&w.gameplay), Some(EndingKind::Secret));
}

#[test]
fn victory02_classic_keeps_the_sun_rule() {
    let mut w = world(false);
    for i in 0..3 {
        collect_log(&mut w, i);
    }
    assert!(
        !w.gameplay.objective_complete,
        "clasico: faltaba llegar al Sol"
    );
}

#[test]
fn victory03_secret_ending_text_and_drawing() {
    let summary = EndingSummary {
        kind: EndingKind::Secret,
        oxygen01: 0.42,
        resources: 2,
        resources_total: 3,
        logs: 3,
        logs_total: 3,
        mission_seconds: 95.0,
    };
    let (title, lines, stats) = ending_text(&summary);
    assert_eq!(title, "EL SECRETO");
    let body = lines.join(" ");
    assert!(body.contains("locura"), "{body}");
    assert!(body.contains("42%"), "{body}");
    assert!(stats.contains("REGISTROS 3/3"), "{stats}");
    // Solo caracteres que la fuente 5x7 sabe dibujar (sin tildes ni signos de apertura).
    let allowed = |c: char| c.is_ascii_alphanumeric() || " -.:/%()=+,|".contains(c);
    for text in std::iter::once(&title).chain(&lines) {
        assert!(
            text.chars().all(allowed),
            "caracter fuera de la fuente en: {text}"
        );
    }
    let mut fb = Framebuffer::new(1280, 720, SPACE_COLOR).unwrap();
    let before = fb.pixels.clone();
    draw_ending(&mut fb, &summary, FULLY_SHOWN);
    assert!(fb.pixels != before);
}
