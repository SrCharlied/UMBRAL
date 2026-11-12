//! Vista sin ventana del espacio procedural de la presentacion:
//! `cargo run --release --example sector_preview` escribe en `renders/smoke/`:
//! - `sector_inicio.png`: la vista inicial con tanques modelados y marcadores;
//! - `sector_planeta.png`: frente a un planeta errante con su tanque;
//! - `sector_lejos.png`: viajando lejos del sistema, con varios planetas a la vista.

use std::path::Path;
use umbral::{
    app::{PresentationLayers, compose_presentation},
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::{
        sectors::generate,
        ship::{Ship, ShipInput},
        world::{SPACE_COLOR, World},
    },
    visual::creature_distant_turn::CreaturePreset,
};

fn shot(world: &World, fb: &mut Framebuffer, name: &str) -> Result<(), String> {
    let start = std::time::Instant::now();
    for _ in 0..9 {
        world.render(fb);
    }
    let stats = world.render(fb);
    let ms = start.elapsed().as_secs_f64() * 100.0;
    compose_presentation(
        fb,
        world,
        &PresentationLayers {
            overlay: None,
            comparison: false,
            distant_turn: false,
            horizontal: false,
            preset: CreaturePreset::Far,
            gameplay_feedback: false,
            perception: false,
            ending: None,
        },
    )?;
    let path = Path::new("renders/smoke").join(name);
    save_png(&path, fb)?;
    println!(
        "{} | {ms:.1} ms/cuadro (escena) | objetos {} | planetas errantes {} | tanques vivos {}",
        path.display(),
        stats.objects_submitted,
        world.sectors.as_ref().map_or(0, |f| f.planets().count()),
        world.sectors.as_ref().map_or(0, |f| f.tanks().count())
    );
    Ok(())
}

fn settle(world: &mut World) {
    for _ in 0..30 {
        world.update(&ShipInput::default(), 0.05);
    }
}

fn main() -> Result<(), String> {
    std::fs::create_dir_all("renders/smoke").map_err(|e| e.to_string())?;
    let (width, height) = (1280, 720);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut world = World::new(
        load_model(&ship_model_path())?,
        width as f64 / height as f64,
    )?;
    world.enable_sectors();
    world.place_near(1, 30.0)?;
    shot(&world, &mut fb, "sector_inicio.png")?;

    let sector = (2..200i64)
        .map(|i| generate((i, 0, -i / 2)))
        .find(|s| !s.planets.is_empty() && !s.tanks.is_empty())
        .ok_or("ningun sector con planeta")?;
    let planet = &sector.planets[0];
    let eye = planet.center
        + Vec3::new(
            planet.radius * 1.2,
            planet.radius * 0.6,
            planet.radius * 3.2,
        );
    world.ship = Ship::new(eye, (planet.center - eye).normalized());
    world.enable_sectors();
    settle(&mut world);
    shot(&world, &mut fb, "sector_planeta.png")?;

    let far = Vec3::new(2600.0, 300.0, -1800.0);
    world.ship = Ship::new(far, Vec3::new(1.0, -0.1, -0.4).normalized());
    world.enable_sectors();
    settle(&mut world);
    shot(&world, &mut fb, "sector_lejos.png")?;

    // Cinturon de asteroides del sistema, visto de canto desde fuera.
    let belt_eye = Vec3::new(0.0, 25.0, 560.0);
    world.ship = Ship::new(belt_eye, Vec3::new(0.25, -0.03, -1.0).normalized());
    world.enable_sectors();
    settle(&mut world);
    shot(&world, &mut fb, "asteroides_cinturon.png")?;

    // Baliza de registro del espacio profundo.
    let log = umbral::scene::sectors::DEEP_LOG_POSITIONS[2];
    world.ship = Ship::new(
        log + Vec3::new(30.0, 10.0, 90.0),
        Vec3::new(-0.3, -0.1, -1.0).normalized(),
    );
    world.enable_sectors();
    settle(&mut world);
    shot(&world, &mut fb, "registro_lejano.png")?;

    // Costo de generar al cruzar de sector (el peor caso: una cara entera de 5x5 sectores).
    let mut field = umbral::scene::sectors::SectorField::new();
    field.update(far);
    let mut worst: f64 = 0.0;
    for step in 1..=6 {
        let start = std::time::Instant::now();
        field.update(far + Vec3::new(umbral::scene::sectors::SECTOR_SIZE * step as f64, 0.0, 0.0));
        worst = worst.max(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!("Generacion al cruzar de sector: peor {worst:.1} ms");
    Ok(())
}
