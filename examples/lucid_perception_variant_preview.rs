//! Preview AISLADO de UMB-310 variante 02 (grises + morado del monstruo, tunel hacia la nave,
//! vision borrosa y negro a 0 % o con la muestra explicita de ataque) sobre cuadros REALES del
//! juego: `World`, camara, luz, nave versionada, superficie lucida y ojo del vacio aprobados y el
//! arranque `--near` junto a Antena y Vigilia. Sin ventana, HUD ni integracion a App/World/CLI.
//! Ver `docs/lucid-perception-variant.md`.
//!
//! `cargo run --release --locked --offline --example lucid_perception_variant_preview -- --output-dir <carpeta-nueva> [--time <s>]`
//!
//! Cada escena se carga y se dibuja UNA vez con tiempo y O2 de mundo fijos; solo cambia el
//! postproceso. El foco es la posicion proyectada real de `World.ship`. Argumentos, tiempo, foco
//! y todos los casos se validan antes de crear la carpeta, que no debe existir.

#[path = "../src/visual/lucid_perception.rs"]
mod lucid_perception;
#[path = "../src/visual/lucid_perception_variant.rs"]
mod lucid_perception_variant;

use lucid_perception::lucid_perception;
use lucid_perception_variant::{
    BLACKOUT_FADE_OXYGEN, BLUR_MAX_SIGMA_FRACTION, MONSTER_GREY, MONSTER_PURPLE,
    PERCEPTION_FULL_OXYGEN, PERCEPTION_ONSET_OXYGEN, TINT_MAX, TINT_PURPLE_SHARE,
    TUNNEL_CLOSED_RADIUS, TUNNEL_CLOSED_SOFTNESS, TUNNEL_OPEN_RADIUS, TUNNEL_OPEN_SOFTNESS,
    lucid_perception_variant,
};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::world::{SPACE_COLOR, World},
    visual::{lucid_game::set_lucid_surface, void_game::set_void_eye},
};

/// Tiempo de mundo por defecto (el mismo que el preview 01).
const DEFAULT_WORLD_TIME: f64 = 1.3;
/// O2 del MUNDO, fijo: sin grietas lucidas, para aislar el postproceso (igual que la 01).
const WORLD_OXYGEN: f64 = 1.0;
/// Distancia a la superficie del arranque `--near` del juego.
const SURFACE_GAP: f64 = 25.0;
/// Niveles de O2 del POSTPROCESO (01 de referencia y 02 candidata).
const POST_OXYGEN: [f64; 7] = [1.0, 0.5, 0.25, 0.1, 0.05, 0.01, 0.0];
/// O2 del postproceso en la muestra de ataque (positivo a proposito).
const ATTACK_OXYGEN: f64 = 1.0;
/// Secuencia GUIONIZADA de caida de O2 (no es consumo real): de 50 % a 0 % y negro sostenido.
const SEQUENCE_FRAMES: usize = 48;
const SEQUENCE_FALL_FRAMES: usize = 40;
const SEQUENCE_START_OXYGEN: f64 = 0.5;
const SEQUENCE_FPS: f64 = 12.0;
/// Radio (fraccion del lado menor) de la zona de la nave medida en el manifest.
const SHIP_AREA_RADIUS: f64 = 0.17;

struct Scene {
    name: &'static str,
    body: usize,
    body_name: &'static str,
    frame: Framebuffer,
    focus_uv: [f64; 2],
    camera: [f64; 6],
    ship: [f64; 3],
}

