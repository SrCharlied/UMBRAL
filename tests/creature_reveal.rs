//! Revelacion de la criatura por oxigeno: estado puro, compositor fantasma, integracion con
//! `World` y linea de comandos. Fixtures sinteticos y pequenos creados aqui; no necesita el
//! asset real ni abre ventanas.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use umbral::{
    assets::{
        obj::{Model, load_model, parse_obj},
        ship_model_path,
    },
    math::Vec3,
    renderer::{framebuffer::Framebuffer, shader::to_rgb8, stats::RenderStats, texture::Texture},
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_reveal::*,
        static_creature::{CreaturePlacement, DEMO_CENTER, DEMO_WORLD_RADIUS, StaticCreature},
    },
};

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// Cubo de lado `2 * half` centrado en `center`, con UV completos por cara y normales.
fn cube_source(center: Vec3, half: f64) -> String {
    let mut source = String::new();
    for (x, y, z) in [
        (-1.0, -1.0, -1.0),
        (1.0, -1.0, -1.0),
        (1.0, 1.0, -1.0),
        (-1.0, 1.0, -1.0),
        (-1.0, -1.0, 1.0),
        (1.0, -1.0, 1.0),
        (1.0, 1.0, 1.0),
        (-1.0, 1.0, 1.0),
    ] {
        source += &format!(
            "v {} {} {}\n",
            center.x + x * half,
            center.y + y * half,
            center.z + z * half
        );
    }
    source += "vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n";
    source += "vn 0 0 1\nvn 0 0 -1\nvn 1 0 0\nvn -1 0 0\nvn 0 1 0\nvn 0 -1 0\n";
    for (n, [a, b, c, d]) in [
        [5, 6, 7, 8],
        [2, 1, 4, 3],
        [6, 2, 3, 7],
        [1, 5, 8, 4],
        [8, 7, 3, 4],
        [1, 2, 6, 5],
    ]
    .into_iter()
    .enumerate()
    {
        let n = n + 1;
        source += &format!("f {a}/1/{n} {b}/2/{n} {c}/3/{n}\nf {a}/1/{n} {c}/3/{n} {d}/4/{n}\n");
    }
    source
}

fn rgb_texture() -> Texture {
    Texture::from_rgb8(
        2,
        2,
        vec![230, 40, 40, 40, 220, 60, 50, 80, 240, 240, 220, 40],
    )
    .unwrap()
}

const W: usize = 320;
const H: usize = 180;

fn new_world() -> World {
    World::new(load_model(&ship_model_path()).unwrap(), W as f64 / H as f64).unwrap()
}

fn cube_creature(center: Vec3, radius: f64) -> StaticCreature {
    let model = parse_obj(&cube_source(Vec3::new(5.0, -3.0, 2.0), 1.5)).unwrap();
    let placement = CreaturePlacement::new(center, radius, 0.0).unwrap();
    StaticCreature::new(model, rgb_texture(), placement).unwrap()
}

/// Mundo con la criatura de cubo en la ubicacion demo y la nave frente a ella.
fn near_creature_world() -> World {
    let mut world = new_world();
    world
        .set_creature(cube_creature(DEMO_CENTER, DEMO_WORLD_RADIUS))
        .unwrap();
    world.place_near_creature().unwrap();
    world
}

fn render(world: &World) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Un triangulo degenerado (area cero, no cubre pixeles): oculta la nave sin tocar su camara.
fn no_ship() -> Model {
    parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3\n").unwrap()
}

// --- Ajustes y objetivo ---

#[test]
fn default_settings_follow_the_proposal_and_invalid_settings_are_rejected() {
    let d = RevealSettings::default();
    assert_eq!(
        (
            d.start_oxygen(),
            d.full_oxygen(),
            d.rise_seconds(),
            d.fall_seconds()
        ),
        (0.50, 0.10, 2.0, 0.7)
    );
    assert_eq!(RevealSettings::new(0.50, 0.10, 2.0, 0.7), Ok(d));
    let nan = f64::NAN;
    let inf = f64::INFINITY;
    let invalid = [
        (nan, 0.1, 2.0, 0.7),
        (0.5, nan, 2.0, 0.7),
        (inf, 0.1, 2.0, 0.7),
        (1.01, 0.1, 2.0, 0.7),
        (0.5, -0.01, 2.0, 0.7),
        (0.1, 0.5, 2.0, 0.7),
        (0.3, 0.3, 2.0, 0.7),
        (0.5, 0.1, nan, 0.7),
        (0.5, 0.1, 2.0, inf),
        (0.5, 0.1, 0.0, 0.7),
        (0.5, 0.1, 2.0, -1.0),
        (0.5, 0.1, MIN_TRANSITION_SECONDS * 0.5, 0.7),
        (0.5, 0.1, 2.0, MAX_TRANSITION_SECONDS * 1.01),
    ];
    for (start, full, rise, fall) in invalid {
        assert!(
            RevealSettings::new(start, full, rise, fall).is_err(),
            "aceptado: {start} {full} {rise} {fall}"
        );
    }
    // Extremos validos.
    for (start, full, rise, fall) in [
        (1.0, 0.0, MIN_TRANSITION_SECONDS, MAX_TRANSITION_SECONDS),
        (0.2, 0.19, MAX_TRANSITION_SECONDS, MIN_TRANSITION_SECONDS),
    ] {
        assert!(RevealSettings::new(start, full, rise, fall).is_ok());
    }
}

