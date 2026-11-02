//! Preview AISLADO de las estrellas observadoras (UMB-310, preview02 «pupila reforzada progresiva»
//! sobre «Pupilas de alfiler»). Ver `docs/watching-stars-preview.md` y
//! `docs/watching-stars-pupil-preview.md`. No integra nada en el juego: compila el modulo por `#[path]`.
//!
//! `cargo run --release --locked --offline --example watching_stars_preview -- --output-dir <carpeta-nueva>`
//!
//! - Matriz: O2 100/70/60/55/50/40/25/10/0 en dos vistas REALES del World (arranque normal y
//!   `--near 3`) y el cielo solo (diagnostico, camara del arranque normal). Por estado, tres PNG
//!   del MISMO framebuffer original: `original`, `observers` (sin post) y `combined` (con la
//!   percepcion APROBADA sin cambiar sus parametros, despues de los ojos).
//! - Guion: caida y recuperacion en el arranque normal con `OxygenTransition` y
//!   `advance_perception_frame` (eventos logicos a 12 fps, no teclado ni vuelo real).
//!
//! Todo a 1280x720; los argumentos se validan antes de crear la carpeta, que no debe existir.

#[path = "../src/visual/watching_stars.rs"]
mod watching_stars;

use std::{fmt::Write as _, path::PathBuf};
use umbral::{
    app::advance_perception_frame,
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::{
        camera::Camera,
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::perception_game::{
        OXYGEN_TRANSITION_RATE, OxygenKeys, OxygenTransition, apply_lucid_perception, ship_focus,
    },
};
use watching_stars::{WatchingStars, render_watching_stars};

const SIZE: (usize, usize) = (1280, 720);
const MATRIX_OXYGEN: [f64; 9] = [1.0, 0.7, 0.6, 0.55, 0.5, 0.4, 0.25, 0.1, 0.0];
/// `--near 3` del juego: cuerpo 3 a 25 unidades de su superficie; tiempo 0 en ambas vistas.
const NEAR_BODY: usize = 3;
const SURFACE_GAP: f64 = 25.0;
const FPS: f64 = 12.0;
const LAST_FRAME: usize = 108;

/// Guion: (cuadro, nombre, teclas sobre el OBJETIVO).
fn script() -> Vec<(usize, &'static str, OxygenKeys)> {
    let preset = |value: f64| OxygenKeys {
        preset: Some(value),
        ..OxygenKeys::default()
    };
    vec![
        (6, "tecla 3: objetivo 25 %", preset(0.25)),
        (60, "tecla 1: recuperacion a 100 %", preset(1.0)),
    ]
}

fn usage() -> String {
    "Uso: watching_stars_preview --output-dir <carpeta-nueva>".into()
}

fn parse_args() -> Result<PathBuf, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [flag, value] = args.as_slice() else {
        return Err(usage());
    };
    if flag != "--output-dir" {
        return Err(usage());
    }
    let output = PathBuf::from(value);
    if output.exists() {
        return Err(format!(
            "la carpeta de salida ya existe: {}",
            output.display()
        ));
    }
    match output.parent() {
        Some(p) if !p.as_os_str().is_empty() && !p.is_dir() => {
            Err(format!("no existe la carpeta padre: {}", p.display()))
        }
        _ => Ok(output),
    }
}

fn v3(v: Vec3) -> String {
    format!("[{:.6}, {:.6}, {:.6}]", v.x, v.y, v.z)
}

fn ids(list: &[usize]) -> String {
    let items: Vec<String> = list.iter().map(|i| i.to_string()).collect();
    format!("[{}]", items.join(", "))
}

fn luma(p: u32) -> f64 {
    let c = |s: u32| f64::from((p >> s) & 0xff);
    0.299 * c(16) + 0.587 * c(8) + 0.114 * c(0)
}

fn with_pixels(pixels: &[u32]) -> Result<Framebuffer, String> {
    let mut fb = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR)?;
    fb.pixels.copy_from_slice(pixels);
    Ok(fb)
}

