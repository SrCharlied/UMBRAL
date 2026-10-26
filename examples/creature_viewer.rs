//! Visor interactivo de un OBJ con su textura PNG (RGB8): criatura estatica y camara orbital
//! alrededor de su centro envolvente. Usa el mismo render que el modo headless
//! (`visual::creature_viewer::render_view`, sobre `draw_model_with` y `TexturedShader`).
//! No integra nada en la escena. Ver `docs/creature-viewer.md`.
//!
//! cargo run --release --example creature_viewer -- --model modelo.obj --texture textura.png
//!     [--output captura.png] [--frames N] [--yaw RAD] [--pitch RAD] [--distance RADIOS] [--unlit]
//!     [--headless --output salida.png [--benchmark]]

use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{path::PathBuf, time::Instant};
use umbral::{
    assets::obj::load_model,
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    debug::overlay::draw_overlay,
    renderer::{framebuffer::Framebuffer, image::save_png, stats::RenderStats, texture::Texture},
    scene::world::SPACE_COLOR,
    visual::{
        creature_viewer::{
            CreatureBounds, HudTiming, OrbitController, OrbitInput, OrbitPose, TARGET_FPS,
            hud_lines, render_view, validate_output_path,
        },
        textured::TexturedMode,
    },
};

const TITLE: &str = "UMBRAL - visor tecnico de criatura";
const WARMUPS: usize = 2;
const SAMPLES: usize = 11;
/// Periodo de actualizacion de los tiempos del HUD.
const HUD_PERIOD: f64 = 0.5;

struct HudWindow {
    started: Instant,
    frames: u32,
    render_ms: f64,
}

impl HudWindow {
    fn new(start: Instant) -> Self {
        Self {
            started: start,
            frames: 0,
            render_ms: 0.0,
        }
    }

    fn presented(&mut self, render_ms: f64, clock: impl FnOnce() -> Instant) -> Option<HudTiming> {
        // Se invoca solo despues de update_with_buffer: incluye el cuadro actual,
        // el HUD, las capturas y la espera de presentacion, sin usar dt de camara.
        let presented_at = clock();
        let period = presented_at.duration_since(self.started).as_secs_f64();
        self.frames += 1;
        self.render_ms += render_ms;
        if period < HUD_PERIOD {
            return None;
        }
        let timing = HudTiming {
            render_ms: self.render_ms / f64::from(self.frames),
            presented_fps: f64::from(self.frames) / period,
        };
        // El siguiente intervalo y sus acumuladores nacen en el mismo timestamp.
        (self.started, self.frames, self.render_ms) = (presented_at, 0, 0.0);
        Some(timing)
    }
}

#[derive(Debug)]
struct Options {
    model: PathBuf,
    texture: PathBuf,
    output: Option<PathBuf>,
    headless: bool,
    benchmark: bool,
    frames: Option<usize>,
    pose: OrbitPose,
    mode: TexturedMode,
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} necesita un valor"))
}

fn number(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<f64, String> {
    let text = value(args, flag)?;
    text.parse::<f64>()
        .map_err(|_| format!("{flag} necesita un numero: {text}"))
}

/// Valida toda la linea de comandos antes de leer archivos o abrir la ventana.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let defaults = OrbitPose::default();
    let (mut model, mut texture, mut output) = (None, None, None);
    let (mut headless, mut benchmark, mut frames) = (false, false, None);
    let (mut yaw, mut pitch, mut distance) =
        (defaults.yaw(), defaults.pitch(), defaults.distance_radii());
    let mut mode = TexturedMode::Lit;
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--model" => model = Some(PathBuf::from(value(&mut args, &flag)?)),
            "--texture" => texture = Some(PathBuf::from(value(&mut args, &flag)?)),
            "--output" => output = Some(PathBuf::from(value(&mut args, &flag)?)),
            "--headless" => headless = true,
            "--benchmark" => benchmark = true,
            "--unlit" => mode = TexturedMode::Unlit,
            "--frames" => {
                let text = value(&mut args, &flag)?;
                frames = Some(
                    text.parse::<usize>()
                        .ok()
                        .filter(|&n| n >= 1)
                        .ok_or(format!("--frames necesita un entero >= 1: {text}"))?,
                );
            }
            "--yaw" => yaw = number(&mut args, &flag)?,
            "--pitch" => pitch = number(&mut args, &flag)?,
            "--distance" => distance = number(&mut args, &flag)?,
            _ => return Err(format!("Argumento desconocido: {flag}")),
        }
    }
    let model = model.ok_or("Falta --model archivo.obj")?;
    let texture = texture.ok_or("Falta --texture archivo.png")?;
    if headless && output.is_none() {
        return Err("--headless necesita --output archivo.png".into());
    }
    if headless && frames.is_some() {
        return Err("--frames solo aplica a la ventana (sin --headless)".into());
    }
    if benchmark && !headless {
        return Err("--benchmark requiere --headless".into());
    }
    let pose = OrbitPose::new(yaw, pitch, distance)?;
    Ok(Options {
        model,
        texture,
        output,
        headless,
        benchmark,
        frames,
        pose,
        mode,
    })
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn describe(stats: &RenderStats) -> String {
    format!(
        "triangulos rasterizados {} descartados {} | fragmentos {} | pixeles {}",
        stats.triangles_rasterized,
        stats.triangles_culled,
        stats.fragments_evaluated,
        stats.pixels_written
    )
}