#[test]
fn the_target_is_a_continuous_smoothstep_between_the_oxygen_thresholds() {
    let s = RevealSettings::default();
    // Teclas de depuracion: 1 y 2 ocultan, 3 da un estado intermedio, 4 revela entera.
    assert_eq!(reveal_target(1.0, &s), 0.0);
    assert_eq!(reveal_target(0.5, &s), 0.0);
    let key3 = reveal_target(0.25, &s);
    assert!(close(key3, 0.683_593_75, 1e-12), "tecla 3: {key3}");
    assert_eq!(reveal_target(0.05, &s), 1.0);
    assert_eq!(reveal_target(0.10, &s), 1.0);
    assert_eq!(reveal_target(0.0, &s), 1.0);
    // Punto medio de smoothstep.
    assert!(close(reveal_target(0.30, &s), 0.5, 1e-12));
    // Monotona (no creciente con el oxigeno) y dentro de [0, 1].
    let mut previous = 1.0;
    for i in 0..=1000 {
        let value = reveal_target(i as f64 / 1000.0, &s);
        assert!((0.0..=1.0).contains(&value));
        assert!(value <= previous + 1e-15, "no monotona en {i}");
        previous = value;
    }
    // Fuera de rango se recorta; no finito se trata como oculta (politica defensiva).
    assert_eq!(reveal_target(1.5, &s), 0.0);
    assert_eq!(reveal_target(-0.5, &s), 1.0);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(reveal_target(value, &s), 0.0, "{value}");
    }
}

// --- Mezcla en lineal ---

#[test]
fn srgb8_decoding_is_the_exact_inverse_of_to_rgb8_on_every_code() {
    for code in 0..=255u8 {
        let linear = srgb8_to_linear(code);
        assert!((0.0..=1.0).contains(&linear));
        let gray = Vec3::splat(linear);
        let expected = u32::from(code) * 0x010101;
        assert_eq!(to_rgb8(gray), expected, "codigo {code}");
    }
    assert_eq!(srgb8_to_linear(0), 0.0);
    assert_eq!(srgb8_to_linear(255), 1.0);
    // Curva sRGB estandar, no lineal: el gris 50 % lineal es ~188 en sRGB.
    assert!(close(srgb8_to_linear(188), 0.502_886, 1e-6));
    for code in 1..=255u8 {
        assert!(srgb8_to_linear(code) > srgb8_to_linear(code - 1));
    }
}

#[test]
fn blend_mixes_background_and_foreground_once_in_linear_light() {
    let (background, foreground) = (0x10_80_f0, 0xf0_20_40);
    // Extremos exactos.
    assert_eq!(blend_rgb8(background, foreground, 0.0), background);
    assert_eq!(blend_rgb8(background, foreground, 1.0), foreground);
    // Mezcla en lineal por canal, no en sRGB.
    let alpha = 0.3;
    let channel = |shift: u32| {
        let (b, f) = (
            srgb8_to_linear((background >> shift) as u8),
            srgb8_to_linear((foreground >> shift) as u8),
        );
        b + (f - b) * alpha
    };
    let expected = to_rgb8(Vec3::new(channel(16), channel(8), channel(0)));
    assert_eq!(blend_rgb8(background, foreground, alpha), expected);
    let naive = (0..3).fold(0, |acc, k| {
        let shift = 16 - 8 * k;
        let (b, f) = (
            ((background >> shift) & 0xff) as f64,
            ((foreground >> shift) & 0xff) as f64,
        );
        acc | (((b + (f - b) * alpha).round() as u32) << shift)
    });
    assert_ne!(expected, naive, "el fixture debe distinguir lineal de sRGB");
    // Alfa fuera de rango se recorta; no finito deja el fondo (politica defensiva).
    assert_eq!(blend_rgb8(background, foreground, 2.0), foreground);
    assert_eq!(blend_rgb8(background, foreground, -1.0), background);
    assert_eq!(blend_rgb8(background, foreground, f64::NAN), background);
    // Los bits altos que no son color se ignoran.
    assert_eq!(
        blend_rgb8(0xff00_0000 | background, foreground, 0.0),
        background
    );
}

// --- Paso temporal ---