/// Metadatos de un cuadro: politica frente a lo dibujado, sin confundir conteo con legibilidad.
fn report_json(out: &WatchingStars, original: &[u32]) -> String {
    let r = &out.report;
    let eyes: Vec<String> = r
        .eyes
        .iter()
        .map(|e| {
            format!(
                "{{\"id\": {}, \"rank\": {}, \"pixel\": [{}, {}], \"weight\": {:.6}, \"pupil_offset_px\": [{:.4}, {:.4}], \"pixels_changed\": {}}}",
                e.id,
                e.rank,
                e.pixel.0,
                e.pixel.1,
                e.weight,
                e.pupil_offset[0],
                e.pupil_offset[1],
                e.pixels_changed
            )
        })
        .collect();
    let partial = r.eyes.iter().filter(|e| e.weight < 1.0).count();
    let legible = r.eyes.iter().filter(|e| e.weight >= 0.5).count();
    let changed = original
        .iter()
        .zip(&out.pixels)
        .filter(|(a, b)| a != b)
        .count();
    format!(
        "{{\"oxygen01\": {:.6}, \"global_density_mean_weight\": {:.6}, \"active_count\": {}, \"projected_count\": {}, \"occluded_count\": {}, \"shadowed_count\": {}, \"eyes_drawn\": {}, \"eyes_legible_weight_ge_0_5\": {legible}, \"eyes_partial_weight_lt_1\": {partial}, \"pixels_changed\": {changed}, \"ship_focus_px\": {}, \"active_ids\": {}, \"projected_ids\": {}, \"occluded_ids\": {}, \"shadowed_ids\": {}, \"eyes\": [{}]}}",
        r.oxygen01,
        r.density,
        r.active_ids.len(),
        r.projected_ids.len(),
        r.occluded_ids.len(),
        r.shadowed_ids.len(),
        r.eyes.len(),
        r.ship_focus_px
            .map_or("null".into(), |f| format!("[{:.3}, {:.3}]", f[0], f[1])),
        ids(&r.active_ids),
        ids(&r.projected_ids),
        ids(&r.occluded_ids),
        ids(&r.shadowed_ids),
        eyes.join(", ")
    )
}

fn camera_json(camera: &Camera, ship: Vec3, time: f64) -> String {
    format!(
        "{{\"world_time\": {time:.6}, \"camera_position\": {}, \"camera_target\": {}, \"camera_up\": {}, \"fov_y\": {:.6}, \"ship_position\": {}}}",
        v3(camera.position),
        v3(camera.target),
        v3(camera.up),
        camera.fov_y,
        v3(ship)
    )
}

/// Ojos sobre `original` y despues la percepcion aprobada (con el O2 y el reloj del mundo y la nave
/// proyectada como foco). Los ojos nunca se aplican despues del post.
fn columns(world: &World, original: &Framebuffer) -> Result<(WatchingStars, Framebuffer), String> {
    let observers = render_watching_stars(
        original,
        &world.camera,
        world.ship.position,
        world.debug_oxygen01,
    )?;
    let mut combined = with_pixels(&observers.pixels)?;
    apply_lucid_perception(&mut combined, world)?;
    Ok((observers, combined))
}

fn sky_only(camera: &Camera) -> Result<Framebuffer, String> {
    let mut fb = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR)?;
    let vp = watching_stars::sky_view_projection(camera);
    for star in watching_stars::star_catalog() {
        if let Some((x, y)) = watching_stars::star_pixel(&vp, star.direction, SIZE.0, SIZE.1) {
            fb.write(x, y, 1.0, star.color);
        }
    }
    Ok(fb)
}

