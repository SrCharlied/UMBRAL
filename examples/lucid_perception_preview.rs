//! Preview AISLADO de UMB-310 (percepcion alterada por oxigeno, primera hipotesis): postproceso
//! sobre cuadros REALES del juego (`World`, camara, luz, nave versionada y el arranque `--near`),
//! sin ventana, sin HUD y sin integrarse a App/World/CLI. Ver `docs/lucid-perception-preview.md`.
//!
//! `cargo run --release --locked --offline --example lucid_perception_preview -- --output-dir <carpeta-nueva> [--time <s>]`
//!
//! Dos encuadres (junto a Antena y junto a Vigilia, con la superficie lucida y el ojo del vacio
//! aprobados activos). Cada escena se carga y se dibuja UNA vez, con tiempo de mundo, O2 de mundo,
//! camara, luz y composicion fijos; solo varian el O2 y el tiempo del postproceso. La carpeta no
//! debe existir; los argumentos y el tiempo se validan antes de escribir nada.

#[path = "../src/visual/lucid_perception.rs"]
mod lucid_perception;

use lucid_perception::{
    CLEAR_RADIUS, DEFORM_LOBES, DEFORM_MAX_FRACTION, DEFORM_ONSET_OXYGEN, DEFORM_PERIOD_SECONDS,
    PERCEPTION_FULL_OXYGEN, PERCEPTION_ONSET_OXYGEN, VIGNETTE_MAX, lucid_perception,
};
use std::{fmt::Write as _, path::PathBuf};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    math::Vec3,
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::world::{SPACE_COLOR, World},
    visual::{lucid_game::set_lucid_surface, void_game::set_void_eye},
};

/// Tiempo de mundo por defecto (el de las capturas del juego con `--near`).
const DEFAULT_WORLD_TIME: f64 = 1.3;
/// O2 del MUNDO, fijo en todas las tomas: sin grietas lucidas, para aislar el postproceso.
const WORLD_OXYGEN: f64 = 1.0;
/// Distancia a la superficie del arranque `--near` del juego.
const SURFACE_GAP: f64 = 25.0;
/// Niveles de O2 del POSTPROCESO.
const POST_OXYGEN: [f64; 4] = [1.0, 0.5, 0.25, 0.05];
/// Secuencia corta (animacion secundaria): una vuelta completa de la onda con O2 critico.
const SEQUENCE_OXYGEN: f64 = 0.05;
const SEQUENCE_FRAMES: usize = 24;
/// Cajas de esquina para medir el oscurecimiento.
const CORNER_BOX: usize = 96;

struct Scene {
    name: &'static str,
    body: usize,
    body_name: &'static str,
    camera: (Vec3, Vec3),
    ship: Vec3,
    ship_px: Option<(f64, f64)>,
    frame: Framebuffer,
}

fn usage() -> String {
    "Uso: lucid_perception_preview --output-dir <carpeta-nueva> [--time <segundos >= 0>]".into()
}

fn parse_args() -> Result<(PathBuf, f64), String> {
    let (mut output, mut time) = (None, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(usage)?;
        match flag.as_str() {
            "--output-dir" if output.is_none() => output = Some(PathBuf::from(value)),
            "--time" if time.is_none() => {
                time = Some(
                    value
                        .parse::<f64>()
                        .map_err(|_| format!("--time no es un numero: {value}"))?,
                )
            }
            _ => return Err(usage()),
        }
    }
    let output = output.ok_or_else(usage)?;
    let time = time.unwrap_or(DEFAULT_WORLD_TIME);
    if !(time.is_finite() && time >= 0.0) {
        return Err(format!("--time necesita segundos >= 0: {time}"));
    }
    Ok((output, time))
}

/// Tiempos del postproceso: la fase A coincide con el tiempo del mundo; la B, medio periodo
/// despues (la onda queda invertida).
fn post_times(world_time: f64) -> [f64; 2] {
    [world_time, world_time + DEFORM_PERIOD_SECONDS / 2.0]
}