#[test]
fn step_reaches_95_percent_in_the_configured_real_seconds_and_snaps_to_the_ends() {
    let s = RevealSettings::default();
    // Aparicion: 2 s al 95 %; desaparicion: 0.7 s al 95 %.
    assert!(close(step_reveal(0.0, 1.0, 2.0, &s), 0.95, 1e-12));
    assert!(close(step_reveal(1.0, 0.0, 0.7, &s), 0.05, 1e-12));
    // Hacia un objetivo intermedio se usa la velocidad del sentido del cambio.
    let target = reveal_target(0.25, &s);
    assert!(close(
        step_reveal(0.0, target, 2.0, &s),
        target * 0.95,
        1e-12
    ));
    assert!(close(
        step_reveal(1.0, target, 0.7, &s),
        target + (1.0 - target) * 0.05,
        1e-12
    ));
    // Dentro de REVEAL_SNAP_EPSILON del objetivo se fija exactamente en el.
    assert_eq!(
        step_reveal(1.0 - REVEAL_SNAP_EPSILON * 0.5, 1.0, 1e-6, &s),
        1.0
    );
    assert_eq!(step_reveal(REVEAL_SNAP_EPSILON * 0.5, 0.0, 1e-6, &s), 0.0);
    // Llega exactamente a los extremos en tiempo finito, cuadro a cuadro.
    for (from, to, seconds) in [(0.0, 1.0, 5.0), (1.0, 0.0, 2.0)] {
        let mut shown = from;
        let mut previous = from;
        for _ in 0..(seconds * 60.0) as usize {
            shown = step_reveal(shown, to, 1.0 / 60.0, &s);
            // Monotona hacia el objetivo y sin pasarse.
            assert!((shown - previous) * (to - from) >= 0.0);
            assert!((shown - to) * (from - to) >= 0.0);
            previous = shown;
        }
        assert_eq!(shown, to, "{from} -> {to} en {seconds} s");
    }
    // Fuera del epsilon no se fija: el ajuste final es pequeno (documentado 0.001).
    let outside = 1.0 - 1.5e-3;
    assert!(step_reveal(outside, 1.0, 1e-9, &s) < 1.0);
    assert!(step_reveal(1.0 - 1.5 * REVEAL_SNAP_EPSILON, 1.0, 1e-9, &s) < 1.0);
}

#[test]
fn step_composes_over_split_frames_and_is_not_capped_like_the_physics_dt() {
    let s = RevealSettings::default();
    for (from, to, total) in [(0.0, 1.0, 0.5), (1.0, 0.0, 0.3), (0.2, 0.7, 0.9)] {
        let once = step_reveal(from, to, total, &s);
        for n in [2, 7, 60] {
            let mut shown = from;
            for _ in 0..n {
                shown = step_reveal(shown, to, total / n as f64, &s);
            }
            assert!(
                close(shown, once, 1e-12),
                "{from}->{to} {total}s en {n}: {shown} vs {once}"
            );
        }
    }
    // Un cuadro largo (> 0.1 s) avanza todo su tiempo real: no se limita como `dt` fisico.
    let long = step_reveal(0.0, 1.0, 0.25, &s);
    assert!(long > step_reveal(0.0, 1.0, 0.1, &s));
    let mut split = 0.0;
    for _ in 0..25 {
        split = step_reveal(split, 1.0, 0.01, &s);
    }
    assert!(close(long, split, 1e-12));
}

#[test]
fn step_handles_non_finite_and_out_of_range_inputs_defensively() {
    let s = RevealSettings::default();
    // Sin tiempo valido no avanza.
    for elapsed in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY] {
        assert_eq!(step_reveal(0.3, 1.0, elapsed, &s), 0.3, "{elapsed}");
    }
    // Tiempo infinito: llega al objetivo.
    assert_eq!(step_reveal(0.3, 1.0, f64::INFINITY, &s), 1.0);
    // Valor mostrado no finito: salta al objetivo; fuera de rango: se recorta antes.
    assert_eq!(step_reveal(f64::NAN, 0.4, 0.1, &s), 0.4);
    assert_eq!(step_reveal(f64::INFINITY, 0.4, 0.1, &s), 0.4);
    assert_eq!(step_reveal(2.0, 1.0, 1e-9, &s), 1.0);
    assert_eq!(step_reveal(-1.0, 0.0, 1e-9, &s), 0.0);
    // Objetivo no finito: oculta; fuera de rango: se recorta.
    assert_eq!(step_reveal(0.0, f64::NAN, 0.1, &s), 0.0);
    assert_eq!(step_reveal(0.5, 3.0, f64::INFINITY, &s), 1.0);
    assert_eq!(step_reveal(0.5, -3.0, f64::INFINITY, &s), 0.0);
    // Cualquier combinacion termina finita y en [0, 1].
    let values = [
        f64::NAN,
        f64::INFINITY,
        -f64::INFINITY,
        -1.0,
        0.0,
        0.5,
        1.0,
        9.0,
    ];
    for &shown in &values {
        for &target in &values {
            for &elapsed in &values {
                let next = step_reveal(shown, target, elapsed, &s);
                assert!((0.0..=1.0).contains(&next), "{shown} {target} {elapsed}");
            }
        }
    }
}

// --- Integracion con World: extremos y estado ---

#[test]
fn without_reveal_mode_nothing_changes_and_hidden_matches_a_world_without_creature() {
    // Sin criatura ni modo: el mundo de siempre. Activar el modo sin criatura no cambia nada.
    let mut plain = new_world();
    let baseline = render(&plain);
    assert!(plain.creature_reveal().is_none());
    plain.enable_creature_reveal(RevealSettings::default());
    assert_eq!(render(&plain), baseline);

    // Con criatura y sin modo: exactamente la variante estatica.
    let mut world = near_creature_world();
    let static_frame = render(&world);
    assert!(world.creature_reveal().is_none());

    // Modo activo con O2 1.0: oculta (0) desde el primer cuadro, sin transicion previa.
    world.debug_oxygen01 = 1.0;
    world.enable_creature_reveal(RevealSettings::default());
    let reveal = world.creature_reveal().expect("modo activo");
    assert_eq!(reveal.shown(), 0.0);
    assert_eq!(reveal.settings(), RevealSettings::default());
    let hidden = render(&world);
    assert_ne!(hidden.0, static_frame.0, "oculta no debe verse");

    // Oculta: mismos pixeles y mismos contadores que sin criatura (misma camara, O2, tiempo).
    let creature = world.clear_creature().unwrap();
    assert_eq!(hidden, render(&world));
    world.set_creature(creature).unwrap();

    // Desactivar el modo devuelve la variante estatica.
    world.disable_creature_reveal();
    assert!(world.creature_reveal().is_none());
    assert_eq!(render(&world), static_frame);
}