fn usage() -> String {
    "Uso: lucid_perception_variant_preview --output-dir <carpeta-nueva> [--time <segundos >= 0>]"
        .into()
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

fn sequence_oxygen(frame: usize) -> f64 {
    let fall = 1.0 - frame as f64 / SEQUENCE_FALL_FRAMES as f64;
    (SEQUENCE_START_OXYGEN * fall).max(0.0)
}

fn sequence_time(world_time: f64, frame: usize) -> f64 {
    world_time + frame as f64 / SEQUENCE_FPS
}

/// Carga y dibuja una escena una sola vez y proyecta la nave real para el foco.
fn render_scene(name: &'static str, near_void: bool, time: f64) -> Result<Scene, String> {
    let ship_model = load_model(&ship_model_path())?;
    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut world = World::new(ship_model, width as f64 / height as f64)?;
    world.time = time;
    world.debug_oxygen01 = WORLD_OXYGEN;
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
    if clip.w <= 0.0 {
        return Err(format!("{name}: la nave queda detras de la camara"));
    }
    // Misma conversion NDC -> pantalla que `pipeline::draw_model`, normalizada a [0, 1].
    let focus_uv = [clip.x / clip.w * 0.5 + 0.5, 0.5 - clip.y / clip.w * 0.5];
    let (c, s) = (world.camera, world.ship.position);
    Ok(Scene {
        name,
        body,
        body_name: world.system.bodies[body].name,
        frame,
        focus_uv,
        camera: [
            c.position.x,
            c.position.y,
            c.position.z,
            c.target.x,
            c.target.y,
            c.target.z,
        ],
        ship: [s.x, s.y, s.z],
    })
}

fn luma(p: u32) -> f64 {
    0.299 * ((p >> 16) & 0xff) as f64 + 0.587 * ((p >> 8) & 0xff) as f64 + 0.114 * (p & 0xff) as f64
}

/// Metricas sin rotulos ni dependencias, en el orden del manifest.
fn metrics(scene: &Scene, pixels: &[u32]) -> String {
    let (w, h) = (scene.frame.width, scene.frame.height);
    let (fx, fy) = (scene.focus_uv[0] * w as f64, scene.focus_uv[1] * h as f64);
    let ship_radius = SHIP_AREA_RADIUS * w.min(h) as f64;
    let (mut differing, mut black, mut luma_sum) = (0usize, 0usize, 0.0);
    let (mut violet_sum, mut lit) = (0.0, 0usize);
    let (mut ship_px, mut ship_visible) = (0usize, 0usize);
    for (i, (&p, &b)) in pixels.iter().zip(&scene.frame.pixels).enumerate() {
        differing += usize::from(p != b);
        black += usize::from(p == 0);
        luma_sum += luma(p);
        let (r, g, bl) = ((p >> 16) & 0xff, (p >> 8) & 0xff, p & 0xff);
        if r + g + bl > 60 {
            violet_sum += r.min(bl) as f64 - g as f64;
            lit += 1;
        }
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        if ((x - fx).powi(2) + (y - fy).powi(2)).sqrt() <= ship_radius {
            ship_px += 1;
            ship_visible += usize::from(p != 0);
        }
    }
    let n = pixels.len() as f64;
    format!(
        "\"differing_vs_baseline\": {differing}, \"black_fraction\": {:.4}, \"mean_luma\": {:.2}, \"mean_violet_minRB_minus_G_lit\": {:.2}, \"ship_area_nonblack_fraction\": {:.4}",
        black as f64 / n,
        luma_sum / n,
        if lit == 0 {
            0.0
        } else {
            violet_sum / lit as f64
        },
        ship_visible as f64 / ship_px.max(1) as f64
    )
}

fn save(path: &Path, pixels: Vec<u32>) -> Result<(), String> {
    let mut fb = Framebuffer::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, SPACE_COLOR)?;
    fb.pixels = pixels;
    save_png(path, &fb)
}

fn pct(oxygen: f64) -> String {
    format!("{:03}", (oxygen * 100.0).round() as u32)
}

