//! UMB-310: progresividad del O2 manual con `--lucid-perception`. Las teclas cambian el
//! OBJETIVO; el O2 mostrado lo persigue en linea recta con segundos reales, sin saltos, sin
//! pasarse y llegando exactamente. Nada consume O2 solo.

use umbral::visual::perception_game::{OXYGEN_TRANSITION_RATE, OxygenKeys, OxygenTransition};

const FRAME: f64 = 1.0 / 60.0;

fn preset(value: f64) -> OxygenKeys {
    OxygenKeys {
        preset: Some(value),
        ..OxygenKeys::default()
    }
}

fn increase() -> OxygenKeys {
    OxygenKeys {
        increase: true,
        ..OxygenKeys::default()
    }
}

fn decrease() -> OxygenKeys {
    OxygenKeys {
        decrease: true,
        ..OxygenKeys::default()
    }
}

fn transition(oxygen: f64) -> OxygenTransition {
    OxygenTransition::new(oxygen).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn it_starts_exactly_at_the_requested_oxygen() {
    for oxygen in [1.0, 0.5, 0.37, 0.25, 0.05, 0.01, 0.0] {
        let o = transition(oxygen);
        assert_eq!((o.target(), o.shown()), (oxygen, oxygen));
    }
}

#[test]
fn a_preset_moves_the_shown_oxygen_linearly_both_down_and_up_without_jumping() {
    let mut o = transition(1.0);
    o.press(preset(0.05)).unwrap();
    assert_eq!(
        (o.target(), o.shown()),
        (0.05, 1.0),
        "la tecla cambia solo el objetivo"
    );
    let shown = o.advance(0.1).unwrap();
    assert!(close(shown, 1.0 - 0.1 * OXYGEN_TRANSITION_RATE), "{shown}");
    assert_eq!(o.shown(), shown);
    // La recuperacion va a la misma velocidad.
    let mut o = transition(0.05);
    o.press(preset(1.0)).unwrap();
    let shown = o.advance(0.1).unwrap();
    assert!(close(shown, 0.05 + 0.1 * OXYGEN_TRANSITION_RATE), "{shown}");
}

#[test]
fn at_sixty_frames_per_second_it_never_jumps_or_overshoots_and_lands_exactly() {
    let max_step = OXYGEN_TRANSITION_RATE * FRAME;
    for (from, to) in [
        (1.0, 0.05),
        (0.05, 1.0),
        (0.25, 0.0),
        (0.0, 1.0),
        (0.37, 0.5),
    ] {
        let mut o = transition(from);
        o.press(preset(to)).unwrap();
        let (low, high) = (f64::min(from, to), f64::max(from, to));
        let (mut previous, mut frames) = (from, 0usize);
        while o.shown() != to {
            let shown = o.advance(FRAME).unwrap();
            frames += 1;
            assert!(
                (shown - previous).abs() <= max_step + 1e-15,
                "{from}->{to}: salto de {previous} a {shown}"
            );
            assert!(
                (low..=high).contains(&shown),
                "{from}->{to}: se pasa: {shown}"
            );
            assert!(
                (shown - previous) * (to - from) >= 0.0,
                "{from}->{to}: retrocede"
            );
            assert!(frames <= 1000, "{from}->{to}: no llega");
            previous = shown;
        }
        let expected = ((to - from).abs() / max_step).ceil();
        assert!(
            (frames as f64 - expected).abs() <= 1.0,
            "{from}->{to}: {frames} cuadros, esperados {expected}"
        );
        assert_eq!(o.shown(), to, "llega exactamente");
    }
}

#[test]
fn it_reaches_exactly_zero_in_finite_time_and_only_then_is_zero() {
    let mut o = transition(0.05);
    o.press(decrease()).unwrap();
    assert_eq!((o.target(), o.shown()), (0.0, 0.05));
    let (mut elapsed, mut previous) = (0.0, 0.05);
    while o.shown() != 0.0 {
        let shown = o.advance(FRAME).unwrap();
        elapsed += FRAME;
        assert!(previous - shown <= OXYGEN_TRANSITION_RATE * FRAME + 1e-15);
        assert!(shown >= 0.0);
        assert!(elapsed < 1.0, "debe llegar a 0 en tiempo finito");
        previous = shown;
    }
    assert!(
        elapsed >= 0.05 / OXYGEN_TRANSITION_RATE - 1e-9,
        "no antes de tiempo: {elapsed}"
    );
    assert!(elapsed <= 0.05 / OXYGEN_TRANSITION_RATE + FRAME + 1e-9);
    assert_eq!(o.shown().to_bits(), 0.0f64.to_bits(), "cero exacto (+0)");
}

#[test]
fn a_long_real_dt_is_applied_whole_but_never_past_the_target() {
    let mut o = transition(1.0);
    o.press(preset(0.05)).unwrap();
    // Dos segundos reales completos (no recortados al limite de la fisica).
    assert!(close(o.advance(2.0).unwrap(), 0.5));
    assert_eq!(o.advance(3600.0).unwrap(), 0.05);
    o.press(preset(1.0)).unwrap();
    assert_eq!(o.advance(f64::MAX).unwrap(), 1.0);
}

#[test]
fn zero_dt_changes_nothing_and_invalid_inputs_leave_the_state_intact() {
    let mut o = transition(1.0);
    o.press(preset(0.05)).unwrap();
    let before = o;
    assert_eq!(o.advance(0.0).unwrap(), 1.0);
    assert_eq!(o, before);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1] {
        assert!(o.advance(bad).is_err(), "dt {bad}");
        assert_eq!(o, before, "dt {bad}");
    }
    for bad in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(OxygenTransition::new(bad).is_err(), "inicio {bad}");
        let keys = OxygenKeys {
            preset: Some(bad),
            increase: true,
            decrease: false,
        };
        assert!(o.press(keys).is_err(), "preset {bad}");
        assert_eq!(o, before, "preset {bad}");
    }
}

