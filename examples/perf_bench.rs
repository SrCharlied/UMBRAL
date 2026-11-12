//! Tiempo por etapa de un cuadro, sin ventana: `cargo run --release --example perf_bench`.
//! Escena 3D (`World::render`), ojos de `--watching-stars` y percepcion de `--lucid-perception`,
//! en varias posiciones y con O2 bajo para que los efectos esten activos.

use std::time::Instant;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
    visual::{
        lucid_game::set_lucid_surface, perception_game::apply_lucid_perception,
        void_game::set_void_eye, watching_stars_game::apply_watching_stars,
    },
};

fn ms(frames: u32, f: &mut impl FnMut()) -> f64 {
    f(); // calentamiento
    let start = Instant::now();
    for _ in 0..frames {
        f();
    }
    start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames)
}

fn main() -> Result<(), String> {
    let (width, height) = (1280, 720);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    set_lucid_surface(&mut world, true)?;
    set_void_eye(&mut world, true)?;
    if std::env::args().any(|a| a == "--sectors") {
        world.enable_sectors();
    }
    let frames = 20;
    let mut total = 0.0;
    for (label, near) in [
        ("inicio", None),
        ("Sol", Some((0, 40.0))),
        ("Antena", Some((1, 10.0))),
        ("Osario", Some((2, 8.0))),
        ("Vigilia", Some((3, 14.0))),
    ] {
        if let Some((index, gap)) = near {
            world.place_near(index, gap)?;
        }
        for oxygen in [1.0, 0.3] {
            world.set_debug_oxygen(oxygen);
            let scene = ms(frames, &mut || {
                world.render(&mut fb);
            });
            let mut eyes_err = None;
            let eyes = ms(frames, &mut || {
                if let Err(e) = apply_watching_stars(&mut fb, &world) {
                    eyes_err = Some(e);
                }
            });
            let mut post_err = None;
            let post = ms(frames, &mut || {
                if let Err(e) = apply_lucid_perception(&mut fb, &world) {
                    post_err = Some(e);
                }
            });
            if let Some(e) = eyes_err.or(post_err) {
                return Err(e);
            }
            let sum = scene + eyes + post;
            total += sum;
            println!(
                "{label:8} O2 {oxygen:.1} | escena {scene:6.2} | ojos {eyes:6.2} | percepcion {post:6.2} | total {sum:6.2} ms ({:5.1} FPS)",
                1000.0 / sum
            );
        }
    }
    println!("SUMA {total:.2} ms");
    Ok(())
}