#[test]
fn fully_revealed_draws_through_the_static_path_exactly() {
    let mut world = near_creature_world();
    world.debug_oxygen01 = 0.05;
    world.time = 12.5;
    let static_frame = render(&world);
    world.enable_creature_reveal(RevealSettings::default());
    assert_eq!(world.creature_reveal().unwrap().shown(), 1.0);
    assert_eq!(render(&world), static_frame, "pixeles y contadores");
    // Tambien con el descarte por frustum apagado.
    world.toggles.frustum_culling = false;
    let reveal = world.creature_reveal().unwrap();
    world.disable_creature_reveal();
    let static_no_culling = render(&world);
    world.enable_creature_reveal(reveal.settings());
    assert_eq!(render(&world), static_no_culling);
}

#[test]
fn transitions_use_real_elapsed_time_separately_from_orbits_and_physics() {
    let s = RevealSettings::default();
    let mut world = near_creature_world();
    world.debug_oxygen01 = 1.0;
    world.enable_creature_reveal(s);
    // Tecla 4: aparece de forma gradual con tiempo real, no de golpe.
    world.debug_oxygen01 = 0.05;
    assert_eq!(world.creature_reveal().unwrap().target(0.05), 1.0);
    assert_eq!(
        world.creature_reveal().unwrap().shown(),
        0.0,
        "sin paso visual no cambia"
    );
    // `World::update` (fisica, orbitas) no avanza el efecto.
    world.update(&ShipInput::default(), 0.1);
    assert_eq!(world.creature_reveal().unwrap().shown(), 0.0);
    // F5 congela orbitas, no el efecto; y el efecto no suma al tiempo orbital.
    world.toggles.freeze_orbits = true;
    let time = world.time;
    world.update_creature_reveal(1.0);
    assert!(close(
        world.creature_reveal().unwrap().shown(),
        step_reveal(0.0, 1.0, 1.0, &s),
        1e-15
    ));
    world.update_creature_reveal(1.0);
    assert!(close(world.creature_reveal().unwrap().shown(), 0.95, 1e-12));
    assert_eq!(world.time, time);
    // Tecla 3: baja al intermedio a la velocidad de ocultarse.
    world.debug_oxygen01 = 0.25;
    let target = reveal_target(0.25, &s);
    for _ in 0..600 {
        world.update_creature_reveal(1.0 / 60.0);
    }
    assert_eq!(world.creature_reveal().unwrap().shown(), target);
    // Tecla 1: se vuelve a ocultar gradualmente y termina exactamente en 0.
    world.debug_oxygen01 = 1.0;
    world.update_creature_reveal(0.35);
    let falling = world.creature_reveal().unwrap().shown();
    assert!(falling > 0.0 && falling < target, "{falling}");
    world.update_creature_reveal(5.0);
    assert_eq!(world.creature_reveal().unwrap().shown(), 0.0);
    // Paso visual sin modo activo: no hace nada.
    world.disable_creature_reveal();
    world.update_creature_reveal(1.0);
    assert!(world.creature_reveal().is_none());
}

#[test]
fn resets_and_creature_changes_keep_oxygen_and_snap_the_effect_to_its_target() {
    let s = RevealSettings::default();
    let mut world = near_creature_world();
    world.debug_oxygen01 = 1.0;
    world.enable_creature_reveal(s);
    world.debug_oxygen01 = 0.25;
    world.update_creature_reveal(0.2);
    let target = reveal_target(0.25, &s);
    let mid = world.creature_reveal().unwrap().shown();
    assert!(mid > 0.0 && mid < target);
    // R con --near-creature: vuelve la pose, conserva O2 y salta al objetivo.
    world.place_near_creature().unwrap();
    assert_eq!(world.debug_oxygen01, 0.25);
    assert_eq!(world.creature_reveal().unwrap().shown(), target);
    // R normal: igual.
    world.debug_oxygen01 = 0.05;
    world.reset_ship();
    assert_eq!(world.creature_reveal().unwrap().shown(), 1.0);
    // Arranque junto a un cuerpo: igual.
    world.debug_oxygen01 = 1.0;
    world.place_near(1, 25.0).unwrap();
    assert_eq!(world.creature_reveal().unwrap().shown(), 0.0);
    // Snap explicito.
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    assert_eq!(world.creature_reveal().unwrap().shown(), 1.0);
    // Quitar la criatura conserva el modo; ponerla de nuevo salta al objetivo vigente.
    world.debug_oxygen01 = 1.0;
    let creature = world.clear_creature().unwrap();
    assert!(world.creature_reveal().is_some());
    world.set_creature(creature).unwrap();
    assert_eq!(world.creature_reveal().unwrap().shown(), 0.0);
    // La malla, el transform y la textura no cambian con el efecto.
    let before = world.creature().unwrap().transform();
    let vertices = world.creature().unwrap().model().vertices.clone();
    world.debug_oxygen01 = 0.25;
    for _ in 0..30 {
        world.update_creature_reveal(1.0 / 60.0);
        let _ = render(&world);
    }
    assert_eq!(world.creature().unwrap().transform(), before);
    assert_eq!(world.creature().unwrap().model().vertices, vertices);
}