#[test]
fn presets_and_m_n_during_a_transition_only_retarget_it_from_the_shown_value() {
    let mut o = transition(1.0);
    o.press(preset(0.05)).unwrap();
    assert!(close(o.advance(1.0).unwrap(), 0.75));
    // Otro preset a mitad: cambia el objetivo, el mostrado sigue desde donde estaba.
    o.press(preset(0.5)).unwrap();
    assert!(close(o.shown(), 0.75) && o.target() == 0.5);
    assert!(close(o.advance(0.5).unwrap(), 0.625));
    // Reversion: vuelve a subir desde el valor mostrado, sin salto.
    o.press(preset(1.0)).unwrap();
    assert!(close(o.shown(), 0.625));
    assert!(close(o.advance(0.5).unwrap(), 0.75));
    // M/N mueven el OBJETIVO en pasos de 5 % (la regla de siempre), no el mostrado.
    o.press(decrease()).unwrap();
    assert!(close(o.target(), 0.95) && close(o.shown(), 0.75));
    o.press(increase()).unwrap();
    o.press(increase()).unwrap();
    assert_eq!(o.target(), 1.0, "acotado a 100 %");
    // Preset y M en el mismo cuadro: primero el preset, despues M (como antes).
    let keys = OxygenKeys {
        preset: Some(0.25),
        increase: true,
        decrease: false,
    };
    o.press(keys).unwrap();
    assert!(close(o.target(), 0.3) && close(o.shown(), 0.75));
    // Sin mas teclas la transicion sigue sola hasta el objetivo.
    for _ in 0..200 {
        o.advance(FRAME).unwrap();
    }
    assert_eq!(o.shown(), o.target());
}

#[test]
fn nothing_consumes_oxygen_on_its_own() {
    for start in [1.0, 0.5, 0.37, 0.05, 0.0] {
        let mut o = transition(start);
        for _ in 0..10_000 {
            o.press(OxygenKeys::default()).unwrap();
            o.advance(FRAME).unwrap();
        }
        assert_eq!((o.target(), o.shown()), (start, start));
    }
}

use umbral::{
    app::advance_perception_frame,
    assets::{obj::load_model, ship_model_path},
    scene::{ship::ShipInput, world::World},
};

