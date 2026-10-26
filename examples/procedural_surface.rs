//! Vista tecnica de UMB-304, sin ventana: esfera unidad gris y un marcador esferico por
//! muestra, coloreado por densidad (cian = umbral, naranja = 1). No es arte final, ni props, ni un
//! planeta aprobado. Ver `docs/procedural.md`.
//!
//! `cargo run --release --example procedural_surface -- --seed 24531`
//! Opciones: `--seed N --attempts N --noise-scale X --threshold X --min-spacing X --output archivo.png`.

use std::path::PathBuf;
use umbral::{
    assets::primitives::uv_sphere,
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    debug::overlay::draw_text,
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer,
        image::save_png,
        pipeline::draw_model,
        shader::{Material, ShaderContext, ShaderKind},
        stats::RenderStats,
    },
    scene::{camera::Camera, world::SPACE_COLOR},
    visual::procedural_surface::{DistributionSettings, distribute},
};

/// Radio de cada marcador relativo al planeta. El centro se eleva un radio sobre la
/// superficie, asi que el marcador entero sobresale (no hay orientacion de UMB-305).
const MARKER_RADIUS: f64 = 0.025;

fn value<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<T, String> {
    args.next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("{flag} necesita un valor valido"))
}

fn main() -> Result<(), String> {
    let mut settings = DistributionSettings {
        seed: 24531,
        attempts: 2048,
        noise_scale: 2.5,
        threshold: 0.55,
        min_spacing: 0.08,
    };
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" => settings.seed = value(&mut args, &flag)?,
            "--attempts" => settings.attempts = value(&mut args, &flag)?,
            "--noise-scale" => settings.noise_scale = value(&mut args, &flag)?,
            "--threshold" => settings.threshold = value(&mut args, &flag)?,
            "--min-spacing" => settings.min_spacing = value(&mut args, &flag)?,
            "--output" => output = Some(PathBuf::from(value::<String>(&mut args, &flag)?)),
            _ => return Err(format!("Argumento desconocido: {flag}")),
        }
    }
    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "renders/smoke/procedural_seed_{}.png",
            settings.seed
        ))
    });
    let samples = distribute(&settings)?;

    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut camera = Camera::new(40f64.to_radians(), width as f64 / height as f64, 0.1, 100.0)?;
    camera.position = Vec3::new(0.0, 1.6, 4.1);
    camera.target = Vec3::ZERO;
    let context = ShaderContext {
        elapsed_time: 0.0,
        oxygen01: 1.0,
        camera_position: camera.position,
        ship_position: camera.position,
        light_position: Vec3::new(4.0, 5.0, 6.0),
    };
    let planet = Material {
        cull_backfaces: true,
        ..Material::solid(0.22, 0.23, 0.26)
    };
    draw_model(
        &mut fb,
        &uv_sphere(48, 96),
        &Transform::IDENTITY,
        &planet,
        ShaderKind::LitPlanet,
        &context,
        &camera,
    );

    let marker = uv_sphere(6, 12);
    let low = Vec3::new(0.1, 0.85, 0.9);
    let high = Vec3::new(1.0, 0.45, 0.05);
    let mut markers = RenderStats::default();
    for s in &samples {
        let t = ((s.density - settings.threshold) / (1.0 - settings.threshold)).clamp(0.0, 1.0);
        let material = Material {
            base_color: low.lerp(high, t),
            ambient: 0.35,
            ..Material::solid(0.0, 0.0, 0.0)
        };
        let transform = Transform {
            position: s.normal * (1.0 + MARKER_RADIUS),
            scale: Vec3::splat(MARKER_RADIUS),
            ..Transform::IDENTITY
        };
        markers += draw_model(
            &mut fb,
            &marker,
            &transform,
            &material,
            ShaderKind::LitPlanet,
            &context,
            &camera,
        );
    }

    let lines = [
        "UMB-304 VISTA TECNICA - NO ES ARTE FINAL".to_string(),
        format!(
            "SEMILLA {} | INTENTOS {} | ACEPTADAS {}",
            settings.seed,
            settings.attempts,
            samples.len()
        ),
        format!(
            "ESCALA {} | UMBRAL {} | ESPACIADO {}",
            settings.noise_scale, settings.threshold, settings.min_spacing
        ),
    ];
    for (i, line) in lines.iter().enumerate() {
        draw_text(&mut fb, 16, 16 + i * 22, 2, 0xC8D2D2, line);
    }
    save_png(&output, &fb)?;

    let densities = samples.iter().map(|s| s.density);
    let min_chord = samples
        .iter()
        .enumerate()
        .flat_map(|(i, a)| {
            samples[i + 1..]
                .iter()
                .map(move |b| (a.position - b.position).length())
        })
        .fold(f64::INFINITY, f64::min);
    let toward_camera = samples
        .iter()
        .filter(|s| s.normal.dot(camera.position.normalized()) > 0.0)
        .count();
    println!("{settings:?}");
    println!(
        "Muestras: {} de {} | densidad {:.3}..{:.3} | cuerda minima {:.4} | norte {} sur {} | hacia la camara {}",
        samples.len(),
        settings.attempts,
        densities.clone().fold(f64::INFINITY, f64::min),
        densities.fold(f64::NEG_INFINITY, f64::max),
        min_chord,
        samples.iter().filter(|s| s.position.y > 0.0).count(),
        samples.iter().filter(|s| s.position.y <= 0.0).count(),
        toward_camera
    );
    println!(
        "Marcadores: pixeles escritos {} | fragmentos {}",
        markers.pixels_written, markers.fragments_evaluated
    );
    println!("PNG: {} ({width}x{height})", output.display());
    Ok(())
}
