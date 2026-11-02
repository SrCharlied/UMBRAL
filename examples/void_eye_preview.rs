//! Preview tecnico AISLADO de UMB-309 (`ShaderKind::VoidEye`, hipotesis «pupila del vacio»), sin
//! ventana y sin assets externos: la esfera del juego (`uv_sphere(32, 64)`, LOD 0) con
//! `draw_model` real. No es el ojo de la criatura, ni un planeta integrado al juego, ni una
//! identidad final. La nave no se dibuja: solo su posicion entra al shader. Ver
//! `docs/void-eye-preview.md`.
//!
//! `cargo run --release --example void_eye_preview -- --output-dir <carpeta-nueva>`
//! La carpeta no debe existir (nunca sobrescribe). Escribe 9 PNG 1280x720 y `manifest.json`.

use std::{fmt::Write as _, path::PathBuf};
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
/// Tiempo comun de las tomas y la segunda fase: medio periodo de los anillos (3 s) despues.
const TIME: f64 = 1.3;
const SECOND_PHASE: f64 = 2.8;

#[derive(Clone, Copy)]
struct View {
    name: &'static str,
    position: Vec3,
}

/// Acercamiento y distancia de juego (como el preview lucido).
const CLOSE: View = View {
    name: "cerca",
    position: Vec3::new(0.0, 0.35, 2.6),
};
const GAME: View = View {
    name: "juego",
    position: Vec3::new(0.0, 1.4, 10.0),
};

#[derive(Clone, Copy)]
struct Ship {
    name: &'static str,
    position: Vec3,
}

/// Naves de las tomas: frente a la camara (entre ella y la esfera, como en la persecucion) o a un
/// lado. La camara no se mueve entre izquierda y derecha.
const FRONT_CLOSE: Ship = Ship {
    name: "frente",
    position: Vec3::new(0.0, 0.25, 1.9),
};
const LEFT: Ship = Ship {
    name: "izquierda",
    position: Vec3::new(-1.6, 0.2, 1.2),
};
const RIGHT: Ship = Ship {
    name: "derecha",
    position: Vec3::new(1.6, 0.2, 1.2),
};
const FRONT_GAME: Ship = Ship {
    name: "frente",
    position: Vec3::new(0.0, 1.1, 7.5),
};

struct Shot {
    shader: ShaderKind,
    oxygen01: f64,
    elapsed_time: f64,
    view: View,
    ship: Ship,
}

impl Shot {
    fn file(&self) -> String {
        let shader = match self.shader {
            ShaderKind::LitPlanet => "base_lit".to_string(),
            ShaderKind::AnimatedSun => "comparar_animated_sun".to_string(),
            ShaderKind::LucidSurface => format!(
                "comparar_lucid_o2_{:03}",
                (self.oxygen01 * 100.0).round() as u32
            ),
            _ => format!("void_eye_nave_{}", self.ship.name),
        };
        format!(
            "{shader}_t{:04}ms_{}.png",
            (self.elapsed_time * 1000.0).round() as u32,
            self.view.name
        )
    }
}

fn shots() -> Vec<Shot> {
    let shot = |shader, oxygen01, elapsed_time, view, ship| Shot {
        shader,
        oxygen01,
        elapsed_time,
        view,
        ship,
    };
    let void = ShaderKind::VoidEye;
    vec![
        shot(ShaderKind::LitPlanet, 1.0, TIME, CLOSE, FRONT_CLOSE),
        shot(ShaderKind::AnimatedSun, 1.0, TIME, CLOSE, FRONT_CLOSE),
        shot(ShaderKind::LucidSurface, 0.05, TIME, CLOSE, FRONT_CLOSE),
        shot(void, 1.0, TIME, CLOSE, FRONT_CLOSE),
        shot(void, 1.0, TIME, CLOSE, LEFT),
        shot(void, 1.0, TIME, CLOSE, RIGHT),
        shot(void, 1.0, SECOND_PHASE, CLOSE, FRONT_CLOSE),
        shot(ShaderKind::LitPlanet, 1.0, TIME, GAME, FRONT_GAME),
        shot(void, 1.0, TIME, GAME, FRONT_GAME),
    ]
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
        ship_position: shot.ship.position,
        light_position: LIGHT,
    };
    let material = Material {
        cull_backfaces: true,
        ..Material::solid(STONE.0, STONE.1, STONE.2)
    };
    draw_model(
        &mut fb,
        &uv_sphere(RINGS, SEGMENTS),
        &Transform::IDENTITY,
        &material,
        shot.shader,
        &context,
        &camera,
    );
    Ok(fb)
}