// --- Linea de comandos ---

#[test]
fn manual_oxygen_steps_use_five_percentage_points_and_cancel_opposite_keys() {
    assert_eq!(step_debug_oxygen(0.5, true, false), 0.55);
    assert_eq!(step_debug_oxygen(0.5, false, true), 0.45);
    assert_eq!(step_debug_oxygen(0.5, true, true), 0.5);
    assert_eq!(step_debug_oxygen(0.253, false, false), 0.253);
}

#[test]
fn manual_oxygen_meter_stays_bounded_and_uses_exact_integer_percentages() {
    assert_eq!(step_debug_oxygen(0.98, true, false), 1.0);
    assert_eq!(step_debug_oxygen(0.02, false, true), 0.0);
    assert_eq!(step_debug_oxygen(0.253, true, false), 0.30);
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(step_debug_oxygen(bad, false, false), 1.0);
    }
    let mut oxygen = 0.50;
    for _ in 0..5 {
        oxygen = step_debug_oxygen(oxygen, false, true);
    }
    assert_eq!(oxygen, 0.25);
    for _ in 0..30 {
        oxygen = step_debug_oxygen(oxygen, true, false);
        assert!((0.0..=1.0).contains(&oxygen));
    }
    assert_eq!(oxygen, 1.0);
    for _ in 0..30 {
        oxygen = step_debug_oxygen(oxygen, false, true);
    }
    assert_eq!(oxygen, 0.0);
}

#[test]
fn reveal_flags_and_debug_oxygen_are_validated_before_any_io() {
    for (text, value) in [("0", 0.0), ("1", 1.0), ("0.25", 0.25), ("0.05", 0.05)] {
        assert_eq!(parse_debug_oxygen(text), Ok(value));
    }
    for text in ["-0.1", "1.01", "NaN", "inf", "-inf", "abc", ""] {
        let error = parse_debug_oxygen(text).expect_err(text);
        assert!(error.contains("--debug-oxygen"), "{error}");
    }
    assert_eq!(validate_reveal_flags(false, false, false, false), Ok(()));
    assert_eq!(validate_reveal_flags(true, true, true, false), Ok(()));
    // --debug-oxygen sirve sin criatura (control de depuracion del juego normal).
    assert_eq!(validate_reveal_flags(false, false, true, false), Ok(()));
    // --legacy sin los flags nuevos no cambia.
    assert_eq!(validate_reveal_flags(false, false, false, true), Ok(()));
    for (reveal, creature, oxygen, legacy, needle) in [
        (true, false, false, false, "--creature-reveal necesita"),
        (true, false, false, true, "--legacy"),
        (false, false, true, true, "--legacy"),
        (true, true, true, true, "--legacy"),
    ] {
        let error = validate_reveal_flags(reveal, creature, oxygen, legacy)
            .expect_err(&format!("aceptado: {reveal} {creature} {oxygen} {legacy}"));
        assert!(error.contains(needle), "{needle:?} no esta en {error:?}");
    }
}

/// Carpeta temporal propia de cada prueba; se borra al final.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("umbral-reveal-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// OBJ (cubo descentrado) y PNG RGB8 2x2 sinteticos en `dir`.
fn write_assets(dir: &Path) -> (PathBuf, PathBuf) {
    let (model, texture) = (dir.join("criatura.obj"), dir.join("criatura.png"));
    std::fs::write(&model, cube_source(Vec3::new(5.0, -3.0, 2.0), 1.5)).unwrap();
    let file = std::fs::File::create(&texture).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[230, 40, 40, 40, 220, 60, 50, 80, 240, 240, 220, 40])
        .unwrap();
    writer.finish().unwrap();
    (model, texture)
}

