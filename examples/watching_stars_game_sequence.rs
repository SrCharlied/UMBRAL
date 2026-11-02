//! Secuencia de evidencia de `--watching-stars` en el juego (UMB-310): la escena del juego
//! normal `--watching-stars --debug-oxygen 1` con los helpers REALES del bucle
//! (`OxygenTransition`, `advance_perception_frame`, `render_game_frame`, que pone los ojos
//! aprobados sobre la escena, y `apply_lucid_perception` como ultimo paso). Ver
//! `docs/watching-stars-game.md`.
//!
//! `cargo run --release --locked --offline --example watching_stars_game_sequence -- --output-dir <carpeta-nueva> [--width N --height N]`
//!
//! Es un GUION: los objetivos de O2 (presets 1-4, `N`), `F5`, `R` y el mando de la nave llegan en
//! cuadros fijos como eventos logicos, no como teclado fisico, y los segundos reales son un paso
//! fijo de 1/12 s (en el juego son segundos monotonicos de `Instant`). No mide FPS. Nada consume
//! O2 solo, y no hay combate ni ataque. Todo se valida antes de crear la carpeta, que no debe
//! existir.

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
        perception_game::{
            OXYGEN_TRANSITION_RATE, OxygenKeys, OxygenTransition, apply_lucid_perception,
            ship_focus,
        },
        watching_stars_game::render_game_frame,
    },
};

const DEFAULT_SIZE: (usize, usize) = (640, 360);
const START_OXYGEN: f64 = 1.0;
/// Paso fijo de segundos reales del guion (12 cuadros por segundo).
const FPS: f64 = 12.0;
const LAST_FRAME: usize = 210;
/// Cuadros donde `F5` congela el reloj orbital (la transicion de O2 sigue).
const FREEZE_FROM: usize = 54;
const FREEZE_UNTIL: usize = 78;
/// Cuadro de `R` (juego normal: la nave vuelve al arranque; objetivo y O2 mostrado se conservan).
const RESET_AT: usize = 66;

/// Eventos logicos de O2: (cuadro, nombre, teclas sobre el OBJETIVO).
fn script() -> Vec<(usize, &'static str, OxygenKeys)> {
    let preset = |value: f64| OxygenKeys {
        preset: Some(value),
        ..OxygenKeys::default()
    };
    let decrease = OxygenKeys {
        decrease: true,
        ..OxygenKeys::default()
    };
    vec![
        (6, "tecla 3: objetivo 25 %", preset(0.25)),
        (
            24,
            "tecla 2 a mitad de la caida: objetivo 50 %",
            preset(0.5),
        ),
        (36, "tecla N: objetivo 45 %", decrease),
        (42, "tecla 3: objetivo 25 %", preset(0.25)),
        (
            60,
            "tecla 1 con F5 activo: recuperacion a 100 %",
            preset(1.0),
        ),
        (
            100,
            "teclas 4 y N: objetivo 0 % (blackout)",
            OxygenKeys {
                preset: Some(0.05),
                decrease: true,
                ..OxygenKeys::default()
            },
        ),
        (156, "tecla 1: recuperacion a 100 %", preset(1.0)),
    ]
}

/// Mando logico de la nave (sin teclado): gira y sube un rato para que la camara real y la nave
/// proyectada cambien y las pupilas las sigan.
fn ship_input(frame: usize) -> (&'static str, ShipInput) {
    match frame {
        10..=50 => (
            "giro a la izquierda",
            ShipInput {
                yaw: 1.0,
                ..ShipInput::default()
            },
        ),
        110..=140 => (
            "empuje y cabeceo",
            ShipInput {
                thrust: 1.0,
                pitch: 0.6,
                ..ShipInput::default()
            },
        ),
        _ => ("ninguno", ShipInput::default()),
    }
}

