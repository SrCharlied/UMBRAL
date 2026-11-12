//! Vista sin ventana de la presentacion final y de los props de los planetas.
//!
//! `cargo run --release --example ending_and_props` escribe en `renders/smoke/`:
//! - `props_<planeta>.png` con y sin props (F2) junto a Antena, Osario y Vigilia, y los
//!   milisegundos por cuadro de cada caso;
//! - `ending_<muerte|victoria>_<segundos>.png` en varios instantes del final.

use std::{path::Path, time::Instant};
use umbral::{
    app::{PresentationLayers, compose_presentation},
    assets::{obj::load_model, ship_model_path},
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::world::{SPACE_COLOR, World},
    visual::creature_distant_turn::CreaturePreset,
};

fn layers(ending: Option<f64>) -> PresentationLayers<'static> {
    PresentationLayers {
        overlay: None,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback: false,
        perception: false,
        ending,
    }
}

fn main() -> Result<(), String> {
    let (width, height) = (1280, 720);
    let dir = Path::new("renders/smoke");
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    for (index, name, gap) in [
        (1, "antena", 10.0),
        (2, "osario", 7.0),
        (3, "vigilia", 14.0),
    ] {
        world.place_near(index, gap)?;
        for props in [true, false] {
            world.toggles.planet_props = props;
            let start = Instant::now();
            let frames = 10;
            let mut stats = Default::default();
            for _ in 0..frames {
                stats = world.render(&mut fb);
            }
            let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(frames);
            let label = if props { "con" } else { "sin" };
            println!(
                "{name:8} {label} props | {ms:6.1} ms/cuadro | tri {} rasterizados {} | fragmentos {}",
                stats.triangles_submitted, stats.triangles_rasterized, stats.fragments_evaluated
            );
            if props {
                save_png(&dir.join(format!("props_{name}.png")), &fb)?;
            }
        }
    }
    world.toggles.planet_props = true;
    for (label, victory) in [("muerte", false), ("victoria", true)] {
        world.restart_game();
        world.place_near(
            if victory { 0 } else { 3 },
            if victory { 25.0 } else { 18.0 },
        )?;
        world.gameplay.collected_resources = 2;
        world.gameplay.collected_logs = if victory { 3 } else { 1 };
        world.time = 312.0;
        if victory {
            world.gameplay.oxygen01 = 0.27;
            world.gameplay.objective_complete = true;
        } else {
            world.gameplay.oxygen01 = 0.0;
            world.gameplay.game_over = true;
        }
        for seconds in [0.6, 1.8, 3.0, 30.0] {
            world.render(&mut fb);
            compose_presentation(&mut fb, &world, &layers(Some(seconds)))?;
            let path = dir.join(format!("ending_{label}_{seconds:04.1}.png"));
            save_png(&path, &fb)?;
            println!("{}", path.display());
        }
    }
    Ok(())
}
