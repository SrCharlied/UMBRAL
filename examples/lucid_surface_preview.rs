//! Preview tecnico/artistico CANDIDATO de UMB-308 (superficie lucida), sin ventana y sin assets
//! externos: la esfera del juego (`uv_sphere(32, 64)`, LOD 0) con `draw_model` real. No es el
//! planeta final Antena ni un paso integrado al juego. Ver `docs/lucid-surface-preview.md`.
//!
//! `cargo run --release --example lucid_surface_preview -- --output-dir <carpeta-nueva>`
//! La carpeta no debe existir (nunca sobrescribe). Escribe 14 PNG 1280x720 y `manifest.json`.

use std::{f64::consts::PI, fmt::Write as _, path::PathBuf};
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
    },
    scene::{camera::Camera, world::SPACE_COLOR},
};

const FOV_DEGREES: f64 = 60.0;
const LIGHT: Vec3 = Vec3::new(5.0, 2.0, 1.0);
const STONE: (f64, f64, f64) = (0.22, 0.23, 0.26);
const RINGS: usize = 32;
const SEGMENTS: usize = 64;
/// Las etiquetas van por debajo de esta fila; la esfera nunca llega ahi (se comprueba).
const LABEL_ROW: usize = 664;
/// Tiempo comun de las tomas; 0 y 2.25 s son el maximo y el minimo de la pulsacion (4.5 s).
const TIME: f64 = 1.3;

#[derive(Clone, Copy)]
struct View {
    name: &'static str,
    position: Vec3,
}

/// Acercamiento y distancia de juego (radio aparente ~0.1, como un planeta a media distancia).
const CLOSE: View = View {
    name: "cerca",
    position: Vec3::new(0.0, 0.35, 2.6),
};
const GAME: View = View {
    name: "juego",
    position: Vec3::new(0.0, 1.4, 10.0),
};

struct Shot {
    shader: ShaderKind,
    oxygen01: f64,
    elapsed_time: f64,
    view: View,
    yaw: f64,
}

impl Shot {
    fn file(&self) -> String {
        let shader = match self.shader {
            ShaderKind::LitPlanet => "baseline_lit".to_string(),
            _ => format!("lucid_o2_{:03}", (self.oxygen01 * 100.0).round() as u32),
        };
        format!(
            "{shader}_t{:04}ms_yaw{:+04}deg_{}.png",
            (self.elapsed_time * 1000.0).round() as u32,
            self.yaw.to_degrees().round() as i32,
            self.view.name
        )
    }
}

fn shots() -> Vec<Shot> {
    let shot = |shader, oxygen01, elapsed_time, view, yaw| Shot {
        shader,
        oxygen01,
        elapsed_time,
        view,
        yaw,
    };
    let (lit, lucid) = (ShaderKind::LitPlanet, ShaderKind::LucidSurface);
    let mut list = vec![shot(lit, 1.0, TIME, CLOSE, 0.0)];
    for oxygen in [1.0, 0.5, 0.4, 0.25, 0.05] {
        list.push(shot(lucid, oxygen, TIME, CLOSE, 0.0));
    }
    for time in [0.0, 2.25] {
        for oxygen in [0.25, 0.05] {
            list.push(shot(lucid, oxygen, time, CLOSE, 0.0));
        }
    }
    // Rotacion: las grietas viajan con la superficie (mismo tiempo, planeta girado 35 grados).
    list.push(shot(lucid, 0.05, TIME, CLOSE, 35.0 * PI / 180.0));
    list.push(shot(lit, 1.0, TIME, GAME, 0.0));
    list.push(shot(lucid, 0.25, TIME, GAME, 0.0));
    list.push(shot(lucid, 0.05, TIME, GAME, 0.0));
    list
}

