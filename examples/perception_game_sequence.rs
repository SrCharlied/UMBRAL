//! Secuencia de evidencia de `--lucid-perception` en el juego (UMB-310): la escena del juego
//! `--lucid-perception --lucid-surface --void-eye --near 3` con los helpers REALES del bucle
//! (`OxygenTransition`, `advance_perception_frame`, `apply_lucid_perception`, `ship_focus`).
//! Ver `docs/perception-game.md`.
//!
//! `cargo run --release --locked --offline --example perception_game_sequence -- --output-dir <carpeta-nueva> [--width N --height N]`
//!
//! Es un GUION: los objetivos de O2 (presets 1-4 y `N`) y el `F5` llegan en cuadros fijos como
//! eventos logicos, no como teclado fisico, y los segundos reales son un paso fijo de 1/12 s (en
//! el juego son segundos monotonicos de `Instant`). La nave no recibe mando (sin empuje ni giro).
//! Nada consume O2 solo, y no hay combate ni ataque. Todo se valida antes de crear la carpeta,
//! que no debe existir.

use std::{fmt::Write as _, path::PathBuf};
use umbral::{
    app::advance_perception_frame,
    assets::{obj::load_model, ship_model_path},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        image::save_png,
    },
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        lucid_game::set_lucid_surface,
        perception_game::{
            OXYGEN_TRANSITION_RATE, OxygenKeys, OxygenTransition, apply_lucid_perception,
            perception_title, ship_focus,
        },
        void_game::set_void_eye,
    },
};

const DEFAULT_SIZE: (usize, usize) = (640, 360);
/// Arranque del juego que reproduce la escena (`--near 3 --time 1.3 --debug-oxygen 1`).
const NEAR_BODY: usize = 3;
const SURFACE_GAP: f64 = 25.0;
const START_TIME: f64 = 1.3;
const START_OXYGEN: f64 = 1.0;
/// Paso fijo de segundos reales del guion (12 cuadros por segundo).
const FPS: f64 = 12.0;
const LAST_FRAME: usize = 162;
/// Cuadros (a 12 fps) donde `F5` congela y descongela el reloj orbital.
const FREEZE_FROM: usize = 60;
const FREEZE_UNTIL: usize = 84;

/// Eventos logicos del guion: (cuadro, nombre, teclas sobre el OBJETIVO).
fn script() -> Vec<(usize, &'static str, OxygenKeys)> {
    let preset = |value: f64| OxygenKeys {
        preset: Some(value),
        ..OxygenKeys::default()
    };
    vec![
        (6, "tecla 4: objetivo 5 %", preset(0.05)),
        (
            42,
            "tecla 1 a mitad de la caida: reversion a 100 %",
            preset(1.0),
        ),
        (60, "tecla 4: objetivo 5 % otra vez", preset(0.05)),
        (
            96,
            "tecla N: objetivo 0 % (blackout)",
            OxygenKeys {
                decrease: true,
                ..OxygenKeys::default()
            },
        ),
        (108, "tecla 1: recuperacion a 100 %", preset(1.0)),
    ]
}

fn usage() -> String {
    "Uso: perception_game_sequence --output-dir <carpeta-nueva> [--width N --height N]".into()
}

fn parse_args() -> Result<(PathBuf, usize, usize), String> {
    let (mut output, mut width, mut height) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(usage)?;
        let size = |v: &str| {
            v.parse::<usize>()
                .ok()
                .filter(|&n| n > 0)
                .ok_or(format!("{flag} necesita un entero positivo: {v}"))
        };
        match flag.as_str() {
            "--output-dir" if output.is_none() => output = Some(PathBuf::from(value)),
            "--width" if width.is_none() => width = Some(size(&value)?),
            "--height" if height.is_none() => height = Some(size(&value)?),
            _ => return Err(usage()),
        }
    }
    let output = output.ok_or_else(usage)?;
    let (width, height) = (
        width.unwrap_or(DEFAULT_SIZE.0),
        height.unwrap_or(DEFAULT_SIZE.1),
    );
    if width.checked_mul(height).is_none_or(|n| n > MAX_PIXELS) {
        return Err(format!("{width}x{height} (maximo {MAX_PIXELS} pixeles)"));
    }
    if output.exists() {
        return Err(format!(
            "la carpeta de salida ya existe: {}",
            output.display()
        ));
    }
    Ok((output, width, height))
}

fn luma(p: u32) -> f64 {
    let c = |s: u32| f64::from((p >> s) & 0xff);
    0.299 * c(16) + 0.587 * c(8) + 0.114 * c(0)
}