fn main() -> Result<(), String> {
    let (output, world_time) = parse_args()?;
    // Escenas en memoria (sin escribir nada) para conocer el foco real de cada una.
    let scenes = [
        render_scene("antena", false, world_time)?,
        render_scene("vigilia", true, world_time)?,
    ];
    // Todos los casos se validan con la funcion real antes de crear la carpeta.
    let probe = Framebuffer::new(1, 1, 0)?;
    for scene in &scenes {
        for oxygen in POST_OXYGEN {
            lucid_perception(&probe, oxygen, world_time)?;
            lucid_perception_variant(&probe, oxygen, world_time, scene.focus_uv, false)?;
        }
        lucid_perception_variant(&probe, ATTACK_OXYGEN, world_time, scene.focus_uv, true)?;
        for k in 0..SEQUENCE_FRAMES {
            let (o, t) = (sequence_oxygen(k), sequence_time(world_time, k));
            lucid_perception_variant(&probe, o, t, scene.focus_uv, false)?;
        }
    }
    // `create_dir` falla si la ruta ya existe (carpeta, archivo o input): nunca sobrescribe.
    std::fs::create_dir(&output).map_err(|e| {
        format!(
            "No se pudo crear {} (debe ser nueva): {e}",
            output.display()
        )
    })?;
    let sequence_dir = output.join("sequence");
    std::fs::create_dir(&sequence_dir).map_err(|e| e.to_string())?;

    let mut images = Vec::new();
    let mut sequence = Vec::new();
    let mut entry = |file: String, scene: &Scene, case: &str, post: String, pixels: &[u32]| {
        images.push(format!(
            "    {{\"file\": \"{file}\", \"scene\": \"{}\", \"case\": \"{case}\", \"world_oxygen01\": {WORLD_OXYGEN}, \"world_time\": {world_time}, {post}, {}}}",
            scene.name,
            metrics(scene, pixels)
        ));
        println!("{file}");
    };
    for scene in &scenes {
        let base = &scene.frame;
        let file = format!("{}_00_baseline_sin_postproceso.png", scene.name);
        save_png(&output.join(&file), base)?;
        entry(
            file,
            scene,
            "baseline",
            "\"post_oxygen01\": null, \"post_time\": null, \"attack_blackout\": false".into(),
            &base.pixels,
        );
        for oxygen in POST_OXYGEN {
            let post = format!(
                "\"post_oxygen01\": {oxygen}, \"post_time\": {world_time}, \"attack_blackout\": false"
            );
            let reference = lucid_perception(base, oxygen, world_time)?;
            let file = format!("{}_01_referencia_o2_{}.png", scene.name, pct(oxygen));
            entry(
                file.clone(),
                scene,
                "referencia_01",
                post.clone(),
                &reference,
            );
            save(&output.join(&file), reference)?;
            let candidate =
                lucid_perception_variant(base, oxygen, world_time, scene.focus_uv, false)?;
            let file = format!("{}_02_variante_o2_{}.png", scene.name, pct(oxygen));
            entry(file.clone(), scene, "variante_02", post, &candidate);
            save(&output.join(&file), candidate)?;
        }
        let attack =
            lucid_perception_variant(base, ATTACK_OXYGEN, world_time, scene.focus_uv, true)?;
        let file = format!(
            "{}_02_variante_ataque_o2_{}.png",
            scene.name,
            pct(ATTACK_OXYGEN)
        );
        entry(
            file.clone(),
            scene,
            "variante_02_muestra_ataque",
            format!(
                "\"post_oxygen01\": {ATTACK_OXYGEN}, \"post_time\": {world_time}, \"attack_blackout\": true"
            ),
            &attack,
        );
        save(&output.join(&file), attack)?;
        for k in 0..SEQUENCE_FRAMES {
            let (oxygen, time) = (sequence_oxygen(k), sequence_time(world_time, k));
            let pixels = lucid_perception_variant(base, oxygen, time, scene.focus_uv, false)?;
            let file = format!("{}_f{k:02}.png", scene.name);
            save(&sequence_dir.join(&file), pixels)?;
            sequence.push(format!(
                "    {{\"file\": \"sequence/{file}\", \"scene\": \"{}\", \"post_oxygen01\": {oxygen}, \"post_time\": {time}, \"attack_blackout\": false}}",
                scene.name
            ));
        }
    }

    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut json = String::from("{\n");
    let mut line = |s: String| {
        let _ = writeln!(json, "  {s}");
    };
    line("\"preview\": \"UMB-310 variante 02: grises + morado del monstruo, tunel hacia la nave, vision borrosa, negro a 0 % o con muestra de ataque\",".into());
    line("\"status\": \"una hipotesis pendiente de decision artistica de Charlie; aislada, no integrada; sin combate, dano, recuperacion, consumo de O2, estrellas observadoras ni HUD\",".into());
    line("\"command\": \"cargo run --release --locked --offline --example lucid_perception_variant_preview -- --output-dir <carpeta-nueva> [--time <s>]\",".into());
    line(format!("\"resolution\": [{width}, {height}],"));
    line("\"labels_in_images\": false,".into());
    line(format!(
        "\"world\": {{\"time_fixed\": {world_time}, \"oxygen01_fixed\": {WORLD_OXYGEN}, \"note\": \"tiempo, O2, camara, luz, materiales y nave del MUNDO fijos; solo varian O2, tiempo y ataque del POSTPROCESO\", \"approved_setters\": [\"lucid_game::set_lucid_surface (Antena)\", \"void_game::set_void_eye (Vigilia)\"], \"ship_model\": \"assets/models/ship/Umbral_v4_Desgastada.obj (versionado)\", \"private_assets\": false, \"loads_per_scene\": 1, \"renders_per_scene\": 1}},"
    ));
    line(format!(
        "\"variant_02_settings\": {{\"perception_onset_oxygen\": {PERCEPTION_ONSET_OXYGEN}, \"perception_full_oxygen\": {PERCEPTION_FULL_OXYGEN}, \"blackout_fade_oxygen\": {BLACKOUT_FADE_OXYGEN}, \"tint\": {{\"monster_purple_rgb\": {MONSTER_PURPLE:?}, \"monster_grey_rgb\": {MONSTER_GREY:?}, \"purple_share\": {TINT_PURPLE_SHARE}, \"max_mix\": {TINT_MAX}, \"mode\": \"luma * mezcla gris-morado normalizada (conserva brillo, negro sigue negro)\"}}, \"tunnel_short_side_units\": {{\"open_radius\": {TUNNEL_OPEN_RADIUS}, \"open_softness\": {TUNNEL_OPEN_SOFTNESS}, \"closed_radius\": {TUNNEL_CLOSED_RADIUS}, \"closed_softness\": {TUNNEL_CLOSED_SOFTNESS}, \"center\": \"foco = nave proyectada\"}}, \"blur\": {{\"max_sigma_fraction_of_short_side\": {BLUR_MAX_SIGMA_FRACTION}, \"max_sigma_px\": {:.2}, \"kind\": \"gaussiano separable, radio ceil(3 sigma), bordes acotados\"}}, \"time\": \"validado; la variante 02 es estatica en el tiempo (sin pulsos ni onda)\", \"status\": \"parametros candidatos, no aprobados\"}},",
        BLUR_MAX_SIGMA_FRACTION * width.min(height) as f64
    ));
    line(format!(
        "\"attack_sample\": {{\"post_oxygen01\": {ATTACK_OXYGEN}, \"note\": \"booleano explicito del preview; sin combate, dano, duracion ni recuperacion\"}},"
    ));
    let scenes_json: Vec<String> = scenes
        .iter()
        .map(|s| {
            let c = s.camera;
            format!(
                "    {{\"name\": \"{}\", \"body\": {}, \"body_name\": \"{}\", \"start\": \"World::place_near(body, {SURFACE_GAP}) como --near\", \"focus_uv\": [{:.6}, {:.6}], \"focus_px\": [{:.1}, {:.1}], \"focus_source\": \"World.ship.position proyectada con camera.view_projection()\", \"camera_position\": [{:.4}, {:.4}, {:.4}], \"camera_target\": [{:.4}, {:.4}, {:.4}], \"ship_position\": [{:.4}, {:.4}, {:.4}], \"baseline_equivalent_game_command\": \"umbral --headless --width {width} --height {height} --near {} --time {world_time} --debug-oxygen {WORLD_OXYGEN} --lucid-surface --void-eye\"}}",
                s.name,
                s.body,
                s.body_name,
                s.focus_uv[0],
                s.focus_uv[1],
                s.focus_uv[0] * width as f64,
                s.focus_uv[1] * height as f64,
                c[0],
                c[1],
                c[2],
                c[3],
                c[4],
                c[5],
                s.ship[0],
                s.ship[1],
                s.ship[2],
                s.body
            )
        })
        .collect();
    line(format!("\"scenes\": [\n{}\n  ],", scenes_json.join(",\n")));
    line(format!(
        "\"metrics_definition\": {{\"differing_vs_baseline\": \"pixeles distintos del baseline\", \"black_fraction\": \"fraccion de pixeles exactamente 0\", \"mean_luma\": \"luma Rec.601 media\", \"mean_violet_minRB_minus_G_lit\": \"media de min(R,B)-G en pixeles con R+G+B>60\", \"ship_area_nonblack_fraction\": \"fraccion no negra a <= {SHIP_AREA_RADIUS} del lado menor del foco\"}},"
    ));
    line(format!("\"images\": [\n{}\n  ],", images.join(",\n")));
    line(format!(
        "\"sequence\": {{\"note\": \"GUIONIZADA, no consumo real: O2 del postproceso baja linealmente de {SEQUENCE_START_OXYGEN} a 0 en {SEQUENCE_FALL_FRAMES} cuadros y se sostiene en negro; reproducir a {SEQUENCE_FPS} fps\", \"fps\": {SEQUENCE_FPS}, \"frames\": [\n{}\n  ]}}",
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
