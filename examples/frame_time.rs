//! Mide el tiempo de render por cuadro sin ventana: `cargo run --release --example frame_time`.
//! Compara escenas con y sin descarte; sirve de evidencia de rendimiento (UMB-401/403).

use std::time::Instant;
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::{SPACE_COLOR, World},
};

fn main() -> Result<(), String> {
    let (width, height) = (1280, 720);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    for (label, near) in [
        ("vista inicial", None),
        ("junto a Antena", Some(1)),
        ("junto a Vigilia", Some(3)),
    ] {
        if let Some(index) = near {
            world.place_near(index, 25.0)?;
        }
        for (culling, lod) in [(true, true), (true, false), (false, false)] {
            world.toggles.frustum_culling = culling;
            world.toggles.lod = lod;
            let frames = 30;
            let start = Instant::now();
            let mut stats = Default::default();
            for _ in 0..frames {
                stats = world.render(&mut fb);
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
            println!(
                "{label:16} culling {:3} LOD {:3} | {ms:6.1} ms/cuadro | vertices {:6} | tri descartados {:6} | fragmentos {}",
                if culling { "ON" } else { "OFF" },
                if lod { "ON" } else { "OFF" },
                stats.vertices_processed,
                stats.triangles_culled,
                stats.fragments_evaluated
            );
        }
    }
    Ok(())
}
