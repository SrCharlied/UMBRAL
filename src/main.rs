use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::PathBuf;
use umbral::{
    app::{GameOptions, run_game},
    assets::{obj::load_model, ship_model_path},
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        image::save_png,
        legacy::{BACKGROUND, render_model},
    },
    visual::{
        creature_reveal::{parse_debug_oxygen, validate_reveal_flags},
        creature_viewer::validate_output_path,
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
    let mut creature_reveal_stages = false;
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
                    "cargo run --release -- [--headless] [--output archivo.png] [--frames N] [--width N] [--height N]\nResolucion por defecto: {DEFAULT_WIDTH}x{DEFAULT_HEIGHT} (maximo {MAX_PIXELS} pixeles)\nA/D o flechas: rotar | R: restablecer | P: guardar PNG | Esc: salir\nCriatura estatica (debug, opt-in): --creature-model modelo.obj --creature-texture textura.png (siempre juntos) [--near-creature: arrancar frente a ella; R vuelve ahi]. Los flags de criatura son incompatibles con --legacy; solo el arranque --near-creature es incompatible con --near. Con criatura, --output debe ser .png y distinto de las entradas\nRevelacion por oxigeno (debug, opt-in): --creature-reveal (requiere la criatura; no mueve la nave) | --debug-oxygen X: oxigeno de depuracion inicial en [0, 1] (por defecto 1); teclas 1-4: 100/50/25/5 %\nRevelacion por etapas (debug, opt-in): --creature-reveal-stages (requiere la criatura; incompatible con --creature-reveal; no mueve la nave): primero el ojo y despues el cuerpo"
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
    if let Some(creature) = &creature {
        validate_output_path(&output, &[&creature.model, &creature.texture])?;
    }
    if !legacy {
        return run_game(GameOptions {
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