/// Siempre con `--headless`: estas pruebas nunca abren ventana.
fn run_game(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_game_command_line_wires_the_reveal_and_rejects_bad_combinations() {
    let dir = temp_dir("cli");
    let (model, texture) = write_assets(&dir);
    let texture_bytes = std::fs::read(&texture).unwrap();
    let output = dir.join("salida.png");
    let os = OsStr::new;
    let outcome = std::panic::catch_unwind(|| {
        let creature = [
            os("--creature-model"),
            model.as_os_str(),
            os("--creature-texture"),
            texture.as_os_str(),
        ];
        let missing = [
            os("--creature-model"),
            os("no-existe.obj"),
            os("--creature-texture"),
            os("no-existe.png"),
        ];
        let failing: Vec<(Vec<&OsStr>, &str)> = vec![
            (vec![os("--creature-reveal")], "--creature-reveal necesita"),
            (vec![os("--legacy"), os("--creature-reveal")], "--legacy"),
            (
                vec![os("--legacy"), os("--debug-oxygen"), os("0.5")],
                "--legacy",
            ),
            (vec![os("--debug-oxygen"), os("2")], "--debug-oxygen"),
            (vec![os("--debug-oxygen"), os("NaN")], "--debug-oxygen"),
            (
                vec![os("--debug-oxygen")],
                "Falta el valor despues de --debug-oxygen",
            ),
            // Rutas inexistentes: el error es de validacion, no de IO.
            (
                [
                    &missing[..],
                    &[os("--creature-reveal"), os("--debug-oxygen"), os("-1")],
                ]
                .concat(),
                "--debug-oxygen",
            ),
        ];
        for (args, needle) in failing {
            let mut all = vec![os("--output"), output.as_os_str()];
            all.extend(args.iter().copied());
            let result = run_game(&all);
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert_eq!(result.status.code(), Some(1), "{args:?}: {stderr}");
            assert!(stderr.contains(needle), "{needle:?} no esta en {stderr}");
            assert!(!output.exists(), "{args:?} genero un PNG");
        }
        // La captura no puede sobrescribir una entrada, tampoco con la revelacion.
        let mut args = vec![os("--output"), texture.as_os_str(), os("--creature-reveal")];
        args.extend(creature);
        let result = run_game(&args);
        assert_eq!(result.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&result.stderr).contains("coincide con la entrada"));
        assert_eq!(std::fs::read(&texture).unwrap(), texture_bytes);

        let capture = |name: &str, extra: &[&OsStr]| {
            let path = dir.join(name);
            let mut args = vec![os("--output"), path.as_os_str()];
            args.extend(extra.iter().copied());
            let result = run_game(&args);
            let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
            assert!(
                result.status.success(),
                "{extra:?}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            (std::fs::read(&path).unwrap(), stdout)
        };
        let near = [
            &creature[..],
            &[os("--near-creature"), os("--creature-reveal")],
        ]
        .concat();
        // Tecla 3: parcial desde el primer cuadro (sin transicion en headless).
        let (_, stdout) = capture(
            "o25.png",
            &[&near[..], &[os("--debug-oxygen"), os("0.25")]].concat(),
        );
        assert!(
            stdout.contains("Revelacion por oxigeno: O2 0.25 | visibilidad 68%"),
            "{stdout}"
        );
        assert!(stdout.contains("objetos 6 (descartados"), "{stdout}");
        // O2 1.0 (por defecto): oculta, ni se dibuja ni se cuenta.
        let (_, stdout) = capture("o100.png", &near);
        assert!(stdout.contains("O2 1.00 | visibilidad 0%"), "{stdout}");
        assert!(stdout.contains("objetos 5 (descartados"), "{stdout}");
        // --creature-reveal no mueve la nave y entera es la estatica exacta.
        let (full, stdout) = capture(
            "full.png",
            &[
                &creature[..],
                &[os("--creature-reveal"), os("--debug-oxygen"), os("0.05")],
            ]
            .concat(),
        );
        assert!(stdout.contains("visibilidad 100%"), "{stdout}");
        let (static_frame, _) = capture("static.png", &creature);
        assert_eq!(full, static_frame);
        // --debug-oxygen sin criatura: el cuadro del juego normal no cambia.
        let (plain, _) = capture("plain.png", &[]);
        let (oxygen, stdout) = capture("plain-o25.png", &[os("--debug-oxygen"), os("0.25")]);
        assert_eq!(oxygen, plain);
        assert!(!stdout.contains("Revelacion"), "{stdout}");
    });
    std::fs::remove_dir_all(&dir).unwrap();
    outcome.unwrap();
}

// --- Compositor fantasma (revelacion parcial) ---

fn render_at(world: &World, width: usize, height: usize) -> (Vec<u32>, RenderStats) {
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR).unwrap();
    let stats = world.render(&mut fb);
    (fb.pixels, stats)
}

/// Mismo mundo, camara, O2 y tiempo: sin criatura, criatura estatica opaca y el cuadro con
/// la revelacion activa (que debe ser parcial).
struct Frames {
    base: (Vec<u32>, RenderStats),
    opaque: (Vec<u32>, RenderStats),
    ghost: (Vec<u32>, RenderStats),
    alpha: f64,
}

fn frames(world: &mut World, width: usize, height: usize) -> Frames {
    let reveal = world.creature_reveal().expect("modo activo");
    let alpha = reveal.shown();
    assert!(alpha > 0.0 && alpha < 1.0, "revelacion no parcial: {alpha}");
    let ghost = render_at(world, width, height);
    world.disable_creature_reveal();
    let opaque = render_at(world, width, height);
    let creature = world.clear_creature().unwrap();
    let base = render_at(world, width, height);
    world.set_creature(creature).unwrap();
    world.enable_creature_reveal(reveal.settings());
    assert_eq!(
        world.creature_reveal(),
        Some(reveal),
        "mismo estado al restaurar"
    );
    Frames {
        base,
        opaque,
        ghost,
        alpha,
    }
}

