//! Aplicacion del juego: bucle de ventana, entrada y modo headless.

use crate::{
    assets::{obj::load_model, ship_model_path},
    debug::overlay::{OverlayState, draw_overlay, overlay_lines},
    renderer::{framebuffer::Framebuffer, image::save_png, stats::RenderStats},
    scene::{
        ship::ShipInput,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_reveal::{RevealSettings, step_debug_oxygen},
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreatureOptions, CreaturePlacement, StaticCreature},
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
    /// Criatura estatica opcional (debug, opt-in); `None` deja el juego como siempre.
    pub creature: Option<CreatureOptions>,
    /// Revelacion de la criatura por oxigeno (`--creature-reveal`, requiere la criatura).
    pub creature_reveal: bool,
    /// Revelacion por etapas (`--creature-reveal-stages`, requiere la criatura; excluyente
    /// con `creature_reveal`).
    pub creature_reveal_stages: bool,
    /// Oxigeno de depuracion inicial en `[0, 1]` (`--debug-oxygen`, por defecto 1).
    pub debug_oxygen: f64,
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
    world.debug_oxygen01 = options.debug_oxygen;
    world.toggles.frustum_culling = options.culling;
    world.toggles.lod = options.lod;
    if let Some(index) = options.near {
        world.place_near(index, 25.0)?;
    }
    let near_creature = options.creature.as_ref().is_some_and(|c| c.near);
    if let Some(requested) = &options.creature {
        // Se carga una sola vez; el mundo toma posesion del modelo y la textura.
        let start = Instant::now();
        let creature = StaticCreature::load(
            &requested.model,
            &requested.texture,
            CreaturePlacement::demo(),
        )?;
        let (placement, texture) = (creature.placement(), creature.texture());
        println!(
            "Criatura: {} vertices, {} triangulos, textura {}x{} | carga {:.0} ms | centro ({}, {}, {}) radio {} (ubicacion de demostracion)",
            creature.model().vertices.len(),
            creature.model().faces.len(),
            texture.width(),
            texture.height(),
            start.elapsed().as_secs_f64() * 1000.0,
            placement.center().x,
            placement.center().y,
            placement.center().z,
            placement.world_radius()
        );
        world.set_creature(creature)?;
        if near_creature {
            world.place_near_creature()?;
        }
        if options.creature_reveal {
            // Arranca ya en el objetivo del oxigeno inicial: cuadros headless deterministas.
            world.enable_creature_reveal(RevealSettings::default());
            let shown = world.creature_reveal().map_or(0.0, |r| r.shown());
            println!(
                "Revelacion por oxigeno: O2 {:.2} | visibilidad {:.0}%",
                world.debug_oxygen01,
                shown * 100.0
            );
        }
        if options.creature_reveal_stages {
            // Particion una vez al activar; tambien arranca ya en el objetivo del O2 inicial.
            world.enable_staged_creature_reveal(
                StagedRevealSettings::default(),
                CREATURE_EYE_REGION,
            )?;
            let (eye, body) = world
                .staged_creature_reveal()
                .map_or((0.0, 0.0), |r| r.alphas());
            let (eye_faces, body_faces) = world
                .staged_creature_parts()
                .map_or((0, 0), |p| (p.eye_faces(), p.body_faces()));
            println!(
                "Revelacion por etapas: O2 {:.2} | ojo {:.0}% cuerpo {:.1}% | caras ojo {eye_faces} cuerpo {body_faces} (region del ojo calibrada para este asset)",
                world.debug_oxygen01,
                eye * 100.0,
                body * 100.0
            );
        }
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
        let elapsed = now.duration_since(last).as_secs_f64();
        // Limitar dt evita saltos enormes tras arrastrar la ventana o pausar el proceso.
        let dt = elapsed.min(0.1);
        last = now;

        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            if near_creature {
                // Con --near-creature, R vuelve al arranque frente a la criatura.
                world.place_near_creature()?;
            } else {
                world.reset_ship();
            }
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
                world.debug_oxygen01 = value;
            }
        }
        // Una pulsacion por paso, sin repeticion al mantener la tecla. Los presets 1-4
        // siguen disponibles; M/N se aplican despues si coinciden en el mismo cuadro.
        world.debug_oxygen01 = step_debug_oxygen(
            world.debug_oxygen01,
            window.is_key_pressed(Key::M, KeyRepeat::No),
            window.is_key_pressed(Key::N, KeyRepeat::No),
        );
        world.update(&keyboard_input(&window), dt);
        // La revelacion avanza con el tiempo real, sin el limite de `dt` de la fisica.
        world.update_creature_reveal(elapsed);
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