fn luminance(c: u32) -> u32 {
    ((c >> 16) & 0xff) + ((c >> 8) & 0xff) + (c & 0xff)
}

/// Pixeles mucho mas oscuros que la base (menos de 1/4): la pupila. Devuelve cantidad y centro.
fn pupil(frame: &[u32], base: &[u32], width: usize) -> (usize, Option<(f64, f64)>) {
    let dark: Vec<usize> = (0..frame.len())
        .filter(|&i| base[i] != SPACE_COLOR && luminance(frame[i]) * 4 < luminance(base[i]))
        .collect();
    let center = (!dark.is_empty()).then(|| {
        let n = dark.len() as f64;
        (
            dark.iter().map(|&i| (i % width) as f64).sum::<f64>() / n,
            dark.iter().map(|&i| (i / width) as f64).sum::<f64>() / n,
        )
    });
    (dark.len(), center)
}

fn differing(a: &[u32], b: &[u32]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let output = match (args.next().as_deref(), args.next(), args.next()) {
        (Some("--output-dir"), Some(dir), None) => PathBuf::from(dir),
        _ => return Err("Uso: void_eye_preview --output-dir <carpeta-nueva>".into()),
    };
    // `create_dir` falla si la carpeta ya existe: nunca sobrescribe una salida previa.
    std::fs::create_dir(&output).map_err(|e| {
        format!(
            "No se pudo crear {} (debe ser nueva): {e}",
            output.display()
        )
    })?;

    // Primero todos los cuadros sin etiquetas: las comparaciones usan pixeles limpios.
    let shots = shots();
    let mut frames = Vec::new();
    for shot in &shots {
        let fb = render(shot)?;
        if fb.pixels[LABEL_ROW * fb.width..]
            .iter()
            .any(|&p| p != SPACE_COLOR)
        {
            return Err(format!(
                "{}: la esfera invade la franja de etiquetas",
                shot.file()
            ));
        }
        frames.push(fb);
    }
    let width = frames[0].width;
    let find = |pred: &dyn Fn(&Shot) -> bool| shots.iter().position(pred).unwrap();
    let base_of = |view: &str| find(&|s| s.shader == ShaderKind::LitPlanet && s.view.name == view);

    let mut entries = Vec::new();
    for (shot, fb) in shots.iter().zip(&frames) {
        let base = &frames[base_of(shot.view.name)].pixels;
        let disc = base.iter().filter(|&&p| p != SPACE_COLOR).count();
        let (dark, center) = if shot.shader == ShaderKind::VoidEye {
            pupil(&fb.pixels, base, width)
        } else {
            (0, None)
        };
        entries.push(manifest_entry(
            shot,
            disc,
            differing(&fb.pixels, base),
            dark,
            center,
        ));
    }
    // Distinciones comprobables entre tomas (pixeles sin etiquetas).
    let void_front = find(&|s| {
        s.shader == ShaderKind::VoidEye && s.ship.name == "frente" && s.view.name == "cerca"
    });
    let index = |name: &str| find(&|s| s.file().starts_with(name));
    let close_base = &frames[base_of("cerca")].pixels;
    let pupil_x = |i: usize| pupil(&frames[i].pixels, close_base, width).1.map(|c| c.0);
    let left = index("void_eye_nave_izquierda");
    let right = index("void_eye_nave_derecha");
    let second = find(&|s| s.elapsed_time == SECOND_PHASE);
    let mut checks = String::new();
    let mut check = |name: &str, value: String| {
        let _ = write!(
            checks,
            "{}    \"{name}\": {value}",
            if checks.is_empty() { "" } else { ",\n" }
        );
    };
    for (name, a, b) in [
        (
            "void_frente_vs_lit_pixeles_distintos",
            void_front,
            base_of("cerca"),
        ),
        (
            "void_frente_vs_animated_sun_pixeles_distintos",
            void_front,
            index("comparar_animated_sun"),
        ),
        (
            "void_frente_vs_lucid_o2_5_pixeles_distintos",
            void_front,
            index("comparar_lucid"),
        ),
        (
            "void_frente_t1300_vs_t2800_pixeles_distintos",
            void_front,
            second,
        ),
        ("void_izquierda_vs_derecha_pixeles_distintos", left, right),
    ] {
        check(
            name,
            differing(&frames[a].pixels, &frames[b].pixels).to_string(),
        );
    }
    let fmt = |x: Option<f64>| x.map_or("null".to_string(), |x| format!("{x:.1}"));
    check("pupila_x_nave_izquierda", fmt(pupil_x(left)));
    check("pupila_x_nave_frente", fmt(pupil_x(void_front)));
    check("pupila_x_nave_derecha", fmt(pupil_x(right)));
    check(
        "pupila_izquierda_a_la_izquierda_de_la_derecha",
        match (pupil_x(left), pupil_x(right)) {
            (Some(l), Some(r)) => (l < r).to_string(),
            _ => "false".to_string(),
        },
    );

    for (shot, fb) in shots.iter().zip(frames.iter_mut()) {
        let label = match shot.shader {
            ShaderKind::LitPlanet => "LITPLANET (BASE)".to_string(),
            ShaderKind::AnimatedSun => "ANIMATEDSUN (COMPARACION)".to_string(),
            ShaderKind::LucidSurface => format!(
                "LUCIDSURFACE O2 {:.0}% (COMPARACION)",
                shot.oxygen01 * 100.0
            ),
            _ => format!("VOIDEYE | NAVE {}", shot.ship.name.to_uppercase()),
        };
        let lines = [
            format!(
                "UMB-309 PREVIEW AISLADO | {label} | T {:.2} S | CAMARA {}",
                shot.elapsed_time,
                shot.view.name.to_uppercase()
            ),
            "HIPOTESIS PUPILA DEL VACIO: NO ES EL OJO DE LA CRIATURA NI UN PLANETA INTEGRADO"
                .to_string(),
        ];
        for (i, line) in lines.iter().enumerate() {
            draw_text(fb, 16, LABEL_ROW + 12 + i * 20, 2, 0xC8D2D2, line);
        }
        let file = shot.file();
        save_png(&output.join(&file), fb)?;
        println!("{file}");
    }

    let mut json = String::from("{\n");
    let _ = writeln!(
        json,
        "  \"preview\": \"UMB-309 VoidEye, hipotesis pupila del vacio\","
    );
    let _ = writeln!(
        json,
        "  \"status\": \"preview tecnico AISLADO, primera hipotesis; no es el ojo de la criatura, ni un planeta integrado al juego, ni una identidad final\","
    );
    let _ = writeln!(
        json,
        "  \"command\": \"cargo run --release --locked --offline --example void_eye_preview -- --output-dir <carpeta-nueva>\","
    );
    let _ = writeln!(json, "  \"png_count\": {},", entries.len());
    let _ = writeln!(
        json,
        "  \"resolution\": [{DEFAULT_WIDTH}, {DEFAULT_HEIGHT}],"
    );
    let _ = writeln!(
        json,
        "  \"geometry\": \"uv_sphere({RINGS}, {SEGMENTS}) radio 1 en el origen (LOD 0 del juego); la nave no se dibuja, solo su posicion entra al shader\","
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
        "  \"label_band\": \"filas >= {LABEL_ROW}; la esfera no las toca (comprobado); todas las comparaciones se hacen antes de etiquetar\","
    );
    let _ = writeln!(
        json,
        "  \"pupil_definition\": \"pixeles con suma RGB menor que 1/4 de la de la base LitPlanet de la misma camara\","
    );
    let _ = writeln!(json, "  \"checks\": {{\n{checks}\n  }},");
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

fn manifest_entry(
    shot: &Shot,
    disc: usize,
    differing_vs_base: usize,
    dark: usize,
    center: Option<(f64, f64)>,
) -> String {
    let (p, s) = (shot.view.position, shot.ship.position);
    format!(
        "    {{\"file\": \"{}\", \"shader\": \"{:?}\", \"oxygen01\": {}, \"elapsed_time\": {}, \
         \"camera\": {{\"name\": \"{}\", \"position\": [{}, {}, {}], \"target\": [0, 0, 0], \
         \"fov_y_degrees\": {FOV_DEGREES}}}, \"ship\": {{\"name\": \"{}\", \"position\": [{}, {}, {}]}}, \
         \"disc_pixels\": {disc}, \"differing_pixels_vs_lit_same_camera\": {differing_vs_base}, \
         \"pupil_pixels\": {dark}, \"pupil_center_px\": {}}}",
        shot.file(),
        shot.shader,
        shot.oxygen01,
        shot.elapsed_time,
        shot.view.name,
        p.x,
        p.y,
        p.z,
        shot.ship.name,
        s.x,
        s.y,
        s.z,
        center.map_or("null".to_string(), |(x, y)| format!("[{x:.1}, {y:.1}]"))
    )
}