/// El fantasma es exactamente, en cada pixel, la mezcla lineal (una vez) del cuadro real sin
/// criatura y del cuadro con la criatura opaca: fuera de la criatura y donde algo mas cercano
/// la tapa, ambos coinciden y el pixel queda intacto. Devuelve los pixeles cubiertos.
fn assert_ghost_is_one_linear_blend(f: &Frames) -> usize {
    let (base, opaque, ghost) = (&f.base.0, &f.opaque.0, &f.ghost.0);
    for i in 0..ghost.len() {
        assert_eq!(
            ghost[i],
            blend_rgb8(base[i], opaque[i], f.alpha),
            "pixel {i}: fondo {:06x} opaca {:06x}",
            base[i],
            opaque[i]
        );
    }
    let changed = (0..ghost.len()).filter(|&i| ghost[i] != base[i]).count();
    let covered = (0..ghost.len()).filter(|&i| opaque[i] != base[i]).count();
    assert_eq!(
        changed, covered,
        "la mascara no coincide con la cobertura opaca"
    );
    let translucent = (0..ghost.len())
        .filter(|&i| opaque[i] != base[i] && ghost[i] != opaque[i])
        .count();
    assert_eq!(translucent, covered, "algun pixel quedo opaco");
    covered
}

#[test]
fn partial_reveal_blends_the_creature_once_over_the_real_frame_without_extra_writes() {
    let mut world = near_creature_world();
    world.debug_oxygen01 = 0.25;
    world.enable_creature_reveal(RevealSettings::default());
    let f = frames(&mut world, W, H);
    let covered = assert_ghost_is_one_linear_blend(&f);
    assert!(covered > 500, "la criatura casi no se ve: {covered}");

    // Contadores: los del cuadro sin criatura mas exactamente los de dibujar la criatura al
    // final, sobre ese cuadro y su z-buffer. La mezcla no suma escrituras.
    let creature = world.clear_creature().unwrap();
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let mut expected = world.render(&mut fb);
    expected += creature
        .render(&mut fb, &world.camera, None, world.system.light_position())
        .unwrap();
    assert_eq!(f.ghost.1, expected);
    // Los contadores aditivos coinciden con los de la variante estatica.
    let additive = |s: &RenderStats| {
        [
            s.objects_submitted,
            s.objects_culled,
            s.vertices_processed,
            s.triangles_submitted,
            s.triangles_culled,
            s.triangles_clipped,
            s.triangles_backface,
            s.triangles_rasterized,
        ]
    };
    assert_eq!(additive(&f.ghost.1), additive(&f.opaque.1));
    // Repetible.
    world.set_creature(creature).unwrap();
    assert_eq!(render(&world), f.ghost);
}

/// Dos cuadrados paralelos: el lejano (mas grande) va primero en la lista de caras, asi que
/// en la zona comun cada pixel se sombrea dos veces y solo el cercano debe mezclarse.
fn stacked_quads() -> Model {
    parse_obj(
        "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\n\
         v -0.5 -0.5 1\nv 0.5 -0.5 1\nv 0.5 0.5 1\nv -0.5 0.5 1\n\
         vt 0.25 0.25\nvt 0.75 0.75\nvn 0 0 1\n\
         f 1/1/1 2/1/1 3/1/1\nf 1/1/1 3/1/1 4/1/1\n\
         f 5/2/1 6/2/1 7/2/1\nf 5/2/1 7/2/1 8/2/1\n",
    )
    .unwrap()
}

#[test]
fn overlapping_faces_blend_only_the_nearest_surface_once() {
    let mut world = new_world();
    let placement = CreaturePlacement::new(DEMO_CENTER, DEMO_WORLD_RADIUS, 0.0).unwrap();
    world
        .set_creature(StaticCreature::new(stacked_quads(), rgb_texture(), placement).unwrap())
        .unwrap();
    world.place_near_creature().unwrap();
    // Sin nave, para que nada tape la zona comun.
    world.ship_model = no_ship();
    world.debug_oxygen01 = 0.3;
    world.enable_creature_reveal(RevealSettings::default());
    let f = frames(&mut world, W, H);
    let covered = assert_ghost_is_one_linear_blend(&f);
    // Hubo mas fragmentos que pixeles cubiertos: la zona comun se sombreo dos veces.
    let fragments = f.ghost.1.fragments_evaluated - f.base.1.fragments_evaluated;
    assert!(
        fragments > covered + 100,
        "sin superposicion: {fragments} fragmentos, {covered} pixeles"
    );
    // En la cobertura hay dos colores opacos (lejano y cercano); el fantasma solo mezcla el
    // cercano en la zona comun (mezclar cada fragmento daria otro color y fallaria arriba).
    let colors: std::collections::BTreeSet<u32> = (0..W * H)
        .filter(|&i| f.opaque.0[i] != f.base.0[i])
        .map(|i| f.opaque.0[i])
        .collect();
    assert_eq!(colors.len(), 2, "dos cuadrados de color unico: {colors:x?}");
}

