//! Preview tecnico sin ventana de un OBJ con su textura PNG (RGB8), dibujado por el mismo
//! pipeline del juego (`draw_model_with`) con el shader texturizado. No integra nada en la
//! escena. Ver `docs/textured-preview.md`.
//!
//! cargo run --release --example creature_preview -- --model modelo.obj --texture textura.png
//!     --output salida.png [--unlit] [--yaw RAD] [--benchmark]

use std::{path::PathBuf, time::Instant};
use umbral::{
    assets::obj::load_model,
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    debug::overlay::draw_text,
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer, image::save_png, pipeline::draw_model_with, shader::Material,
        stats::RenderStats, texture::Texture,
    },
    scene::{camera::Camera, world::SPACE_COLOR},
    visual::textured::{TexturedMode, TexturedShader},
};

const FOV_DEGREES: f64 = 40.0;
/// Margen sobre la esfera envolvente al encuadrar.
const FRAME_MARGIN: f64 = 1.1;
const AMBIENT: f64 = 0.25;
const WARMUPS: usize = 2;
const SAMPLES: usize = 11;

fn preview_transform(center: Vec3, yaw: f64) -> Transform {
    let mut transform = Transform {
        rotation: Vec3::new(0.0, yaw, 0.0),
        ..Transform::IDENTITY
    };
    // Compensa el giro alrededor del origen sin cambiar el transform original a yaw 0.
    if yaw != 0.0 {
        transform.position = center - transform.matrix().transform_point(center);
    }
    transform
}

fn path_arg(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} necesita una ruta"))
}

fn main() -> Result<(), String> {
    let (mut model_path, mut texture_path, mut output) = (None, None, None);
    let (mut mode, mut yaw, mut benchmark) = (TexturedMode::Lit, 0.0_f64, false);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--model" => model_path = Some(path_arg(&mut args, &flag)?),
            "--texture" => texture_path = Some(path_arg(&mut args, &flag)?),
            "--output" => output = Some(path_arg(&mut args, &flag)?),
            "--unlit" => mode = TexturedMode::Unlit,
            "--yaw" => {
                yaw = args
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .ok_or("--yaw necesita radianes finitos")?
            }
            "--benchmark" => benchmark = true,
            _ => return Err(format!("Argumento desconocido: {flag}")),
        }
    }
    let model_path = model_path.ok_or("Falta --model archivo.obj")?;
    let texture_path = texture_path.ok_or("Falta --texture archivo.png")?;
    let output = output.ok_or("Falta --output archivo.png")?;

    let start = Instant::now();
    let model = load_model(&model_path)?;
    let load_model_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let texture = Texture::load_png(&texture_path)?;
    let load_texture_ms = start.elapsed().as_secs_f64() * 1000.0;

    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let fov = FOV_DEGREES.to_radians();
    let center = Vec3::from_array(model.bounds_center);
    let radius = model.bounds_radius;
    if !(radius.is_finite() && radius > 0.0) {
        return Err(format!("radio envolvente invalido: {radius}"));
    }
    // La esfera envolvente cabe en el FOV vertical (el limitante a 16:9) con margen.
    let distance = radius * FRAME_MARGIN / (fov / 2.0).sin();
    let mut camera = Camera::new(
        fov,
        width as f64 / height as f64,
        distance * 0.01,
        distance * 4.0,
    )?;
    camera.position = center + Vec3::new(0.0, 0.15, 1.0).normalized() * distance;
    camera.target = center;
    let light = center + Vec3::new(-1.5, 2.0, 3.0) * radius;
    let shader = TexturedShader::new(&texture, mode, light, AMBIENT)?;
    let transform = preview_transform(center, yaw);
    // El shader ignora el Kd; from_model no descarta caras de espaldas (malla no cerrada).
    let material = Material::from_model();
    let render = |fb: &mut Framebuffer| -> RenderStats {
        fb.clear();
        draw_model_with(fb, &model, &transform, &material, &camera, |f, _| {
            shader.shade(f)
        })
    };

    let mut times = Vec::new();
    let runs = if benchmark { WARMUPS + SAMPLES } else { 1 };
    let mut stats = RenderStats::default();
    for run in 0..runs {
        let start = Instant::now();
        stats = render(&mut fb);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if !benchmark || run >= WARMUPS {
            times.push(ms);
        }
    }
    let label = format!(
        "VISTA TECNICA - MODELO ORIGINAL CON TEXTURA | {} | YAW {yaw:.2}",
        if mode == TexturedMode::Lit {
            "LIT"
        } else {
            "UNLIT"
        }
    );
    draw_text(&mut fb, 16, 16, 2, 0xC8D2D2, &label);
    save_png(&output, &fb)?;

    println!(
        "Modelo: {} v, {} vt, {} vn, {} triangulos | carga {load_model_ms:.1} ms",
        model.vertices.len(),
        model.uvs.len(),
        model.normals.len(),
        model.faces.len()
    );
    println!(
        "Textura: {}x{} RGB8 ({:.1} MiB) | carga {load_texture_ms:.0} ms",
        texture.width(),
        texture.height(),
        (texture.width() as usize * texture.height() as usize * 3) as f64 / 1048576.0
    );
    println!(
        "Render {width}x{height} {mode:?} yaw {yaw}: triangulos rasterizados {} descartados {} | fragmentos {} | pixeles {}",
        stats.triangles_rasterized,
        stats.triangles_culled,
        stats.fragments_evaluated,
        stats.pixels_written
    );
    if benchmark {
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        println!(
            "Benchmark ({WARMUPS} calentamientos, {SAMPLES} mediciones): mediana {:.1} ms | min {:.1} | max {:.1} | ordenadas {:.1?}",
            sorted[SAMPLES / 2],
            sorted[0],
            sorted[SAMPLES - 1],
            sorted
        );
    } else {
        println!("Render de un cuadro: {:.1} ms", times[0]);
    }
    println!("PNG: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_preserves_displaced_bounds_center_and_radius() {
        let center = Vec3::new(100.0, 0.0, 0.0);
        let matrix = preview_transform(center, 1.0).matrix();
        let world_center = matrix.transform_point(center);
        assert!(
            (world_center - center).length() < 1e-12,
            "el centro envolvente se desplazo a {world_center:?}"
        );
        for vertex in [
            Vec3::new(99.0, -1.0, 0.0),
            Vec3::new(101.0, -1.0, 0.0),
            Vec3::new(101.0, 1.0, 0.0),
            Vec3::new(99.0, 1.0, 0.0),
        ] {
            let radius = (vertex - center).length();
            let world_radius = (matrix.transform_point(vertex) - center).length();
            assert!((world_radius - radius).abs() < 1e-12);
        }
    }

    #[test]
    fn zero_yaw_preserves_original_transform_exactly() {
        let original = Transform {
            rotation: Vec3::new(0.0, 0.0, 0.0),
            ..Transform::IDENTITY
        };
        let transform = preview_transform(Vec3::new(100.0, -25.0, 7.0), 0.0);
        assert_eq!(transform, original);
        assert_eq!(transform.matrix(), original.matrix());
    }
}