fn main() -> Result<(), String> {
    let output = parse_args()?;
    let model = || load_model(&ship_model_path());
    let aspect = SIZE.0 as f64 / SIZE.1 as f64;
    let normal = World::new(model()?, aspect)?;
    let mut near = World::new(model()?, aspect)?;
    near.place_near(NEAR_BODY, SURFACE_GAP)?;
    std::fs::create_dir(&output).map_err(|e| format!("{}: {e}", output.display()))?;
    for dir in ["normal", "near3", "sky", "sequence"] {
        std::fs::create_dir(output.join(dir)).map_err(|e| e.to_string())?;
    }

    // --- Matriz -------------------------------------------------------------------------------
    let mut rows = Vec::new();
    for (view, base) in [("normal", &normal), ("near3", &near), ("sky", &normal)] {
        for &o in &MATRIX_OXYGEN {
            let mut world = World::new(model()?, aspect)?;
            if view == "near3" {
                world.place_near(NEAR_BODY, SURFACE_GAP)?;
            }
            world.debug_oxygen01 = o;
            // Misma camara, geometria, luz y tiempo para todos los O2 de la vista.
            if (world.camera.position - base.camera.position).length() != 0.0
                || world.time != base.time
            {
                return Err(format!("{view}: la vista no se reprodujo igual"));
            }
            let original = if view == "sky" {
                sky_only(&world.camera)?
            } else {
                let mut fb = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR)?;
                world.render(&mut fb);
                fb
            };
            let (observers, combined) = columns(&world, &original)?;
            let stem = format!("{view}/o2_{:03}", (o * 100.0).round() as u32);
            save_png(&output.join(format!("{stem}_original.png")), &original)?;
            save_png(
                &output.join(format!("{stem}_observers.png")),
                &with_pixels(&observers.pixels)?,
            )?;
            save_png(&output.join(format!("{stem}_combined.png")), &combined)?;
            let black = combined.pixels.iter().filter(|&&p| p == 0).count() as f64
                / combined.pixels.len() as f64;
            let mean = combined.pixels.iter().map(|&p| luma(p)).sum::<f64>()
                / combined.pixels.len() as f64;
            rows.push(format!(
                "    {{\"view\": \"{view}\", \"files\": [\"{stem}_original.png\", \"{stem}_observers.png\", \"{stem}_combined.png\"], \"scene\": {}, \"observers_equal_original\": {}, \"combined_equal_observers\": {}, \"combined_black_fraction\": {black:.6}, \"combined_mean_luma\": {mean:.3}, \"observers\": {}}}",
                camera_json(&world.camera, world.ship.position, world.time),
                observers.pixels == original.pixels,
                combined.pixels == observers.pixels,
                report_json(&observers, &original.pixels)
            ));
        }
    }
    let curve: Vec<String> = MATRIX_OXYGEN
        .iter()
        .map(|&o| {
            let w = watching_stars::observer_weights(o)?;
            Ok(format!(
                "{{\"oxygen01\": {o}, \"density\": {:.6}, \"weight_gt_0\": {}, \"weight_ge_0_5\": {}, \"weight_eq_1\": {}}}",
                watching_stars::observer_density(o)?,
                w.iter().filter(|&&v| v > 0.0).count(),
                w.iter().filter(|&&v| v >= 0.5).count(),
                w.iter().filter(|&&v| v == 1.0).count()
            ))
        })
        .collect::<Result<_, String>>()?;
    let reinforcement: Vec<String> = MATRIX_OXYGEN
        .iter()
        .map(|&o| {
            Ok(format!(
                "[{o}, {:.6}]",
                watching_stars::pupil_reinforcement(o)?
            ))
        })
        .collect::<Result<_, String>>()?;
    let ranking = watching_stars::observer_ranking();
    let thresholds = watching_stars::observer_thresholds();
    let first: Vec<String> = ranking[..32]
        .iter()
        .map(|&id| format!("[{id}, {:.6}]", thresholds[id]))
        .collect();
    let knots: Vec<String> = watching_stars::THRESHOLD_KNOTS
        .iter()
        .map(|(r, t)| format!("[{r:.6}, {t}]"))
        .collect();
    let policy = format!(
        "{{\"hypothesis\": \"Pupila reforzada progresiva (preview02 sobre Pupilas de alfiler)\", \"stars\": {}, \"ranking\": \"muestreo del punto mas lejano sobre las direcciones de draw_stars desde el ID 0, empates por ID (fijo, sin semilla aleatoria)\", \"first_32_ranked_ids_with_threshold\": [{}], \"threshold_knots_rank_fraction_to_o2\": [{}], \"onset_oxygen\": {}, \"fade_oxygen\": {}, \"sky_depth_probe\": {:e}, \"eye_px\": {{\"half_width\": {}, \"half_height\": {}, \"closed_aperture\": {}, \"iris_radius\": {}, \"pupil_radius\": {}, \"pupil_shift\": {}}}, \"pupil_reinforcement\": {{\"onset_oxygen\": {}, \"full_oxygen\": {}, \"reinforced_pupil_radius\": {}, \"reinforced_iris_radius\": {}, \"reinforced_iris_gain\": {}, \"at_matrix_oxygen\": [{}]}}, \"curve\": [{}]}}",
        watching_stars::STAR_COUNT,
        first.join(", "),
        knots.join(", "),
        watching_stars::OBSERVER_ONSET_OXYGEN,
        watching_stars::OBSERVER_FADE_OXYGEN,
        watching_stars::SKY_DEPTH_PROBE,
        watching_stars::EYE_HALF_WIDTH_PX,
        watching_stars::EYE_HALF_HEIGHT_PX,
        watching_stars::EYE_CLOSED_APERTURE,
        watching_stars::IRIS_RADIUS_PX,
        watching_stars::PUPIL_RADIUS_PX,
        watching_stars::PUPIL_SHIFT_PX,
        watching_stars::PUPIL_REINFORCE_ONSET_OXYGEN,
        watching_stars::PUPIL_REINFORCE_FULL_OXYGEN,
        watching_stars::REINFORCED_PUPIL_RADIUS_PX,
        watching_stars::REINFORCED_IRIS_RADIUS_PX,
        watching_stars::REINFORCED_IRIS_GAIN,
        reinforcement.join(", "),
        curve.join(", ")
    );
    std::fs::write(
        output.join("matrix.json"),
        format!(
            "{{\n  \"what\": \"UMB-310 preview aislado de estrellas observadoras: matriz O2 x vista; tres columnas del MISMO framebuffer original\",\n  \"size\": [{}, {}],\n  \"views\": {{\"normal\": \"arranque del juego (umbral --headless)\", \"near3\": \"umbral --near {NEAR_BODY} (cuerpo {NEAR_BODY}, separacion {SURFACE_GAP})\", \"sky\": \"diagnostico: solo las estrellas de draw_stars con la camara del arranque normal; la nave no se dibuja pero las pupilas miran a su proyeccion\"}},\n  \"columns\": {{\"original\": \"World::render (o el cielo solo) con world.debug_oxygen01 = O2\", \"observers\": \"render_watching_stars sobre ese cuadro, sin post\", \"combined\": \"percepcion aprobada (apply_lucid_perception) sobre observers, con el O2, el reloj y el foco de la nave del mundo\"}},\n  \"policy\": {policy},\n  \"note\": \"eyes_drawn es politica dibujada, no legibilidad: tras el desenfoque y el tunel de combined un ojo puede no leerse\",\n  \"states\": [\n{}\n  ]\n}}\n",
            SIZE.0,
            SIZE.1,
            rows.join(",\n")
        ),
    )
    .map_err(|e| e.to_string())?;

    // --- Guion --------------------------------------------------------------------------------
    let mut world = World::new(model()?, aspect)?;
    let mut oxygen = OxygenTransition::new(1.0)?;
    let still = ShipInput::default();
    let events = script();
    let mut frames = Vec::new();
    for index in 0..=LAST_FRAME {
        let mut event = "";
        if index > 0 {
            let keys = match events.iter().find(|(at, _, _)| *at == index) {
                Some((_, name, keys)) => {
                    event = name;
                    *keys
                }
                None => OxygenKeys::default(),
            };
            advance_perception_frame(&mut world, &mut oxygen, keys, &still, 1.0 / FPS)?;
        }
        let mut original = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR)?;
        world.render(&mut original);
        let (observers, combined) = columns(&world, &original)?;
        let stem = format!("sequence/{index:03}");
        save_png(
            &output.join(format!("{stem}_observers.png")),
            &with_pixels(&observers.pixels)?,
        )?;
        save_png(&output.join(format!("{stem}_combined.png")), &combined)?;
        let focus = ship_focus(&world);
        let mut row = String::new();
        write!(
            row,
            "    {{\"frame\": {index}, \"files\": [\"{stem}_observers.png\", \"{stem}_combined.png\"], \"real_seconds\": {:.6}, \"event\": \"{event}\", \"oxygen_target\": {:.6}, \"oxygen_shown\": {:.6}, \"world_debug_oxygen01\": {:.6}, \"perception_focus_uv\": [{:.6}, {:.6}], \"scene\": {}, \"observers\": {}}}",
            index as f64 / FPS,
            oxygen.target(),
            oxygen.shown(),
            world.debug_oxygen01,
            focus.uv[0],
            focus.uv[1],
            camera_json(&world.camera, world.ship.position, world.time),
            report_json(&observers, &original.pixels)
        )
        .map_err(|e| e.to_string())?;
        frames.push(row);
    }
    let events_json: Vec<String> = events
        .iter()
        .map(|(at, name, keys)| {
            format!(
                "{{\"frame\": {at}, \"real_seconds\": {:.6}, \"event\": \"{name}\", \"preset\": {}}}",
                *at as f64 / FPS,
                keys.preset.map_or("null".into(), |v| v.to_string())
            )
        })
        .collect();
    std::fs::write(
        output.join("sequence.json"),
        format!(
            "{{\n  \"what\": \"guion logico de caida y recuperacion en el arranque normal (no teclado, no vuelo real; 12 fps fijos)\",\n  \"size\": [{}, {}],\n  \"transition_rate_o2_per_real_second\": {OXYGEN_TRANSITION_RATE},\n  \"ship_input\": \"ninguno (ShipInput::default)\",\n  \"events\": [{}],\n  \"frames\": [\n{}\n  ]\n}}\n",
            SIZE.0,
            SIZE.1,
            events_json.join(", "),
            frames.join(",\n")
        ),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "Preview de estrellas observadoras en {}: {} estados x 3 PNG y {} cuadros de guion",
        output.display(),
        rows.len(),
        LAST_FRAME + 1
    );
    Ok(())
}
