//! HUD de oxigeno DENTRO del juego (hud_oxygen_02, aprobado por Charlie): encendido por defecto en la experiencia
//! completa, sigue la reserva REAL mientras baja (consumo del tanque y teclas que en la experiencia por defecto
//! arrastran el O2 real) y queda legible sobre la percepcion, bajo los marcadores y bajo la pantalla final.

use umbral::{
    app::{
        PresentationLayers, advance_perception_frame, compose_presentation_with_hud,
        set_oxygen_tank_seconds,
    },
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_distant_turn::CreaturePreset,
        ending::{EndingSummary, FULLY_SHOWN, draw_ending},
        markers::draw_markers,
        oxygen_hud::{OxygenHudOptions, OxygenReading, draw_oxygen_hud, oxygen_hud_for_game},
        perception_game::{OxygenKeys, OxygenTransition, apply_lucid_perception},
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

fn layers(ending: Option<f64>) -> PresentationLayers<'static> {
    PresentationLayers {
        overlay: None,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback: false,
        ending,
        perception: true,
    }
}

/// Cuadro compuesto del juego con el HUD.
fn frame(w: &World, ending: Option<f64>) -> Vec<u32> {
    let mut fb = scene(w);
    compose_presentation_with_hud(&mut fb, w, &layers(ending), true).unwrap();
    fb.pixels
}

/// Lo esperado: escena, percepcion, HUD con la reserva real, marcadores (con sectores) y final.
fn expected(w: &World, ending: Option<f64>) -> Vec<u32> {
    let mut fb = scene(w);
    apply_lucid_perception(&mut fb, w).unwrap();
    draw_oxygen_hud(&mut fb, OxygenReading::real_reserve(w).unwrap());
    if w.sectors.is_some() {
        draw_markers(&mut fb, w);
    }
    if let Some(seconds) = ending
        && let Some(summary) = EndingSummary::from_gameplay(&w.gameplay, w.time)
    {
        draw_ending(&mut fb, &summary, seconds);
    }
    fb.pixels
}

#[test]
fn hudgame01_on_by_default_in_the_full_experience_and_opt_in_elsewhere() {
    let none = OxygenHudOptions::default();
    let explicit = OxygenHudOptions {
        enabled: true,
        initial_reserve: Some(0.3),
    };
    assert!(
        oxygen_hud_for_game(none, true).enabled,
        "experiencia completa: encendido"
    );
    assert!(
        !oxygen_hud_for_game(none, false).enabled,
        "--classic/--headless: apagado"
    );
    assert_eq!(
        oxygen_hud_for_game(explicit, false),
        explicit,
        "--oxygen-hud explicito"
    );
    assert_eq!(oxygen_hud_for_game(explicit, true), explicit);
    let reserve_only = OxygenHudOptions {
        enabled: false,
        initial_reserve: Some(0.3),
    };
    assert_eq!(
        oxygen_hud_for_game(reserve_only, true),
        explicit,
        "la reserva inicial vale con el HUD por defecto"
    );
}

#[test]
fn hudgame02_hud_goes_down_with_the_real_reserve_while_playing() {
    // Experiencia por defecto: tanque de 120 s y teclas de O2 que arrastran el O2 REAL.
    let mut w = world();
    set_oxygen_tank_seconds(&mut w, 120.0).unwrap();
    w.oxygen_keys_drive_real = true;
    let mut o2 = OxygenTransition::new(1.0).unwrap();
    let step = |w: &mut World, o2: &mut OxygenTransition, keys: OxygenKeys, seconds: f64| {
        for _ in 0..(seconds * 20.0) as usize {
            advance_perception_frame(w, o2, keys, &ShipInput::default(), 0.05).unwrap();
        }
    };
    let mut readings = vec![OxygenReading::real_reserve(&w).unwrap().percent()];
    let mut frames = vec![frame(&w, None)];
    assert!(frames[0] == expected(&w, None));
    // 1) Consumo del tanque: 30 s de 120 s.
    step(&mut w, &mut o2, OxygenKeys::default(), 30.0);
    readings.push(OxygenReading::real_reserve(&w).unwrap().percent());
    frames.push(frame(&w, None));
    assert!(frames[1] == expected(&w, None));
    // 2) Tecla 3 (25 %): el O2 mostrado baja a 0.25/s y el real lo sigue.
    step(
        &mut w,
        &mut o2,
        OxygenKeys {
            preset: Some(0.25),
            ..OxygenKeys::default()
        },
        0.05,
    );
    step(&mut w, &mut o2, OxygenKeys::default(), 3.0);
    readings.push(OxygenReading::real_reserve(&w).unwrap().percent());
    frames.push(frame(&w, None));
    assert!(frames[2] == expected(&w, None));
    assert_eq!(readings[0], 100);
    assert!((74..=76).contains(&readings[1]), "{readings:?}");
    assert!((23..=26).contains(&readings[2]), "{readings:?}");
    assert!(
        frames[0] != frames[1] && frames[1] != frames[2],
        "el HUD no cambio al bajar"
    );
}

#[test]
fn hudgame03_markers_over_the_hud_and_the_ending_over_everything() {
    let mut w = world();
    w.enable_sectors();
    w.gameplay.oxygen01 = 0.4;
    w.clear_debug_oxygen();
    assert!(
        frame(&w, None) == expected(&w, None),
        "percepcion, HUD, marcadores"
    );
    w.gameplay.oxygen01 = 0.0;
    w.gameplay.game_over = true;
    w.clear_debug_oxygen();
    let end = frame(&w, Some(FULLY_SHOWN));
    assert!(
        end == expected(&w, Some(FULLY_SHOWN)),
        "el final va encima del HUD"
    );
}