/// Mundo real con el arranque `--near 3` y el O2 dado, y su transicion.
fn game(oxygen: f64) -> (World, OxygenTransition) {
    let model = load_model(&ship_model_path()).unwrap();
    let mut world = World::new(model, 320.0 / 180.0).unwrap();
    world.time = 1.3;
    world.debug_oxygen01 = oxygen;
    world.place_near(3, 25.0).unwrap();
    (world, transition(oxygen))
}

#[test]
fn each_frame_the_world_receives_the_shown_oxygen_after_whole_real_seconds() {
    let (mut world, mut o) = game(1.0);
    let still = ShipInput::default();
    advance_perception_frame(&mut world, &mut o, preset(0.05), &still, 0.5).unwrap();
    assert_eq!(o.target(), 0.05);
    assert!(
        close(o.shown(), 0.875),
        "0.5 s reales completos: {}",
        o.shown()
    );
    assert_eq!(
        world.debug_oxygen01,
        o.shown(),
        "el mundo ve el O2 mostrado"
    );
    for _ in 0..240 {
        advance_perception_frame(&mut world, &mut o, OxygenKeys::default(), &still, FRAME).unwrap();
        assert_eq!(world.debug_oxygen01, o.shown());
    }
    assert_eq!(
        world.debug_oxygen01, 0.05,
        "sigue sin mas teclas hasta el objetivo"
    );
}

#[test]
fn f5_freezes_the_orbital_clock_but_not_the_oxygen_transition() {
    let (mut world, mut o) = game(1.0);
    world.toggles.freeze_orbits = true;
    let time = world.time;
    let still = ShipInput::default();
    advance_perception_frame(&mut world, &mut o, preset(0.05), &still, FRAME).unwrap();
    for _ in 0..59 {
        advance_perception_frame(&mut world, &mut o, OxygenKeys::default(), &still, FRAME).unwrap();
    }
    assert_eq!(world.time, time, "F5 congela el reloj orbital");
    assert!(
        close(world.debug_oxygen01, 0.75),
        "{}",
        world.debug_oxygen01
    );
}

#[test]
fn an_invalid_frame_leaves_the_world_and_the_transition_intact() {
    let (mut world, mut o) = game(1.0);
    let still = ShipInput::default();
    advance_perception_frame(&mut world, &mut o, preset(0.25), &still, 1.0).unwrap();
    let (before, oxygen, time) = (o, world.debug_oxygen01, world.time);
    let position = world.ship.position;
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        let result = advance_perception_frame(&mut world, &mut o, preset(0.05), &still, bad);
        assert!(result.is_err(), "dt {bad}");
        assert_eq!(o, before, "dt {bad}: ni el objetivo cambia");
        assert_eq!(
            (world.debug_oxygen01, world.time, world.ship.position),
            (oxygen, time, position),
            "dt {bad}"
        );
    }
    let bad_preset = preset(f64::NAN);
    assert!(advance_perception_frame(&mut world, &mut o, bad_preset, &still, FRAME).is_err());
    assert_eq!((o, world.time), (before, time));
    // dt 0: valido, sin cambios en el O2.
    advance_perception_frame(&mut world, &mut o, OxygenKeys::default(), &still, 0.0).unwrap();
    assert_eq!((o, world.debug_oxygen01), (before, oxygen));
}

#[test]
fn the_frame_loop_never_consumes_oxygen_and_r_keeps_target_and_shown() {
    let (mut world, mut o) = game(0.37);
    let still = ShipInput::default();
    for _ in 0..600 {
        advance_perception_frame(&mut world, &mut o, OxygenKeys::default(), &still, FRAME).unwrap();
    }
    assert_eq!(
        (o.target(), o.shown(), world.debug_oxygen01),
        (0.37, 0.37, 0.37)
    );
    // R a mitad de una transicion (juego normal: `reset_ship`) no toca el O2.
    advance_perception_frame(&mut world, &mut o, preset(1.0), &still, 0.4).unwrap();
    let before = (o, world.debug_oxygen01);
    world.reset_ship();
    assert_eq!((o, world.debug_oxygen01), before);
    advance_perception_frame(&mut world, &mut o, OxygenKeys::default(), &still, 0.4).unwrap();
    assert!(close(o.shown(), 0.57), "{}", o.shown());
}
