use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::PathBuf;
use umbral::{
    app::{
        GameOptions, run_game, run_game_with_distant_turn, run_game_with_eye_comparison,
        run_game_with_horizontal_follow, run_game_with_solar_reveal,
    },
    assets::{obj::load_model, ship_model_path},
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        image::save_png,
        legacy::{BACKGROUND, render_model},
    },
    visual::{
        creature_distant_turn::validate_distant_turn_flags,
        creature_eye_comparison::validate_eye_compare_flags,
        creature_eye_follow::validate_eye_follow_flags,
        creature_horizontal_follow::validate_horizontal_follow_flags,
        creature_reveal::{parse_debug_oxygen, validate_reveal_flags},
        creature_viewer::validate_output_path,
        solar_game::{SolarRevealRequest, validate_solar_reveal_flags},
        staged_creature_reveal::validate_staged_flags,
        static_creature::CreatureOptions,
    },
};

fn positive_argument(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<usize, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("Falta la cantidad despues de {flag}"))?
        .parse::<usize>()
        .map_err(|_| format!("{flag} necesita un entero positivo"))?;
    if value == 0 {
        return Err(format!("{flag} debe ser mayor que cero"));
    }
    Ok(value)
}

fn run() -> Result<(), String> {
    let mut headless = false;
    let mut frame_limit = None;
    let (mut width, mut height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut output = PathBuf::from("renders/nave.png");
    let (mut legacy, mut time, mut near, mut culling) = (false, 0.0, None, true);
    let (mut lod, mut overlay) = (true, false);
    let (mut creature_model, mut creature_texture, mut near_creature) = (None, None, false);
    let (mut creature_reveal, mut debug_oxygen) = (false, None);
    let (mut creature_reveal_stages, mut creature_eye_follow) = (false, false);
    let (mut creature_eye_compare, mut creature_distant_turn) = (false, false);
    let (mut creature_horizontal_follow, mut creature_solar_reveal) = (false, false);
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--headless" => headless = true,
            "--output" => {
                output = PathBuf::from(args.next().ok_or("Falta la ruta despues de --output")?)
            }
            "--legacy" => legacy = true,
            "--no-culling" => culling = false,
            "--no-lod" => lod = false,
            "--overlay" => overlay = true,
            "--time" => {
                time = args
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|t| t.is_finite() && *t >= 0.0)
                    .ok_or("--time necesita segundos >= 0")?
            }
            "--near" => {
                near = Some(
                    args.next()
                        .and_then(|v| v.parse::<usize>().ok())
                        .ok_or("--near necesita el indice de un cuerpo (0 = Sol)")?,
                )
            }
            "--creature-model" => {
                creature_model = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --creature-model")?,
                ))
            }
            "--creature-texture" => {
                creature_texture = Some(PathBuf::from(
                    args.next()
                        .ok_or("Falta la ruta despues de --creature-texture")?,
                ))
            }
            "--near-creature" => near_creature = true,
            "--creature-reveal" => creature_reveal = true,
            "--creature-reveal-stages" => creature_reveal_stages = true,
            "--creature-eye-follow" => creature_eye_follow = true,
            "--creature-eye-compare" => creature_eye_compare = true,
            "--creature-distant-turn" => creature_distant_turn = true,
            "--creature-horizontal-follow" => creature_horizontal_follow = true,
            "--creature-solar-reveal" => creature_solar_reveal = true,
            "--debug-oxygen" => {
                debug_oxygen = Some(parse_debug_oxygen(
                    &args
                        .next()
                        .ok_or("Falta el valor despues de --debug-oxygen")?,
                )?)
            }
            "--frames" => frame_limit = Some(positive_argument(&mut args, "--frames")?),
            "--width" => width = positive_argument(&mut args, "--width")?,
            "--height" => height = positive_argument(&mut args, "--height")?,
            "--help" | "-h" => {
                println!(
                    "cargo run --release -- [--headless] [--output archivo.png] [--frames N] [--width N] [--height N]\nResolucion por defecto: {DEFAULT_WIDTH}x{DEFAULT_HEIGHT} (maximo {MAX_PIXELS} pixeles)\nA/D o flechas: rotar | R: restablecer | P: guardar PNG | Esc: salir\nCriatura estatica (debug, opt-in): --creature-model modelo.obj --creature-texture textura.png (siempre juntos) [--near-creature: arrancar frente a ella; R vuelve ahi]. Los flags de criatura son incompatibles con --legacy; solo el arranque --near-creature es incompatible con --near. Con criatura, --output debe ser .png y distinto de las entradas\nRevelacion por oxigeno (debug, opt-in): --creature-reveal (requiere la criatura; no mueve la nave) | --debug-oxygen X: oxigeno de depuracion inicial en [0, 1] (por defecto 1); teclas 1-4: 100/50/25/5 %\nRevelacion por etapas (debug, opt-in): --creature-reveal-stages (requiere la criatura; incompatible con --creature-reveal; no mueve la nave): primero el ojo y despues el cuerpo\nMirada local (debug, opt-in): --creature-eye-follow (requiere --creature-reveal-stages): el ojo mira a la nave dentro de sus limites; cuerpo y tentaculos quietos\nComparacion de la mirada (debug, opt-in): --creature-eye-compare (requiere --creature-eye-follow): arranca mirando a la camara; G alterna NAVE/CAMARA; panel inferior\nGiro global lejano (candidata, debug, opt-in): --creature-distant-turn (requiere --creature-eye-compare): de lejos la criatura entera gira lenta y limitada (20/12 grados, t95 8 s) hacia el punto de G; de cerca solo el ojo. Arranca LEJOS; H giro ON/OFF, 7 CERCA, 8 LEJOS, R repite el preset\nSeguimiento horizontal (candidata 02, debug, opt-in): --creature-horizontal-follow (requiere --creature-eye-compare; no con --creature-distant-turn): cerca y lejos el cuerpo entero gira solo en horizontal, lento (t95 4 s, tope 18 grados/s), hacia el punto de G en cualquier lado. Arranca LEJOS; H seguimiento ON/OFF (OFF vuelve al frente), 7 CERCA, 8 LEJOS, R repite el preset\nSol con ojo (composicion aprobada 03, opt-in): --creature-solar-reveal (requiere --creature-model y --creature-texture; implica etapas, mirada, comparacion y seguimiento horizontal 02, que pueden repetirse; no con --legacy, --creature-reveal, --near, --near-creature ni --creature-distant-turn): la criatura en el centro del Sol (radio 80, yaw 0), mirada a la camara y luz de camara; arranca FRONTAL. 7 FRONTAL, 8 OBLICUA, R vuelve a FRONTAL, H seguimiento ON/OFF, G CAMARA/NAVE, 1-4 y M/N el O2 (titulo de la ventana)"
                );
                return Ok(());
            }
            _ => return Err(format!("Argumento desconocido: {argument}. Usa --help.")),
        }
    }
    let creature = CreatureOptions::from_flags(
        creature_model,
        creature_texture,
        near_creature,
        near,
        legacy,
    )?;
    // Antes que los demas: el flag solar implica etapas, mirada, comparacion y seguimiento
    // horizontal (redundantes si se pasan) y rechaza las rutas que cambian su composicion.
    validate_solar_reveal_flags(
        creature_solar_reveal,
        SolarRevealRequest {
            has_creature: creature.is_some(),
            legacy,
            near_body: near.is_some(),
            near_creature,
            uniform_reveal: creature_reveal,
            distant_turn: creature_distant_turn,
        },
    )?;
    if creature_solar_reveal {
        creature_reveal_stages = true;
        creature_eye_follow = true;
        creature_eye_compare = true;
        creature_horizontal_follow = true;
    }
    validate_reveal_flags(
        creature_reveal,
        creature.is_some(),
        debug_oxygen.is_some(),
        legacy,
    )?;
    validate_staged_flags(
        creature_reveal_stages,
        creature.is_some(),
        creature_reveal,
        legacy,
    )?;
    validate_eye_follow_flags(
        creature_eye_follow,
        creature_reveal_stages,
        creature.is_some(),
        legacy,
    )?;
    validate_eye_compare_flags(creature_eye_compare, creature_eye_follow, legacy)?;
    validate_distant_turn_flags(creature_distant_turn, creature_eye_compare, legacy)?;
    validate_horizontal_follow_flags(
        creature_horizontal_follow,
        creature_eye_compare,
        creature_distant_turn,
        legacy,
    )?;
    if let Some(creature) = &creature {
        validate_output_path(&output, &[&creature.model, &creature.texture])?;
    }
    if !legacy {
        let game = if creature_solar_reveal {
            run_game_with_solar_reveal
        } else if creature_horizontal_follow {
            run_game_with_horizontal_follow
        } else if creature_distant_turn {
            run_game_with_distant_turn
        } else if creature_eye_compare {
            run_game_with_eye_comparison
        } else {
            run_game
        };
        return game(GameOptions {
            width,
            height,
            headless,
            frame_limit,
            output,
            time,
            near,
            culling,
            lod,
            overlay,
            creature,
            creature_reveal,
            creature_reveal_stages,
            creature_eye_follow,
            debug_oxygen: debug_oxygen.unwrap_or(1.0),
        });
    }
    let model = load_model(&ship_model_path())?;
    let mut framebuffer = Framebuffer::new(width, height, BACKGROUND)
        .map_err(|e| format!("{e}: {width}x{height} (maximo {MAX_PIXELS} pixeles)"))?;
    let mut angle = 0.0;
    let stats = render_model(&mut framebuffer, &model, angle)?;
    println!(
        "Modelo: {} vertices, {} caras, {} materiales",
        model.vertices.len(),
        model.faces.len(),
        model.materials.len()
    );
    println!(
        "Caras recorridas: {} | Escrituras de pixel: {}",
        stats.faces_processed, stats.pixels_written
    );
    if headless {
        save_png(&output, &framebuffer)?;
        println!("PNG: {} ({width}x{height})", output.display());
        return Ok(());
    }
    let mut window = Window::new(
        "UMBRAL - Carga de modelos OBJ - 24531",
        width,
        height,
        WindowOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut frames = 0;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let previous_angle = angle;
        if window.is_key_down(Key::A) || window.is_key_down(Key::Left) {
            angle -= 0.03;
        }
        if window.is_key_down(Key::D) || window.is_key_down(Key::Right) {
            angle += 0.03;
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            angle = 0.0;
        }
        if angle != previous_angle {
            render_model(&mut framebuffer, &model, angle)?;
        }
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            save_png(&output, &framebuffer)?;
            println!("PNG: {}", output.display());
        }
        // minifb presenta pixeles ya calculados; no dibuja los triangulos del modelo.
        window
            .update_with_buffer(&framebuffer.pixels, width, height)
            .map_err(|e| e.to_string())?;
        frames += 1;
        if frame_limit.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    println!("Ventana: {frames} frames mostrados");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