fn usage() -> String {
    "Uso: watching_stars_game_sequence --output-dir <carpeta-nueva> [--width N --height N]".into()
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

fn v3(v: umbral::math::Vec3) -> String {
    format!("[{:.6}, {:.6}, {:.6}]", v.x, v.y, v.z)
}

fn ids(list: &[usize]) -> String {
    list.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn main() -> Result<(), String> {
    let (output, width, height) = parse_args()?;
    // Escena completa en memoria antes de escribir nada.
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    world.debug_oxygen01 = START_OXYGEN;
    let mut oxygen = OxygenTransition::new(START_OXYGEN)?;
    let mut frame = Framebuffer::new(width, height, SPACE_COLOR)?;
    let events = script();
    std::fs::create_dir(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    std::fs::create_dir(output.join("frames")).map_err(|e| e.to_string())?;
    let mut rows = String::new();
    for index in 0..=LAST_FRAME {
        let mut event = String::new();
        let (input_name, input) = ship_input(index);
        if index > 0 {
            // En el orden del bucle del juego: R, F5, teclas de O2 y despues el cuadro.
            if index == RESET_AT {
                world.reset_ship();
                event += "R: la nave vuelve al arranque (O2 conservado); ";
            }
            // F5 logico: congela solo el reloj orbital; la transicion sigue.
            world.toggles.freeze_orbits = (FREEZE_FROM..FREEZE_UNTIL).contains(&index);
            let keys = match events.iter().find(|(at, _, _)| *at == index) {
                Some((_, name, keys)) => {
                    event += name;
                    *keys
                }
                None => OxygenKeys::default(),
            };
            advance_perception_frame(&mut world, &mut oxygen, keys, &input, 1.0 / FPS)?;
        }
        // El helper de PRODUCCION: escena y ojos; despues el ultimo paso aprobado.
        let drawn = render_game_frame(&world, &mut frame, None, true)?;
        let eyes = drawn.watching_stars.ok_or("sin informe de los ojos")?;
        let focus = ship_focus(&world);
        apply_lucid_perception(&mut frame, &world)?;
        let name = format!("frames/{index:03}.png");
        save_png(&output.join(&name), &frame)?;
        let drawn_ids: Vec<usize> = eyes.eyes.iter().map(|e| e.id).collect();
        let legible = eyes.eyes.iter().filter(|e| e.weight >= 0.5).count();
        if !rows.is_empty() {
            rows += ",\n";
        }
        write!(
            rows,
            "    {{\"frame\": {index}, \"file\": \"{name}\", \"real_seconds\": {:.6}, \"world_time\": {:.6}, \"freeze_orbits\": {}, \"event\": \"{}\", \"ship_input\": \"{input_name}\", \"oxygen_target\": {:.6}, \"oxygen_shown\": {:.6}, \"world_debug_oxygen01\": {:.6}, \"eyes_oxygen01\": {:.6}, \"eyes\": {{\"active\": {}, \"projected\": {}, \"occluded\": {}, \"drawn\": {}, \"legible_weight_ge_0_5\": {legible}, \"drawn_ids\": [{}]}}, \"ship_focus_px\": {}, \"perception_focus_uv\": [{:.6}, {:.6}], \"perception_focus_projected\": {}, \"camera_position\": {}, \"camera_target\": {}, \"ship_position\": {}, \"triangles_rasterized\": {}}}",
            index as f64 / FPS,
            world.time,
            world.toggles.freeze_orbits,
            event.trim_end_matches("; "),
            oxygen.target(),
            oxygen.shown(),
            world.debug_oxygen01,
            eyes.oxygen01,
            eyes.active_ids.len(),
            eyes.projected_ids.len(),
            eyes.occluded_ids.len(),
            eyes.eyes.len(),
            ids(&drawn_ids),
            eyes.ship_focus_px
                .map_or("null".into(), |p| format!("[{:.3}, {:.3}]", p[0], p[1])),
            focus.uv[0],
            focus.uv[1],
            focus.projected,
            v3(world.camera.position),
            v3(world.camera.target),
            v3(world.ship.position),
            drawn.stats.triangles_rasterized
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
        "{{\n  \"what\": \"UMB-310 --watching-stars en el juego: secuencia GUIONIZADA con los helpers reales del bucle (escena, ojos aprobados preview02, percepcion aprobada)\",\n  \"equivalent_game_command\": \"umbral --watching-stars --debug-oxygen {START_OXYGEN} --width {width} --height {height}\",\n  \"size\": [{width}, {height}],\n  \"composition\": \"render_game_frame (World::render y despues los ojos con world.debug_oxygen01, camara real y nave real) -> apply_lucid_perception (ultimo paso)\",\n  \"clocks\": {{\"real_seconds\": \"paso fijo 1/{FPS} s del guion (en el juego: Instant monotonico, sin el limite de la fisica y sin F5)\", \"world_time\": \"reloj orbital del mundo (avanza con el dt limitado de la fisica; F5 lo congela)\"}},\n  \"transition_rate_o2_per_real_second\": {OXYGEN_TRANSITION_RATE},\n  \"ship_input\": \"logico por cuadro: giro 10-50, empuje y cabeceo 110-140, ninguno en el resto\",\n  \"freeze_orbits_frames\": [{FREEZE_FROM}, {FREEZE_UNTIL}],\n  \"reset_frame\": {RESET_AT},\n  \"events\": [{events_json}],\n  \"not_included\": [\"teclado fisico\", \"ventana\", \"FPS o rendimiento\", \"consumo automatico de O2\", \"combate o ataque\"],\n  \"frames\": [\n{rows}\n  ]\n}}\n"
    );
    std::fs::write(output.join("manifest.json"), manifest).map_err(|e| e.to_string())?;
    println!(
        "Secuencia: {} cuadros {width}x{height} en {}",
        LAST_FRAME + 1,
        output.display()
    );
    Ok(())
}
