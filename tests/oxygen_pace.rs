//! Ritmo de consumo de oxigeno configurable (`--oxygen-seconds`, tecla `L`) y verificacion de
//! extremo a extremo de que las estrellas observadoras (`--watching-stars`) abren ojos cuando el
//! oxigeno REAL baja solo, sin teclas de depuracion.

use umbral::{
    app::{
        OXYGEN_PACE_SECONDS, advance_perception_frame, cycle_oxygen_pace, oxygen_tank_seconds,
        set_oxygen_tank_seconds,
    },
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        perception_game::{OxygenKeys, OxygenTransition},
        watching_stars::OBSERVER_ONSET_OXYGEN,
        watching_stars_game::apply_watching_stars,
    },
};

fn world() -> World {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    // Como `main` sin `--debug-oxygen`: oxigeno real.
    world.clear_debug_oxygen();
    world
}

#[test]
fn la_tecla_l_alterna_los_ritmos_y_vuelve_al_normal() {
    let mut w = world();
    assert!(
        (oxygen_tank_seconds(&w) - 420.0).abs() < 1e-9,
        "por defecto 420 s"
    );
    let mut seen = Vec::new();
    for _ in 0..OXYGEN_PACE_SECONDS.len() {
        seen.push(cycle_oxygen_pace(&mut w));
    }
    assert_eq!(seen, [120.0, 60.0, 30.0, 420.0]);
    // Desde un valor fuera de la lista vuelve al normal.
    set_oxygen_tank_seconds(&mut w, 45.0).unwrap();
    assert_eq!(cycle_oxygen_pace(&mut w), 420.0);
}

#[test]
fn la_duracion_se_valida_y_sobrevive_a_la_partida_nueva() {
    let mut w = world();
    for bad in [0.0, -5.0, f64::NAN, f64::INFINITY] {
        assert!(set_oxygen_tank_seconds(&mut w, bad).is_err(), "{bad}");
    }
    assert!(
        (oxygen_tank_seconds(&w) - 420.0).abs() < 1e-9,
        "un error no cambia nada"
    );
    set_oxygen_tank_seconds(&mut w, 30.0).unwrap();
    w.restart_game();
    assert!(
        (oxygen_tank_seconds(&w) - 30.0).abs() < 1e-9,
        "Shift+R conserva el ritmo"
    );
}

#[test]
fn con_tanque_de_30_s_se_muere_a_los_30_s() {
    let mut w = world();
    set_oxygen_tank_seconds(&mut w, 30.0).unwrap();
    let mut seconds: f64 = 0.0;
    while !w.gameplay.game_over && seconds < 60.0 {
        w.update(&ShipInput::default(), 0.1);
        seconds += 0.1;
    }
    assert!(w.gameplay.game_over);
    assert!((seconds - 30.0).abs() < 0.11, "murio a los {seconds:.2} s");
}

#[test]
fn las_estrellas_abren_ojos_cuando_el_oxigeno_real_baja_solo() {
    let mut w = world();
    set_oxygen_tank_seconds(&mut w, 30.0).unwrap();
    // Misma preparacion que el bucle con `--watching-stars` (implica la percepcion).
    let mut oxygen = OxygenTransition::new(w.debug_oxygen01).unwrap();
    let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
    w.render(&mut fb);
    let start = apply_watching_stars(&mut fb, &w).unwrap();
    assert!(
        start.active_ids.is_empty(),
        "con O2 lleno no hay observadoras"
    );

    // Sin teclas: solo pasa el tiempo (cuadros de 0.1 s reales).
    let mut seconds: f64 = 0.0;
    let mut first_eyes = None;
    while seconds < 25.0 {
        advance_perception_frame(
            &mut w,
            &mut oxygen,
            OxygenKeys::default(),
            &ShipInput::default(),
            0.1,
        )
        .unwrap();
        seconds += 0.1;
        w.render(&mut fb);
        let report = apply_watching_stars(&mut fb, &w).unwrap();
        if first_eyes.is_none() && !report.eyes.is_empty() {
            first_eyes = Some((seconds, w.debug_oxygen01, report.eyes.len()));
        }
    }
    let (at, shown, eyes) = first_eyes.expect("deben aparecer ojos al bajar el oxigeno real");
    assert!(
        shown < OBSERVER_ONSET_OXYGEN,
        "aparecen bajo el umbral: {shown}"
    );
    // El O2 mostrado sigue al real: a 30 s por tanque cruza 65 % hacia los 10.5 s.
    assert!(at > 9.0 && at < 14.0, "primer ojo a los {at:.1} s");
    assert!(eyes > 0);
    let late = apply_watching_stars(&mut fb, &w).unwrap();
    assert!(
        late.active_ids.len() > 50,
        "con O2 {:.2} hay muchas observadoras: {}",
        w.debug_oxygen01,
        late.active_ids.len()
    );
    assert!(!w.gameplay.game_over);
}
