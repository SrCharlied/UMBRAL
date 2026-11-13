//! HUD de oxigeno en el compositor COMPARTIDO (`compose_presentation_with_hud`): el mismo para headless,
//! ventana y `P`. Cubre el codigo compartido; no certifica la interaccion fisica con la ventana.

use umbral::{
    app::{
        PresentationLayers, compose_presentation, compose_presentation_with_hud,
        draw_gameplay_feedback,
    },
    assets::{obj::load_model, ship_model_path},
    debug::overlay::draw_overlay,
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::{
        creature_distant_turn::CreaturePreset,
        oxygen_hud::{OxygenReading, draw_oxygen_hud, oxygen_hud_layout},
        perception_game::apply_lucid_perception,
    },
};

fn world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap()
}

fn scene(w: &World) -> Framebuffer {
    let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
    w.render(&mut fb);
    fb
}

fn layers<'a>(
    overlay: Option<&'a [String]>,
    feedback: bool,
    perception: bool,
) -> PresentationLayers<'a> {
    PresentationLayers {
        overlay,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback: feedback,
        ending: None,
        perception,
    }
}

/// Estado que el HUD jamas debe tocar (bits exactos).
fn state(w: &World) -> Vec<u64> {
    let g = &w.gameplay;
    vec![
        g.oxygen01.to_bits(),
        w.debug_oxygen01.to_bits(),
        w.oxygen01().to_bits(),
        w.forced_oxygen.map_or(u64::MAX, f64::to_bits),
        w.time.to_bits(),
        w.ship.position.x.to_bits(),
        w.ship.position.y.to_bits(),
        w.ship.position.z.to_bits(),
        u64::from(g.game_over),
        u64::from(g.objective_complete),
        g.collected_resources as u64,
        g.collected_logs as u64,
    ]
}

#[test]
fn hudapp01_off_is_exactly_the_existing_composer() {
    let w = world();
    let lines = vec!["OVERLAY DE PRUEBA".to_string()];
    for (overlay, feedback) in [(None, false), (Some(lines.as_slice()), true)] {
        let mut old = scene(&w);
        compose_presentation(&mut old, &w, &layers(overlay, feedback, false)).unwrap();
        let mut off = scene(&w);
        compose_presentation_with_hud(&mut off, &w, &layers(overlay, feedback, false), false)
            .unwrap();
        assert!(off.pixels == old.pixels, "HUD apagado cambio el cuadro");
    }
}

#[test]
fn hudapp02_on_draws_the_real_reserve_only_in_its_region_without_touching_state() {
    let mut w = world();
    w.gameplay.oxygen01 = 0.72;
    w.clear_debug_oxygen();
    let base = scene(&w).pixels;
    let before = state(&w);
    let mut on = scene(&w);
    compose_presentation_with_hud(&mut on, &w, &layers(None, false, false), true).unwrap();
    assert_eq!(state(&w), before, "el HUD toco el estado");
    let mut expected = scene(&w);
    draw_oxygen_hud(&mut expected, OxygenReading::new(0.72).unwrap());
    assert!(
        on.pixels == expected.pixels,
        "no es el instrumento con la reserva real"
    );
    assert!(on.pixels != base, "precondicion: el HUD cambia el cuadro");
    let l = oxygen_hud_layout(320, 180).unwrap();
    for (i, (a, b)) in on.pixels.iter().zip(&base).enumerate() {
        if !l.bounds.contains(i % 320, i / 320) {
            assert_eq!(
                a,
                b,
                "pixel fuera de la region del HUD ({}, {})",
                i % 320,
                i / 320
            );
        }
    }
}

#[test]
fn hudapp03_reserve_is_the_authority_not_the_visual_debug_oxygen() {
    let mut w = world();
    w.gameplay.oxygen01 = 0.4;
    // 1-4/M/N y --debug-oxygen: solo el O2 VISUAL. La reserva sigue en 0.4.
    w.set_debug_oxygen(0.9);
    assert_eq!(w.oxygen01(), 0.9);
    let mut on = scene(&w);
    compose_presentation_with_hud(&mut on, &w, &layers(None, false, false), true).unwrap();
    let draw = |v: f64| {
        let mut fb = scene(&w);
        draw_oxygen_hud(&mut fb, OxygenReading::new(v).unwrap());
        fb.pixels
    };
    assert!(on.pixels == draw(0.4), "el HUD no muestra la reserva real");
    assert!(on.pixels != draw(0.9), "el HUD mintio con el O2 visual");
}

#[test]
fn hudapp04_invalid_reserve_is_rejected_before_any_layer_is_drawn() {
    let lines = vec!["OVERLAY DE PRUEBA".to_string()];
    for bad in [f64::NAN, f64::INFINITY, -0.5, 1.5] {
        let mut w = world();
        w.gameplay.oxygen01 = bad;
        let mut fb = scene(&w);
        let before = fb.pixels.clone();
        let result =
            compose_presentation_with_hud(&mut fb, &w, &layers(Some(&lines), true, false), true);
        assert!(result.is_err(), "acepto la reserva {bad}");
        assert!(
            fb.pixels == before,
            "con la reserva {bad} se dibujo algo antes del Err"
        );
    }
}

#[test]
fn hudapp05_hud_after_perception_so_it_stays_legible() {
    // Decision de Charlie (hud_oxygen_02): el HUD va DESPUES de la percepcion para seguir legible con O2
    // bajo; las capas de depuracion siguen antes de la percepcion como siempre.
    let mut w = world();
    w.gameplay.oxygen01 = 0.6;
    w.set_debug_oxygen(0.25);
    let lines = vec!["OVERLAY DE PRUEBA".to_string()];
    let mut on = scene(&w);
    compose_presentation_with_hud(&mut on, &w, &layers(Some(&lines), true, true), true).unwrap();
    let mut expected = scene(&w);
    draw_overlay(&mut expected, &lines);
    draw_gameplay_feedback(&mut expected, &w);
    apply_lucid_perception(&mut expected, &w).unwrap();
    draw_oxygen_hud(&mut expected, OxygenReading::new(0.6).unwrap());
    assert!(
        on.pixels == expected.pixels,
        "orden: capas de depuracion, percepcion, HUD"
    );
}

#[test]
fn hudapp06_hud_stays_readable_over_total_black() {
    let mut w = world();
    // Reserva real positiva, O2 visual en 0: la percepcion apaga el mundo y el HUD sigue leyendose encima.
    w.gameplay.oxygen01 = 0.6;
    w.set_debug_oxygen(0.0);
    let mut last = scene(&w);
    compose_presentation_with_hud(&mut last, &w, &layers(None, true, true), true).unwrap();
    let mut expected = Framebuffer::new(320, 180, 0).unwrap();
    draw_oxygen_hud(&mut expected, OxygenReading::new(0.6).unwrap());
    assert!(
        last.pixels == expected.pixels,
        "con O2 visual 0: negro total salvo el HUD con la reserva real"
    );
    let l = oxygen_hud_layout(320, 180).unwrap();
    assert!(
        (0..320 * 180)
            .filter(|i| !l.bounds.contains(i % 320, i / 320))
            .all(|i| last.pixels[i] & 0x00ff_ffff == 0),
        "fuera del HUD el negro total sigue intacto"
    );
}