fn render(shot: &Shot) -> Result<Framebuffer, String> {
    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut camera = Camera::new(
        FOV_DEGREES.to_radians(),
        width as f64 / height as f64,
        0.1,
        100.0,
    )?;
    camera.position = shot.view.position;
    camera.target = Vec3::ZERO;
    let context = ShaderContext {
        elapsed_time: shot.elapsed_time,
        oxygen01: shot.oxygen01,
        camera_position: camera.position,
        ship_position: camera.position,
        light_position: LIGHT,
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    let transform = Transform {
        rotation: Vec3::new(0.0, shot.yaw, 0.0),
        ..Transform::IDENTITY
    };
    draw_model(
        &mut fb,
        &uv_sphere(RINGS, SEGMENTS),
        &transform,
        &material,
        shot.shader,
        &context,
        &camera,
    );
    Ok(fb)
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let output = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--output-dir"), Some(dir), None) => PathBuf::from(dir),
        _ => return Err("Uso: lucid_surface_preview --output-dir <carpeta-nueva>".into()),
    };
    // `create_dir` falla si la carpeta ya existe: nunca sobrescribe una salida previa.
    std::fs::create_dir(&output).map_err(|e| {
        format!(
            "No se pudo crear {} (debe ser nueva): {e}",
            output.display()
        )
    })?;

    let shots = shots();
    let mut baselines: Vec<(&'static str, Vec<u32>)> = Vec::new();
    let mut entries = Vec::new();
    for shot in &shots {
        let mut fb = render(shot)?;
        let first_label_pixel = LABEL_ROW * fb.width;
        if fb.pixels[first_label_pixel..]
            .iter()
            .any(|&p| p != SPACE_COLOR)
        {
            return Err(format!(
                "{}: la esfera invade la franja de etiquetas",
                shot.file()
            ));
        }
        // Comparacion exacta antes de etiquetar: misma camara que la toma base.
        if shot.shader == ShaderKind::LitPlanet {
            baselines.push((shot.view.name, fb.pixels.clone()));
        }
        let baseline = baselines
            .iter()
            .find(|(name, _)| *name == shot.view.name)
            .filter(|_| shot.yaw == 0.0)
            .map(|(_, pixels)| pixels);
        let differing = baseline.map(|b| b.iter().zip(&fb.pixels).filter(|(a, c)| a != c).count());
        let disc = fb.pixels[..first_label_pixel]
            .iter()
            .filter(|&&p| p != SPACE_COLOR)
            .count();

        let label = match shot.shader {
            ShaderKind::LitPlanet => "LITPLANET (BASE)".to_string(),
            _ => format!("LUCIDSURFACE O2 {:.0}%", shot.oxygen01 * 100.0),
        };
        let lines = [
            format!(
                "UMB-308 CANDIDATA - NO APROBADA | {label} | T {:.2} S | GIRO {:.0} | CAMARA {}",
                shot.elapsed_time,
                shot.yaw.to_degrees(),
                shot.view.name.to_uppercase()
            ),
            "PREVIEW TECNICO: NO ES EL PLANETA FINAL NI ESTA INTEGRADO AL JUEGO".to_string(),
        ];
        for (i, line) in lines.iter().enumerate() {
            draw_text(&mut fb, 16, LABEL_ROW + 12 + i * 20, 2, 0xC8D2D2, line);
        }
        let file = shot.file();
        save_png(&output.join(&file), &fb)?;
        println!(
            "{file}: disco {disc} px, distintos a la base {}",
            differing.map_or("-".to_string(), |n| n.to_string())
        );
        entries.push(manifest_entry(shot, &file, disc, differing));
    }

    let mut json = String::from("{\n");
    let _ = writeln!(json, "  \"preview\": \"UMB-308 superficie lucida\",");
    let _ = writeln!(
        json,
        "  \"status\": \"preview tecnico/artistico CANDIDATO; no es el planeta final Antena ni un paso integrado al juego\","
    );
    let _ = writeln!(
        json,
        "  \"command\": \"cargo run --release --locked --offline --example lucid_surface_preview -- --output-dir <carpeta-nueva>\","
    );
    let _ = writeln!(json, "  \"png_count\": {},", entries.len());
    let _ = writeln!(
        json,
        "  \"resolution\": [{DEFAULT_WIDTH}, {DEFAULT_HEIGHT}],"
    );
    let _ = writeln!(
        json,
        "  \"geometry\": \"uv_sphere({RINGS}, {SEGMENTS}) radio 1 en el origen (LOD 0 del juego)\","
    );
    let _ = writeln!(
        json,
        "  \"material\": {{\"base_color\": [{}, {}, {}], \"ambient\": 0.04, \"cull_backfaces\": true}},",
        STONE.0, STONE.1, STONE.2
    );
    let _ = writeln!(
        json,
        "  \"light_position\": [{}, {}, {}],",
        LIGHT.x, LIGHT.y, LIGHT.z
    );
    let _ = writeln!(
        json,
        "  \"label_band\": \"filas >= {LABEL_ROW}; la esfera no las toca (comprobado antes de etiquetar)\","
    );
    let _ = writeln!(
        json,
        "  \"differing_pixels_vs_baseline\": \"contados antes de etiquetar frente a la toma LitPlanet de la misma camara y giro 0\","
    );
    let _ = writeln!(json, "  \"images\": [\n{}\n  ]", entries.join(",\n"));
    json.push_str("}\n");
    std::fs::write(output.join("manifest.json"), json).map_err(|e| e.to_string())?;
    println!(
        "{} PNG y manifest.json en {}",
        entries.len(),
        output.display()
    );
    Ok(())
}

fn manifest_entry(shot: &Shot, file: &str, disc: usize, differing: Option<usize>) -> String {
    let p = shot.view.position;
    format!(
        "    {{\"file\": \"{file}\", \"shader\": \"{:?}\", \"oxygen01\": {}, \"elapsed_time\": {}, \
         \"yaw_degrees\": {}, \"camera\": {{\"name\": \"{}\", \"position\": [{}, {}, {}], \
         \"target\": [0, 0, 0], \"fov_y_degrees\": {FOV_DEGREES}}}, \"disc_pixels\": {disc}, \
         \"differing_pixels_vs_baseline\": {}}}",
        shot.shader,
        shot.oxygen01,
        shot.elapsed_time,
        shot.yaw.to_degrees().round(),
        shot.view.name,
        p.x,
        p.y,
        p.z,
        differing.map_or("null".to_string(), |n| n.to_string())
    )
}