#[test]
fn the_ghost_keeps_foreground_occluders_and_shows_what_is_behind_it() {
    // Criatura pequena entre la camara y la nave: la nave se ve a traves del fantasma.
    let mut world = new_world();
    let (eye, ship) = (world.camera.position, world.ship.position);
    world
        .set_creature(cube_creature(eye + (ship - eye) * 0.5, 1.5))
        .unwrap();
    world.debug_oxygen01 = 0.25;
    world.enable_creature_reveal(RevealSettings::default());
    let f = frames(&mut world, W, H);
    assert_ghost_is_one_linear_blend(&f);
    let ship_model = std::mem::replace(&mut world.ship_model, no_ship());
    let creature = world.clear_creature().unwrap();
    let (no_ship_frame, _) = render(&world);
    world.ship_model = ship_model;
    world.set_creature(creature).unwrap();
    let ship_seen_through = (0..W * H)
        .filter(|&i| {
            f.base.0[i] != no_ship_frame[i] // hay nave
                && f.opaque.0[i] != f.base.0[i] // la criatura opaca la tapa
                && f.ghost.0[i] != f.opaque.0[i] // el fantasma deja ver la nave
        })
        .count();
    assert!(ship_seen_through > 50, "nave detras: {ship_seen_through}");

    // Detras de la nave (arranque frente a la criatura): la nave queda intacta.
    let mut world = near_creature_world();
    world.debug_oxygen01 = 0.25;
    world.enable_creature_reveal(RevealSettings::default());
    let f = frames(&mut world, W, H);
    assert_ghost_is_one_linear_blend(&f);
    let ship_model = std::mem::replace(&mut world.ship_model, no_ship());
    let creature = world.clear_creature().unwrap();
    let (no_ship_frame, _) = render(&world);
    world.ship_model = ship_model;
    world.set_creature(creature).unwrap();
    let ship_pixels: Vec<usize> = (0..W * H)
        .filter(|&i| f.base.0[i] != no_ship_frame[i])
        .collect();
    assert!(ship_pixels.len() > 100);
    assert!(
        ship_pixels.iter().all(|&i| f.ghost.0[i] == f.base.0[i]),
        "tapa la nave"
    );

    // Delante de un planeta: el planeta se ve a traves. Detras: el planeta la tapa entera.
    let mut world = new_world();
    world.place_near(1, 25.0).unwrap();
    let planet = world.system.bodies[1].position(world.time);
    let eye = world.camera.position;
    let to_planet = (planet - eye).normalized();
    let surface = (planet - eye).length() - world.system.bodies[1].body_radius;
    world.debug_oxygen01 = 0.25;
    world.enable_creature_reveal(RevealSettings::default());
    world
        .set_creature(cube_creature(eye + to_planet * (surface * 0.8), 2.0))
        .unwrap();
    let f = frames(&mut world, W, H);
    assert_ghost_is_one_linear_blend(&f);
    let planet_seen_through = (0..W * H)
        .filter(|&i| {
            f.base.0[i] != SPACE_COLOR
                && f.opaque.0[i] != f.base.0[i]
                && f.ghost.0[i] != f.opaque.0[i]
        })
        .count();
    assert!(
        planet_seen_through > 20,
        "planeta detras: {planet_seen_through}"
    );
    world
        .set_creature(cube_creature(planet + to_planet * 30.0, 3.0))
        .unwrap();
    let (behind, _) = render(&world);
    world.clear_creature();
    assert_eq!(behind, render(&world).0, "detras del planeta no se ve");
}

#[test]
fn work_buffers_exist_only_for_visible_partial_frames_and_follow_the_resolution() {
    let mut world = near_creature_world();
    world.debug_oxygen01 = 1.0;
    world.enable_creature_reveal(RevealSettings::default());
    // Extremos: sin buffers.
    let _ = render(&world);
    assert_eq!(world.creature_reveal_buffer_bytes(), 0, "oculta");
    world.debug_oxygen01 = 0.05;
    world.snap_creature_reveal();
    let _ = render(&world);
    assert_eq!(world.creature_reveal_buffer_bytes(), 0, "entera");

    // Parcial pero fuera del frustum: se descarta antes de copiar nada y se cuenta como la
    // estatica descartada.
    let mut culled = new_world();
    culled
        .set_creature(cube_creature(Vec3::new(0.0, 40.0, 1200.0), 50.0))
        .unwrap();
    culled.debug_oxygen01 = 0.25;
    culled.enable_creature_reveal(RevealSettings::default());
    let frame = render(&culled);
    assert_eq!(culled.creature_reveal_buffer_bytes(), 0, "descartada");
    let faces = culled.creature().unwrap().model().faces.len();
    culled.clear_creature();
    let (base, mut expected) = render(&culled);
    assert_eq!(frame.0, base);
    expected.objects_submitted += 1;
    expected.objects_culled += 1;
    expected.triangles_submitted += faces;
    expected.triangles_culled += faces;
    assert_eq!(frame.1, expected);

    // Parcial visible: buffers del tamano del cuadro, reutilizados entre cuadros.
    world.debug_oxygen01 = 0.25;
    world.snap_creature_reveal();
    let first = render(&world);
    let bytes = world.creature_reveal_buffer_bytes();
    assert!(bytes >= W * H * 5, "{bytes}");
    assert_eq!(render(&world), first);
    assert_eq!(world.creature_reveal_buffer_bytes(), bytes, "sin crecer");
    // Otra resolucion: resultado correcto con buffers ajustados, y de vuelta igual que antes.
    let f = frames(&mut world, 160, 90);
    assert_ghost_is_one_linear_blend(&f);
    assert_eq!(render(&world), first);
}