fn sequence_time(world_time: f64, frame: usize) -> f64 {
    world_time + DEFORM_PERIOD_SECONDS * frame as f64 / SEQUENCE_FRAMES as f64
}

/// Carga y dibuja una escena una sola vez. El modelo de la nave se lee aqui, una vez por
/// encuadre (como el juego al arrancar); los cuadros del postproceso no reabren nada.
fn render_scene(name: &'static str, near_void: bool, time: f64) -> Result<Scene, String> {
    let ship_model = load_model(&ship_model_path())?;
    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut world = World::new(ship_model, width as f64 / height as f64)?;
    world.time = time;
    world.debug_oxygen01 = WORLD_OXYGEN;
    // Identidades aprobadas del juego (setters existentes); material y escena intactos.
    let lucid = set_lucid_surface(&mut world, true)?;
    let void = set_void_eye(&mut world, true)?;
    let body = if near_void { void } else { lucid };
    world.place_near(body, SURFACE_GAP)?;
    let mut frame = Framebuffer::new(width, height, SPACE_COLOR)?;
    world.render(&mut frame);
    let clip = world
        .camera
        .view_projection()
        .transform(world.ship.position.extend(1.0));
    let ship_px = (clip.w > 0.0).then(|| {
        (
            (clip.x / clip.w * 0.5 + 0.5) * width as f64,
            (0.5 - clip.y / clip.w * 0.5) * height as f64,
        )
    });
    Ok(Scene {
        name,
        body,
        body_name: world.system.bodies[body].name,
        camera: (world.camera.position, world.camera.target),
        ship: world.ship.position,
        ship_px,
        frame,
    })
}

fn clear_radius_at(width: usize, height: usize, x: f64, y: f64) -> f64 {
    let (hw, hh) = (width as f64 / 2.0, height as f64 / 2.0);
    let (dx, dy) = ((x - hw) / hw, (y - hh) / hh);
    ((dx * dx + dy * dy) / 2.0).sqrt()
}

fn luminance(p: u32) -> u64 {
    ((p >> 16) & 0xff) as u64 + ((p >> 8) & 0xff) as u64 + (p & 0xff) as u64
}

struct Metrics {
    differing: usize,
    clear_area_changed: usize,
    corner_luminance_ratio: f64,
}

fn metrics(base: &Framebuffer, pixels: &[u32]) -> Metrics {
    let (w, h) = (base.width, base.height);
    let mut m = Metrics {
        differing: 0,
        clear_area_changed: 0,
        corner_luminance_ratio: 1.0,
    };
    let (mut corner_base, mut corner_post) = (0u64, 0u64);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if pixels[i] != base.pixels[i] {
                m.differing += 1;
                if clear_radius_at(w, h, x as f64 + 0.5, y as f64 + 0.5) < CLEAR_RADIUS {
                    m.clear_area_changed += 1;
                }
            }
            let corner =
                (x < CORNER_BOX || x >= w - CORNER_BOX) && (y < CORNER_BOX || y >= h - CORNER_BOX);
            if corner {
                corner_base += luminance(base.pixels[i]);
                corner_post += luminance(pixels[i]);
            }
        }
    }
    if corner_base > 0 {
        m.corner_luminance_ratio = corner_post as f64 / corner_base as f64;
    }
    m
}

fn save(
    path: &std::path::Path,
    width: usize,
    height: usize,
    pixels: Vec<u32>,
) -> Result<(), String> {
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    fb.pixels = pixels;
    save_png(path, &fb)
}

