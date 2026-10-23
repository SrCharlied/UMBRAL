use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::path::PathBuf;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::{
        framebuffer::Framebuffer,
        image::save_png,
        legacy::{BACKGROUND, render_model},
    },
};

fn run() -> Result<(), String> {
    let mut headless = false;
    let mut frame_limit = None;
    let mut output = PathBuf::from("renders/nave.png");
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--headless" => headless = true,
            "--output" => {
                output = PathBuf::from(args.next().ok_or("Falta la ruta despues de --output")?)
            }
            "--frames" => {
                let frames = args
                    .next()
                    .ok_or("Falta la cantidad despues de --frames")?
                    .parse::<usize>()
                    .map_err(|_| "--frames necesita un entero positivo")?;
                if frames == 0 {
                    return Err("--frames debe ser mayor que cero".into());
                }
                frame_limit = Some(frames);
            }
            "--help" | "-h" => {
                println!(
                    "cargo run --release -- [--headless] [--output archivo.png] [--frames N]\nA/D o flechas: rotar | R: restablecer | P: guardar PNG | Esc: salir"
                );
                return Ok(());
            }
            _ => return Err(format!("Argumento desconocido: {argument}. Usa --help.")),
        }
    }
    let model = load_model(&ship_model_path())?;
    let (width, height) = (1280, 960);
    let mut framebuffer = Framebuffer::new(width, height, BACKGROUND)?;
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