fn main() -> Result<(), String> {
    let (output, width, height) = parse_args()?;
    // Escena completa en memoria antes de escribir nada.
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    world.time = START_TIME;
    world.debug_oxygen01 = START_OXYGEN;
    set_lucid_surface(&mut world, true)?;
    set_void_eye(&mut world, true)?;
    world.place_near(NEAR_BODY, SURFACE_GAP)?;
    let mut oxygen = OxygenTransition::new(START_OXYGEN)?;
    let mut frame = Framebuffer::new(width, height, SPACE_COLOR)?;
    let events = script();
    std::fs::create_dir(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    std::fs::create_dir(output.join("frames")).map_err(|e| e.to_string())?;
    let still = ShipInput::default();
    let mut rows = String::new();
    for index in 0..=LAST_FRAME {
        let mut event = "";
        if index > 0 {
            // F5 logico: congela solo el reloj orbital; la transicion sigue.
            world.toggles.freeze_orbits = (FREEZE_FROM..FREEZE_UNTIL).contains(&index);
            let keys = match events.iter().find(|(at, _, _)| *at == index) {
                Some((_, name, keys)) => {
                    event = name;
                    *keys
                }
                None => OxygenKeys::default(),
            };
            advance_perception_frame(&mut world, &mut oxygen, keys, &still, 1.0 / FPS)?;
        }
        world.render(&mut frame);
        let focus = ship_focus(&world);
        apply_lucid_perception(&mut frame, &world)?;
        let name = format!("frames/{index:03}.png");
        save_png(&output.join(&name), &frame)?;
        let pixels = &frame.pixels;
        let black = pixels.iter().filter(|&&p| p == 0).count() as f64 / pixels.len() as f64;
        let mean = pixels.iter().map(|&p| luma(p)).sum::<f64>() / pixels.len() as f64;
        if !rows.is_empty() {
            rows += ",\n";
        }
        write!(
            rows,
            "    {{\"frame\": {index}, \"file\": \"{name}\", \"real_seconds\": {:.6}, \"world_time\": {:.6}, \"freeze_orbits\": {}, \"event\": \"{event}\", \"oxygen_target\": {:.6}, \"oxygen_shown\": {:.6}, \"world_debug_oxygen01\": {:.6}, \"focus_uv\": [{:.6}, {:.6}], \"focus_px\": [{:.1}, {:.1}], \"focus_projected\": {}, \"title\": \"{}\", \"black_fraction\": {black:.6}, \"mean_luma\": {mean:.3}}}",
            index as f64 / FPS,
            world.time,
            world.toggles.freeze_orbits,
            oxygen.target(),
            oxygen.shown(),
            world.debug_oxygen01,
            focus.uv[0],
            focus.uv[1],
            focus.uv[0] * width as f64,
            focus.uv[1] * height as f64,
            focus.projected,
            perception_title(&oxygen)
        )
        .map_err(|e| e.to_string())?;
    }
    let mut events_json = String::new();
    for (at, name, keys) in &events {
        if !events_json.is_empty() {
            events_json += ", ";
        }
        write!(
            events_json,
            "{{\"frame\": {at}, \"real_seconds\": {:.6}, \"event\": \"{name}\", \"preset\": {}, \"increase\": {}, \"decrease\": {}}}",
            *at as f64 / FPS,
            keys.preset.map_or("null".into(), |v| v.to_string()),
            keys.increase,
            keys.decrease
        )
        .map_err(|e| e.to_string())?;
    }
    let manifest = format!(
        "{{\n  \"what\": \"UMB-310 --lucid-perception en el juego: secuencia GUIONIZADA con los helpers reales del bucle\",\n  \"equivalent_game_command\": \"umbral --lucid-perception --lucid-surface --void-eye --near {NEAR_BODY} --time {START_TIME} --debug-oxygen {START_OXYGEN} --width {width} --height {height}\",\n  \"size\": [{width}, {height}],\n  \"start\": {{\"world_time\": {START_TIME}, \"oxygen_target\": {START_OXYGEN}, \"oxygen_shown\": {START_OXYGEN}, \"body\": {NEAR_BODY}, \"surface_gap\": {SURFACE_GAP}}},\n  \"clocks\": {{\"real_seconds\": \"paso fijo 1/{FPS} s del guion (en el juego: Instant monotonico, sin el limite de la fisica y sin F5)\", \"world_time\": \"reloj orbital del mundo (avanza con el dt limitado de la fisica; F5 lo congela)\"}},\n  \"transition_rate_o2_per_real_second\": {OXYGEN_TRANSITION_RATE},\n  \"ship_input\": \"ninguno (ShipInput::default)\",\n  \"freeze_orbits_frames\": [{FREEZE_FROM}, {FREEZE_UNTIL}],\n  \"events\": [{events_json}],\n  \"not_included\": [\"teclado fisico\", \"consumo automatico de O2\", \"combate o ataque\"],\n  \"frames\": [\n{rows}\n  ]\n}}\n"
    );
    std::fs::write(output.join("manifest.json"), manifest).map_err(|e| e.to_string())?;
    println!(
        "Secuencia: {} cuadros {width}x{height} en {}",
        LAST_FRAME + 1,
        output.display()
    );
    Ok(())
}
