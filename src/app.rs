//! Aplicacion del juego: bucle de ventana, entrada y modo headless.

use crate::{
    assets::{obj::load_model, ship_model_path},
    debug::overlay::{OverlayState, draw_overlay, overlay_lines},
    renderer::{framebuffer::Framebuffer, image::save_png, stats::RenderStats},
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use std::{path::PathBuf, time::Instant};

pub struct GameOptions {
    pub width: usize,
    pub height: usize,
    pub headless: bool,
    pub frame_limit: Option<usize>,
    pub output: PathBuf,
    /// Tiempo simulado inicial en segundos (`--time`).
    pub time: f64,
    /// Indice de cuerpo junto al cual arrancar (`--near`).
    pub near: Option<usize>,
    pub culling: bool,
    pub lod: bool,
    pub overlay: bool,
}

/// Acciones de Persona 2, separadas del teclado para probarlas sin ventana.
#[derive(Clone, Copy, Debug)]
pub enum GameplayAction {
    ToggleOrbitAssist,
    Restart,
    ForceOxygen(f64),
    RealOxygen,
}

pub fn apply_gameplay_action(world: &mut World, action: GameplayAction) {
    match action {
        GameplayAction::ToggleOrbitAssist => world.orbit_assist ^= true,
        GameplayAction::Restart => world.reset_ship(),
        GameplayAction::ForceOxygen(value) => world.set_debug_oxygen(value),
        GameplayAction::RealOxygen => world.clear_debug_oxygen(),
    }
}

/// Feedback minimo de recoleccion/estados usando la fuente existente del equipo.
/// No sustituye el HUD artistico de Persona 3.
pub fn gameplay_feedback_lines(world: &World) -> Vec<String> {
    let g = &world.gameplay;
    let mut lines = vec![format!(
        "OXIGENO {:.0}%  RECURSOS {}  REGISTROS {}/3",
        g.oxygen01 * 100.0,
        g.collected_resources,
        g.collected_logs
    )];
    lines.push(if g.game_over {
        "SIN OXIGENO - R: REINICIAR".into()
    } else if g.objective_complete {
        "OBJETIVO COMPLETO - R: REINICIAR".into()
    } else if g.collected_logs >= 3 {
        "OBJETIVO: ACERCATE AL SOL".into()
    } else {
        "OBJETIVO: REUNE LOS TRES REGISTROS DORADOS".into()
    });
    lines.push(format!(
        "O: ASISTENCIA {}  R: REINICIAR",
        if world.orbit_assist { "ON" } else { "OFF" }
    ));
    let target = g
        .logs
        .iter()
        .filter(|log| !log.anchor.collected)
        .filter_map(|log| {
            log.anchor
                .position(&world.system, world.time)
                .map(|p| (log, p))
        })
        .min_by(|(_, a), (_, b)| {
            (*a - world.ship.position)
                .length()
                .total_cmp(&(*b - world.ship.position).length())
        });
    if let Some((log, position)) = target {
        let delta = position - world.ship.position;
        let direction = delta.normalized();
        let horizontal = direction.dot(world.ship.right());
        let vertical = direction.dot(world.ship.up());
        let forward = direction.dot(world.ship.forward());
        lines.push(format!(
            "DESTINO REGISTRO {}: {:.0} U - {} {} {}",
            log.anchor.id + 1,
            delta.length(),
            if forward < 0.0 { "ATRAS" } else { "FRENTE" },
            if horizontal > 0.15 {
                "DERECHA"
            } else if horizontal < -0.15 {
                "IZQUIERDA"
            } else {
                "CENTRO"
            },
            if vertical > 0.15 {
                "ARRIBA"
            } else if vertical < -0.15 {
                "ABAJO"
            } else {
                "NIVEL"
            }
        ));
    }
    if world.forced_oxygen.is_some() {
        lines.push(format!(
            "VISUAL DEBUG {:.0}% - 0: OXIGENO REAL",
            world.oxygen01() * 100.0
        ));
    }
    if let Some(log) = g
        .last_log
        .and_then(|id| g.logs.iter().find(|log| log.anchor.id == id))
    {
        lines.push(format!("{}: {}", log.title, log.text));
    }
    lines
}

pub fn draw_gameplay_feedback(fb: &mut Framebuffer, world: &World) {
    let scale = if fb.width >= 960 && fb.height >= 540 {
        2
    } else {
        1
    };
    let columns = fb.width.saturating_sub(16) / (6 * scale);
    if columns == 0 {
        return;
    }
    let mut lines = Vec::new();
    for text in gameplay_feedback_lines(world) {
        let mut line = String::new();
        for word in text.split_whitespace() {
            if !line.is_empty() && line.len() + word.len() + 1 > columns {
                lines.push(line);
                line = String::new();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    let row_height = 8 * scale + 2;
    lines.truncate((fb.height / 3 / row_height).max(1));
    let y = fb.height.saturating_sub(lines.len() * row_height + 8);
    for py in y..fb.height {
        for x in 0..fb.width {
            fb.put_pixel(x, py, 0x080e18);
        }
    }
    for (i, line) in lines.iter().enumerate() {
        crate::debug::overlay::draw_text(fb, 8, y + 4 + i * row_height, scale, 0xdce8ed, line);
    }
}

fn keyboard_input(window: &Window) -> ShipInput {
    let axis = |plus: &[Key], minus: &[Key]| {
        let down = |keys: &[Key]| keys.iter().any(|&k| window.is_key_down(k));
        f64::from(i8::from(down(plus))) - f64::from(i8::from(down(minus)))
    };
    ShipInput {
        thrust: axis(&[Key::W], &[Key::S]),
        yaw: axis(&[Key::A, Key::Left], &[Key::D, Key::Right]),
        pitch: axis(&[Key::Up], &[Key::Down]),
        roll: axis(&[Key::Q], &[Key::E]),
        boost: window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift),
    }
}

fn overlay_state(world: &World) -> OverlayState {
    OverlayState {
        culling: world.toggles.frustum_culling,
        lod: world.toggles.lod,
        backface: world.toggles.backface_culling,
        orbits_frozen: world.toggles.freeze_orbits,
    }
}

fn describe(stats: &RenderStats) -> String {
    format!(
        "objetos {} (descartados {}) | vertices {} | triangulos enviados {} descartados {} recortados {} rasterizados {} | fragmentos {} | pixeles {}",
        stats.objects_submitted,
        stats.objects_culled,
        stats.vertices_processed,
        stats.triangles_submitted,
        stats.triangles_culled,
        stats.triangles_clipped,
        stats.triangles_rasterized,
        stats.fragments_evaluated,
        stats.pixels_written
    )
}

pub fn run_game(options: GameOptions) -> Result<(), String> {
    let (width, height) = (options.width, options.height);
    let ship_model = load_model(&ship_model_path())?;
    println!(
        "Modelo: {} vertices, {} caras, {} materiales",
        ship_model.vertices.len(),
        ship_model.faces.len(),
        ship_model.materials.len()
    );
    let mut framebuffer = Framebuffer::new(width, height, SPACE_COLOR).map_err(|e| {
        format!(
            "{e}: {width}x{height} (maximo {} pixeles)",
            crate::renderer::framebuffer::MAX_PIXELS
        )
    })?;
    let mut world = World::new(ship_model, width as f64 / height as f64)?;
    world.time = options.time;
    world.toggles.frustum_culling = options.culling;
    world.toggles.lod = options.lod;
    if let Some(index) = options.near {
        world.place_near(index, 25.0)?;
    }
    let stats = world.render(&mut framebuffer);
    println!("{}", describe(&stats));
    let mut show_overlay = options.overlay;
    if options.headless {
        if show_overlay {
            let lines = overlay_lines(
                None,
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            );
            draw_overlay(&mut framebuffer, &lines);
        }
        draw_gameplay_feedback(&mut framebuffer, &world);
        save_png(&options.output, &framebuffer)?;
        println!("PNG: {} ({width}x{height})", options.output.display());
        return Ok(());
    }

    let mut window = Window::new("UMBRAL", width, height, WindowOptions::default())
        .map_err(|e| e.to_string())?;
    window.set_target_fps(60);
    let mut fps = 60.0;
    let (mut frames, mut last, mut title_clock, mut title_frames) =
        (0usize, Instant::now(), 0.0, 0u32);
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        // Limitar dt evita saltos enormes tras arrastrar la ventana o pausar el proceso.
        let dt = now.duration_since(last).as_secs_f64().min(0.1);
        last = now;

        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            apply_gameplay_action(&mut world, GameplayAction::Restart);
        }
        if window.is_key_pressed(Key::F3, KeyRepeat::No) {
            world.toggles.frustum_culling ^= true;
        }
        if window.is_key_pressed(Key::F4, KeyRepeat::No) {
            world.toggles.lod ^= true;
        }
        if window.is_key_pressed(Key::F1, KeyRepeat::No) {
            show_overlay ^= true;
        }
        if window.is_key_pressed(Key::F6, KeyRepeat::No) {
            world.toggles.backface_culling ^= true;
        }
        if window.is_key_pressed(Key::F5, KeyRepeat::No) {
            world.toggles.freeze_orbits ^= true;
        }
        for (key, value) in [
            (Key::Key1, 1.0),
            (Key::Key2, 0.5),
            (Key::Key3, 0.25),
            (Key::Key4, 0.05),
        ] {
            if window.is_key_pressed(key, KeyRepeat::No) {
                apply_gameplay_action(&mut world, GameplayAction::ForceOxygen(value));
            }
        }
        if window.is_key_pressed(Key::O, KeyRepeat::No) {
            apply_gameplay_action(&mut world, GameplayAction::ToggleOrbitAssist);
        }
        if window.is_key_pressed(Key::Key0, KeyRepeat::No) {
            apply_gameplay_action(&mut world, GameplayAction::RealOxygen);
        }
        world.update(&keyboard_input(&window), dt);
        let stats = world.render(&mut framebuffer);

        if show_overlay {
            let lines = overlay_lines(
                Some(fps),
                &stats,
                &overlay_state(&world),
                world.ship.speed,
                world.debug_oxygen01,
                world.time,
            );
            draw_overlay(&mut framebuffer, &lines);
        }
        draw_gameplay_feedback(&mut framebuffer, &world);
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            save_png(&options.output, &framebuffer)?;
            println!("PNG: {}", options.output.display());
        }
        // FPS suavizado para el overlay: media movil exponencial sobre 1/dt.
        fps += (1.0 / dt.max(1e-6) - fps) * (1.0 - (-3.0 * dt).exp());
        title_clock += dt;
        title_frames += 1;
        if title_clock >= 0.5 {
            window.set_title(&format!(
                "UMBRAL | {:.0} FPS | v {:.0} u/s | culling {} | LOD {} | tri {} desc {} ras {} | frag {}",
                f64::from(title_frames) / title_clock,
                world.ship.speed,
                if world.toggles.frustum_culling {
                    "ON"
                } else {
                    "OFF"
                },
                if world.toggles.lod { "ON" } else { "OFF" },
                stats.triangles_submitted,
                stats.triangles_culled,
                stats.triangles_rasterized,
                stats.fragments_evaluated
            ));
            (title_clock, title_frames) = (0.0, 0);
        }
        // minifb presenta pixeles ya calculados; no dibuja los triangulos de la escena.
        window
            .update_with_buffer(&framebuffer.pixels, width, height)
            .map_err(|e| e.to_string())?;
        frames += 1;
        if options.frame_limit.is_some_and(|limit| frames >= limit) {
            break;
        }
    }
    println!("Ventana: {frames} frames mostrados");
    Ok(())
}