fn main() -> Result<(), String> {
    let (output, world_time) = parse_args()?;
    // Todo lo que podria rechazar el postproceso se comprueba antes de crear la carpeta.
    let probe = Framebuffer::new(1, 1, 0)?;
    for oxygen in POST_OXYGEN.into_iter().chain([SEQUENCE_OXYGEN]) {
        for time in post_times(world_time)
            .into_iter()
            .chain((0..SEQUENCE_FRAMES).map(|k| sequence_time(world_time, k)))
        {
            lucid_perception(&probe, oxygen, time)?;
        }
    }
    // `create_dir` falla si ya existe (carpeta, archivo o un input): nunca sobrescribe ni
    // escribe dentro de una ruta previa.
    std::fs::create_dir(&output).map_err(|e| {
        format!(
            "No se pudo crear {} (debe ser nueva): {e}",
            output.display()
        )
    })?;
    let sequence_dir = output.join("sequence");
    std::fs::create_dir(&sequence_dir).map_err(|e| e.to_string())?;

    let scenes = [
        render_scene("antena", false, world_time)?,
        render_scene("vigilia", true, world_time)?,
    ];
    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut images = Vec::new();
    let mut sequence = Vec::new();
    for scene in &scenes {
        let base = &scene.frame;
        let file = format!("{}_baseline_sin_postproceso.png", scene.name);
        save_png(&output.join(&file), base)?;
        images.push(format!(
            "    {{\"file\": \"{file}\", \"scene\": \"{}\", \"kind\": \"baseline\", \"post_oxygen01\": null, \"post_time\": null, \"differing_vs_baseline\": 0, \"clear_area_changed\": 0, \"corner_luminance_ratio\": 1.0}}",
            scene.name
        ));
        println!("{file}");
        for (phase, time) in ["a", "b"].into_iter().zip(post_times(world_time)) {
            for oxygen in POST_OXYGEN {
                let pixels = lucid_perception(base, oxygen, time)?;
                let m = metrics(base, &pixels);
                if m.clear_area_changed != 0 {
                    return Err(format!("{}: el centro protegido cambio", scene.name));
                }
                let file = format!(
                    "{}_post_o2_{:03}_fase_{phase}.png",
                    scene.name,
                    (oxygen * 100.0).round() as u32
                );
                save(&output.join(&file), width, height, pixels)?;
                images.push(format!(
                    "    {{\"file\": \"{file}\", \"scene\": \"{}\", \"kind\": \"candidate\", \"post_oxygen01\": {oxygen}, \"post_time\": {time}, \"phase\": \"{phase}\", \"differing_vs_baseline\": {}, \"clear_area_changed\": {}, \"corner_luminance_ratio\": {:.4}}}",
                    scene.name, m.differing, m.clear_area_changed, m.corner_luminance_ratio
                ));
                println!("{file}");
            }
        }
        for k in 0..SEQUENCE_FRAMES {
            let time = sequence_time(world_time, k);
            let pixels = lucid_perception(base, SEQUENCE_OXYGEN, time)?;
            let file = format!(
                "{}_o2_{:03}_f{k:02}.png",
                scene.name,
                (SEQUENCE_OXYGEN * 100.0).round() as u32
            );
            save(&sequence_dir.join(&file), width, height, pixels)?;
            sequence.push(format!(
                "    {{\"file\": \"sequence/{file}\", \"scene\": \"{}\", \"post_oxygen01\": {SEQUENCE_OXYGEN}, \"post_time\": {time}}}",
                scene.name
            ));
        }
    }

    let mut json = String::from("{\n");
    let mut line = |s: String| {
        let _ = writeln!(json, "  {s}");
    };
    line("\"preview\": \"UMB-310 percepcion alterada por oxigeno, primera hipotesis (preview AISLADO)\",".into());
    line("\"status\": \"hipotesis pendiente de decision artistica de Charlie; no integrada a App/World/CLI; sin estrellas observadoras ni umbral para ellas\",".into());
    line("\"command\": \"cargo run --release --locked --offline --example lucid_perception_preview -- --output-dir <carpeta-nueva> [--time <s>]\",".into());
    line(format!("\"resolution\": [{width}, {height}],"));
    line("\"labels_in_images\": false,".into());
    line(format!(
        "\"world\": {{\"time_fixed\": {world_time}, \"oxygen01_fixed\": {WORLD_OXYGEN}, \"note\": \"tiempo y O2 del MUNDO fijos en todas las tomas de una escena: grietas lucidas (ausentes con O2 1), ojo del vacio, orbitas, camara, luz y nave no cambian; solo cambian O2 y tiempo del POSTPROCESO\", \"approved_setters\": [\"lucid_game::set_lucid_surface (Antena)\", \"void_game::set_void_eye (Vigilia)\"], \"ship_model\": \"assets/models/ship/Umbral_v4_Desgastada.obj (versionado)\", \"private_assets\": false, \"creature\": false, \"loads_per_scene\": 1, \"renders_per_scene\": 1}},"
    ));
    line(format!(
        "\"post_settings\": {{\"perception_onset_oxygen\": {PERCEPTION_ONSET_OXYGEN}, \"perception_full_oxygen\": {PERCEPTION_FULL_OXYGEN}, \"deform_onset_oxygen\": {DEFORM_ONSET_OXYGEN}, \"clear_radius\": {CLEAR_RADIUS}, \"vignette_max\": {VIGNETTE_MAX}, \"deform_max_fraction_of_short_side\": {DEFORM_MAX_FRACTION}, \"deform_max_px\": {:.2}, \"deform_lobes\": {DEFORM_LOBES}, \"deform_period_seconds\": {DEFORM_PERIOD_SECONDS}, \"status\": \"hipotesis inicial, no valores aprobados\"}},",
        DEFORM_MAX_FRACTION * width.min(height) as f64
    ));
    line(format!(
        "\"post_times\": {{\"a\": {}, \"b\": {}, \"note\": \"b = a + medio periodo de la onda\"}},",
        post_times(world_time)[0],
        post_times(world_time)[1]
    ));
    let scenes_json: Vec<String> = scenes
        .iter()
        .map(|s| {
            let ship_px = s.ship_px.map_or("null".to_string(), |(x, y)| {
                format!(
                    "{{\"x\": {x:.1}, \"y\": {y:.1}, \"inside_clear_area\": {}}}",
                    clear_radius_at(width, height, x, y) < CLEAR_RADIUS
                )
            });
            format!(
                "    {{\"name\": \"{}\", \"body\": {}, \"body_name\": \"{}\", \"start\": \"World::place_near(body, {SURFACE_GAP}) como --near\", \"camera_position\": [{:.4}, {:.4}, {:.4}], \"camera_target\": [{:.4}, {:.4}, {:.4}], \"ship_position\": [{:.4}, {:.4}, {:.4}], \"ship_center_px\": {ship_px}, \"baseline_equivalent_game_command\": \"umbral --headless --width {width} --height {height} --near {} --time {world_time} --debug-oxygen {WORLD_OXYGEN} --lucid-surface --void-eye\"}}",
                s.name, s.body, s.body_name,
                s.camera.0.x, s.camera.0.y, s.camera.0.z,
                s.camera.1.x, s.camera.1.y, s.camera.1.z,
                s.ship.x, s.ship.y, s.ship.z, s.body
            )
        })
        .collect();
    line(format!("\"scenes\": [\n{}\n  ],", scenes_json.join(",\n")));
    line(format!(
        "\"metrics_definition\": {{\"differing_vs_baseline\": \"pixeles distintos del baseline de la misma escena\", \"clear_area_changed\": \"pixeles distintos dentro de la elipse protegida (debe ser 0)\", \"corner_luminance_ratio\": \"suma RGB en las 4 cajas de esquina de {CORNER_BOX}x{CORNER_BOX} / la del baseline\"}},"
    ));
    line(format!("\"images\": [\n{}\n  ],", images.join(",\n")));
    line(format!(
        "\"sequence\": {{\"note\": \"animacion secundaria: una vuelta de la onda ({SEQUENCE_FRAMES} cuadros en {DEFORM_PERIOD_SECONDS} s simulados, reproducir a {:.1} fps para tiempo real); misma escena fija\", \"frames\": [\n{}\n  ]}}",
        SEQUENCE_FRAMES as f64 / DEFORM_PERIOD_SECONDS,
        sequence.join(",\n")
    ));
    json.push_str("}\n");
    std::fs::write(output.join("manifest.json"), json).map_err(|e| e.to_string())?;
    println!(
        "{} PNG, {} cuadros de secuencia y manifest.json en {}",
        images.len(),
        sequence.len(),
        output.display()
    );
    Ok(())
}