fn main() -> Result<(), String> {
    let options = parse_args(std::env::args().skip(1))?;
    if let Some(output) = &options.output {
        validate_output_path(output, &[&options.model, &options.texture])?;
    }

    let start = Instant::now();
    let model = load_model(&options.model)?;
    let load_model_ms = elapsed_ms(start);
    let start = Instant::now();
    let texture = Texture::load_png(&options.texture)?;
    let load_texture_ms = elapsed_ms(start);
    let bounds = CreatureBounds::from_model(&model)?;
    let mut fb = Framebuffer::new(DEFAULT_WIDTH, DEFAULT_HEIGHT, SPACE_COLOR)?;
    let texture_size = (texture.width(), texture.height());
    println!(
        "Modelo: {} v, {} vt, {} vn, {} triangulos | carga {load_model_ms:.1} ms",
        model.vertices.len(),
        model.uvs.len(),
        model.normals.len(),
        model.faces.len()
    );
    println!(
        "Textura: {}x{} RGB8 | carga {load_texture_ms:.0} ms",
        texture_size.0, texture_size.1
    );

    if options.headless {
        let output = options.output.as_ref().ok_or("falta --output")?;
        let pose = options.pose;
        let runs = if options.benchmark {
            WARMUPS + SAMPLES
        } else {
            1
        };
        let (mut times, mut stats) = (Vec::new(), RenderStats::default());
        for run in 0..runs {
            let start = Instant::now();
            stats = render_view(&mut fb, &model, &texture, &bounds, &pose, options.mode)?;
            let ms = elapsed_ms(start);
            if !options.benchmark || run >= WARMUPS {
                times.push(ms);
            }
        }
        // HUD determinista (sin tiempos) y PNG fuera de la medicion.
        draw_overlay(&mut fb, &hud_lines(options.mode, texture_size, &pose, None));
        save_png(output, &fb)?;
        println!(
            "Render {}x{} {:?} | yaw {} pitch {} distancia {} radios | {}",
            fb.width,
            fb.height,
            options.mode,
            pose.yaw(),
            pose.pitch(),
            pose.distance_radii(),
            describe(&stats)
        );
        if options.benchmark {
            let mut sorted = times.clone();
            sorted.sort_by(f64::total_cmp);
            println!(
                "Benchmark render ({WARMUPS} calentamientos, {SAMPLES} mediciones, sin HUD ni PNG): mediana {:.1} ms | min {:.1} | max {:.1} | ordenadas {:.1?}",
                sorted[SAMPLES / 2],
                sorted[0],
                sorted[SAMPLES - 1],
                sorted
            );
        } else {
            println!("Render de un cuadro: {:.1} ms", times[0]);
        }
        println!("PNG: {}", output.display());
        return Ok(());
    }

    let (width, height) = (fb.width, fb.height);
    let mut window =
        Window::new(TITLE, width, height, WindowOptions::default()).map_err(|e| e.to_string())?;
    window.set_target_fps(TARGET_FPS);
    let mut controller = OrbitController::new(options.pose);
    let mut mode = options.mode;
    let unknown = HudTiming {
        render_ms: f64::NAN,
        presented_fps: f64::NAN,
    };
    let mut timing = unknown;
    let (mut frames, mut last, started) = (0usize, Instant::now(), Instant::now());
    let mut hud_window = HudWindow::new(Instant::now());
    let (mut total_render_ms, mut total_present_ms) = (0.0, 0.0);
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        // Instant es monotono: dt es finito y no negativo; step lo recorta a MAX_DT.
        let dt = now.duration_since(last).as_secs_f64();
        last = now;

        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            controller.reset();
        }
        if window.is_key_pressed(Key::L, KeyRepeat::No) {
            mode = match mode {
                TexturedMode::Lit => TexturedMode::Unlit,
                TexturedMode::Unlit => TexturedMode::Lit,
            };
        }
        let down = |key| window.is_key_down(key);
        let input = OrbitInput::from_keys(
            down(Key::Left),
            down(Key::Right),
            down(Key::Up),
            down(Key::Down),
            down(Key::W),
            down(Key::S),
        );
        controller.step(input, dt)?;
        let pose = controller.pose();

        let start = Instant::now();
        render_view(&mut fb, &model, &texture, &bounds, &pose, mode)?;
        let render_ms = elapsed_ms(start);
        draw_overlay(&mut fb, &hud_lines(mode, texture_size, &pose, Some(timing)));
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            match &options.output {
                Some(path) => {
                    save_png(path, &fb)?;
                    println!("Captura: {}", path.display());
                }
                None => println!("P ignorada: falta --output archivo.png"),
            }
        }
        // Presentacion: copia a la ventana y espera del tope de FPS de minifb.
        let start = Instant::now();
        window
            .update_with_buffer(&fb.pixels, width, height)
            .map_err(|e| e.to_string())?;
        let present_ms = elapsed_ms(start);
        let sample = hud_window.presented(render_ms, Instant::now);

        frames += 1;
        total_render_ms += render_ms;
        total_present_ms += present_ms;
        if let Some(sample) = sample {
            timing = sample;
        }
        if options.frames.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    if frames > 0 {
        println!(
            "Ventana: {frames} cuadros presentados en {seconds:.2} s ({:.1} por segundo) | render CPU medio {:.1} ms | presentacion media {:.1} ms (incluye la espera del tope de {TARGET_FPS})",
            frames as f64 / seconds,
            total_render_ms / frames as f64,
            total_present_ms / frames as f64
        );
    }
    if let (Some(_), Some(path)) = (options.frames, &options.output) {
        save_png(path, &fb)?;
        println!("PNG final del smoke: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn hud_first_slow_frames_include_the_current_frame_cost() {
        let origin = Instant::now();
        let mut hud = HudWindow::new(origin);
        let startup = Duration::from_millis(1);
        let frame = Duration::from_millis(600);
        let first = origin + startup + frame;
        let second = first + frame;
        let samples: Vec<_> = [
            hud.presented(frame.as_secs_f64() * 1000.0, || first),
            hud.presented(frame.as_secs_f64() * 1000.0, || second),
        ]
        .into_iter()
        .flatten()
        .map(|sample| sample.presented_fps)
        .collect();
        let expected = vec![
            1.0 / (first - origin).as_secs_f64(),
            1.0 / (second - first).as_secs_f64(),
        ];
        assert_eq!(samples, expected);
    }

    #[test]
    fn hud_variable_frame_costs_share_the_same_sample_window() {
        let origin = Instant::now();
        let mut hud = HudWindow::new(origin);
        let mut presented_at = origin;
        let costs = [
            (Duration::from_millis(100), 10.0),
            (Duration::from_millis(150), 120.0),
            (Duration::from_millis(300), 240.0),
        ];
        for &(wall_time, render_ms) in &costs[..2] {
            presented_at += wall_time;
            assert!(hud.presented(render_ms, || presented_at).is_none());
        }
        presented_at += costs[2].0;
        let sample = hud.presented(costs[2].1, || presented_at).unwrap();
        assert_eq!(
            sample.presented_fps,
            costs.len() as f64 / (presented_at - origin).as_secs_f64()
        );
        assert_eq!(
            sample.render_ms,
            costs.iter().map(|&(_, ms)| ms).sum::<f64>() / costs.len() as f64
        );
    }

    #[test]
    fn hud_capture_pause_is_in_fps_but_not_cpu_render_time() {
        let origin = Instant::now();
        let mut hud = HudWindow::new(origin);
        let render = Duration::from_millis(40);
        let overlay = Duration::from_millis(10);
        let capture = Duration::from_millis(1200);
        let present = Duration::from_millis(20);
        // El callback entrega el reloj al terminar la presentacion, no al
        // iniciar el cuadro ni antes de guardar la captura.
        let presented_at = origin + render + overlay + capture + present;
        let sample = hud
            .presented(render.as_secs_f64() * 1000.0, || presented_at)
            .unwrap();
        assert_eq!(
            sample.presented_fps,
            1.0 / (render + overlay + capture + present).as_secs_f64()
        );
        assert_eq!(sample.render_ms, render.as_secs_f64() * 1000.0);
    }

    #[test]
    fn hud_reset_uses_the_exact_post_present_boundary_and_new_frames_only() {
        let origin = Instant::now();
        let mut hud = HudWindow::new(origin);
        let first = origin + Duration::from_millis(200);
        assert!(hud.presented(10.0, || first).is_none());
        // Se sobrepasa el periodo: no se arrastra el excedente ni se redondea
        // el reloj a HUD_PERIOD al empezar la siguiente ventana.
        let boundary = first + Duration::from_millis(600);
        let previous = hud.presented(90.0, || boundary).unwrap();
        assert_eq!(
            previous.presented_fps,
            2.0 / (boundary - origin).as_secs_f64()
        );
        assert_eq!(previous.render_ms, (10.0 + 90.0) / 2.0);
        let next = boundary + Duration::from_millis(200);
        assert!(hud.presented(7.0, || next).is_none());
        let end = next + Duration::from_millis(300);
        let sample = hud.presented(13.0, || end).unwrap();
        assert_eq!(sample.presented_fps, 2.0 / (end - boundary).as_secs_f64());
        assert_eq!(sample.render_ms, (7.0 + 13.0) / 2.0);
    }

    fn parse(args: &[&str]) -> Result<Options, String> {
        let base = ["--model", "m.obj", "--texture", "t.png"];
        parse_args(base.iter().chain(args).map(|s| s.to_string()))
    }

    #[test]
    fn defaults_open_the_window_with_the_default_pose() {
        let options = parse(&[]).unwrap();
        assert!(!options.headless && !options.benchmark);
        assert_eq!((options.frames, options.output), (None, None));
        assert_eq!(options.pose, OrbitPose::default());
        assert_eq!(options.mode, TexturedMode::Lit);
        let options = parse(&["--unlit", "--frames", "3", "--output", "c.png"]).unwrap();
        assert_eq!(options.frames, Some(3));
        assert_eq!(options.mode, TexturedMode::Unlit);
    }

    #[test]
    fn pose_flags_reproduce_the_pose_exactly() {
        let options = parse(&[
            "--headless",
            "--output",
            "o.png",
            "--yaw",
            "0.8",
            "--pitch",
            "-0.3",
            "--distance",
            "6",
        ])
        .unwrap();
        assert_eq!(options.pose, OrbitPose::new(0.8, -0.3, 6.0).unwrap());
    }

    #[test]
    fn invalid_arguments_fail_before_any_side_effect() {
        let invalid: [&[&str]; 17] = [
            &["--frames", "0"],
            &["--frames", "-1"],
            &["--frames", "dos"],
            &["--frames"],
            &["--yaw", "NaN"],
            &["--yaw", "inf"],
            &["--pitch", "NaN"],
            &["--pitch", "2"],
            &["--distance", "1.0"],
            &["--distance", "13"],
            &["--distance", "abc"],
            &["--headless"],
            &["--headless", "--output", "o.png", "--frames", "2"],
            &["--benchmark"],
            &["--benchmark", "--output", "o.png"],
            &["--zoom", "2"],
            &["--output"],
        ];
        for args in invalid {
            let error = parse(args).expect_err(&format!("aceptado: {args:?}"));
            assert!(!error.is_empty());
        }
        assert!(parse_args(["--texture", "t.png"].map(String::from)).is_err());
        assert!(parse_args(["--model", "m.obj"].map(String::from)).is_err());
    }
}
