//! Preview del seguimiento corporal horizontal (candidata 02) con el `World` real del juego:
//! framebuffer, z-buffer y render del mundo (no el visor). Compara, con el mismo estado,
//! entradas y tiempo, la LINEA BASE (comparacion de la mirada, cuerpo quieto) contra la
//! CANDIDATA (comparacion + seguimiento horizontal).
//!
//! Matriz declarada (fija, antes de contar):
//! - Saltos de posicion (prueba unitaria, NO fisica), camara de persecucion del juego mirando al
//!   centro, objetivo G = CAMARA, a 0/3/10 s: `cerca-der90` (nave a 2.5 radios, +90),
//!   `cerca-atras180` (2.5, 180), `lejos-izq90` (8, -90), `lejos-atras180` (8, 180) y `wrap`
//!   (8 radios: 20 s asentando en +170 y salto a -170; el cuerpo debe cruzar por detras).
//! - Trayectorias continuas con fisica real (`ShipInput -> World::update -> camara ->
//!   seguimiento`), objetivo G = NAVE, orbita antihoraria a crucero: `orbita-cerca` (2.8 radios) y
//!   `orbita-lejos` (8 radios), dibujadas desde un OBSERVADOR FIJO de preview (no la camara del
//!   juego: asi el giro que se ve es geometria, no movimiento de camara), a 0/6/12/18/24 s.
//! - Vista nativa 1280x720 de `cerca-der90` a 0 s y asentada (10 s).
//!
//! Orbitas congeladas, O2 0.05, pasos de 1/60 s. Escribe los PNG y `manifest.tsv` (objetivo,
//! cuerpo, error, rapidez, mirada y transform aplicado) en `--out`, que no debe existir.
//!
//! ```bash
//! cargo run --example creature_horizontal_follow_preview -- --creature-model m.obj --creature-texture t.png --out dir
//! ```

use std::path::{Path, PathBuf};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::{Mat4, Vec3},
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::{
        camera::Camera,
        ship::{Ship, ShipInput},
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_distant_turn::CreaturePreset,
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::{EyeFollowSettings, GazeAngles},
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const DT: f64 = 1.0 / 60.0;
const DEG: f64 = std::f64::consts::PI / 180.0;
const SMALL: (usize, usize) = (640, 360);

/// Mundo del juego con la criatura, etapas y la comparacion de la mirada; la candidata ademas
/// con el seguimiento horizontal. Orbitas congeladas y O2 bajo (criatura opaca).
fn build(model: &Path, texture: &Path, follow: bool) -> Result<World, String> {
    let mut world = World::new(load_model(&ship_model_path())?, 16.0 / 9.0)?;
    world.toggles.freeze_orbits = true;
    world.debug_oxygen01 = 0.05;
    world.set_creature(StaticCreature::load(
        model,
        texture,
        CreaturePlacement::demo(),
    )?)?;
    world.enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)?;
    world.enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)?;
    if follow {
        world.enable_creature_horizontal_follow()?;
    }
    Ok(world)
}

/// Salto de posicion (no fisica): nave a `radii` radios y `azimuth` del frente de reposo,
/// mirando al centro, y camara de persecucion como la coloca el juego al recolocar la nave.
fn jump(world: &mut World, azimuth: f64, radii: f64) {
    let p = world.creature().expect("criatura").placement();
    let dir = Mat4::rotation_y(p.yaw() + azimuth).transform_dir(Vec3::Z);
    let position = p.center() + dir * (radii * p.world_radius());
    world.ship = Ship::new(position, -dir);
    let basis = world.ship.orientation;
    let (back, up) = (basis.transform_dir(Vec3::Z), basis.transform_dir(Vec3::Y));
    world.camera.position = position + back * world.chase_distance + up * world.chase_height;
    world.camera.target = position - back * 30.0;
    world.camera.up = up;
}

/// Mismo arranque en los dos mundos: preset (cuerpo de frente exacto, sin objetivo previo),
/// objetivo G y salto de la nave; el ojo queda en su objetivo con el cuerpo aun de frente.
fn reset(pair: &mut [World; 2], target: EyeTarget, azimuth: f64, radii: f64) -> Result<(), String> {
    for world in pair.iter_mut() {
        world.place_creature_preset(CreaturePreset::Near)?;
        world.set_creature_eye_target(target)?;
        jump(world, azimuth, radii);
    }
    Ok(())
}

/// Segundos reales solo para el seguimiento y la mirada (saltos: la nave no se mueve).
fn wait(pair: &mut [World; 2], seconds: f64) {
    for _ in 0..(seconds / DT).round() as usize {
        for world in pair.iter_mut() {
            world.update_creature_eye_follow(DT);
        }
    }
}

/// Rumbo de orbita antihoraria (vista desde +Y) a `r0` del centro: tangente con correccion
/// radial; devuelve la entrada de la nave (crucero, sin impulso).
fn orbit_input(world: &World, r0: f64) -> ShipInput {
    let center = world.creature().expect("criatura").placement().center();
    let offset = world.ship.position - center;
    let flat = Vec3::new(offset.x, 0.0, offset.z);
    let radial = flat.normalized();
    let correction = ((flat.length() - r0) / (0.3 * r0)).clamp(-1.0, 1.0);
    let desired = (Vec3::Y.cross(radial) - radial * correction).normalized();
    let forward = world.ship.forward();
    ShipInput {
        thrust: 1.0,
        yaw: ((forward.z * desired.x - forward.x * desired.z) * 4.0).clamp(-1.0, 1.0),
        ..ShipInput::default()
    }
}

/// Camara fija de preview: arriba y al frente-derecha de la criatura, mirando al centro.
fn observer(world: &World, radii: f64) -> Camera {
    let p = world.creature().expect("criatura").placement();
    let mut camera = world.camera;
    let dir = Mat4::rotation_y(p.yaw() + 35.0 * DEG).transform_dir(Vec3::Z);
    camera.position = p.center() + (dir + Vec3::Y * 0.6) * (radii * p.world_radius());
    camera.target = p.center();
    camera.up = Vec3::Y;
    camera
}

struct Sheet {
    out: PathBuf,
    rows: Vec<String>,
}

impl Sheet {
    /// Dibuja `world` (con `view`, la camara se intercambia solo para dibujar y se restaura) y
    /// anota su estado.
    fn shot(
        &mut self,
        world: &mut World,
        view: Option<Camera>,
        name: &str,
        label: (&str, &str),
        t: f64,
        size: (usize, usize),
    ) -> Result<(), String> {
        let mut fb = Framebuffer::new(size.0, size.1, SPACE_COLOR)?;
        let game_camera = world.camera;
        if let Some(camera) = view {
            world.camera = camera;
        }
        world.render(&mut fb);
        world.camera = game_camera;
        save_png(&self.out.join(format!("{name}.png")), &fb)?;
        self.row(world, name, label, t, size);
        Ok(())
    }
    fn row(
        &mut self,
        world: &World,
        name: &str,
        (kind, scenario): (&str, &str),
        t: f64,
        (w, h): (usize, usize),
    ) {
        let p = world.creature().expect("criatura").placement();
        let status = world.creature_horizontal_follow_status();
        let deg = |a: Option<f64>| a.map_or("-".to_string(), |a| format!("{:+.3}", a.to_degrees()));
        let eye = world
            .creature_eye_follow()
            .map_or(GazeAngles::NEUTRAL, |f| f.current());
        let transform = world.creature_render_transform().expect("criatura");
        let radii = |q: Vec3| (q - p.center()).length() / p.world_radius();
        self.rows.push(format!(
            "{name}.png\t{}\t{kind}\t{scenario}\t{t:.3}\t{DT:.6}\t{w}x{h}\t{:.2}\t{:?}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{}\t{:+.3}\t{:+.3}\t{:+.4}\t{:.4},{:.4},{:.4}",
            if status.is_some() { "candidata" } else { "base" },
            world.debug_oxygen01,
            world.creature_eye_target().expect("mirada"),
            radii(world.ship.position),
            radii(world.camera.position),
            deg(status.and_then(|s| s.target_azimuth)),
            deg(status.map(|s| s.follow.yaw())),
            deg(status.and_then(|s| s.follow.error())),
            status.map_or("-".to_string(), |s| format!("{:.3}", s.follow.speed().to_degrees())),
            eye.yaw().to_degrees(),
            eye.pitch().to_degrees(),
            transform.rotation.y.to_degrees(),
            transform.position.x,
            transform.position.y,
            transform.position.z,
        ));
    }
    fn pair(
        &mut self,
        pair: &mut [World; 2],
        view: Option<Camera>,
        prefix: &str,
        label: (&str, &str),
        t: f64,
        size: (usize, usize),
    ) -> Result<(), String> {
        for (world, mode) in pair.iter_mut().zip(["base", "candidata"]) {
            self.shot(world, view, &format!("{prefix}-{mode}"), label, t, size)?;
        }
        Ok(())
    }
}

fn run() -> Result<(), String> {
    let (mut model, mut texture, mut out) = (None, None, None);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || {
            args.next()
                .map(PathBuf::from)
                .ok_or(format!("falta el valor de {a}"))
        };
        match a.as_str() {
            "--creature-model" => model = Some(value()?),
            "--creature-texture" => texture = Some(value()?),
            "--out" => out = Some(value()?),
            _ => return Err(format!("argumento desconocido: {a}")),
        }
    }
    let (model, texture, out) = (
        model.ok_or("falta --creature-model")?,
        texture.ok_or("falta --creature-texture")?,
        out.ok_or("falta --out")?,
    );
    std::fs::create_dir(&out)
        .map_err(|e| format!("--out {} (no debe existir): {e}", out.display()))?;
    let mut pair = [
        build(&model, &texture, false)?,
        build(&model, &texture, true)?,
    ];
    let mut sheet = Sheet {
        out,
        rows: vec![
            "archivo\tmodo\ttipo\tescenario\tt_s\tdt_s\tres\to2\tg\tnave_radios\tcamara_radios\tobjetivo_az\tcuerpo_yaw\terror\trapidez_gps\tojo_yaw\tojo_pitch\ttransform_yaw\ttransform_pos"
                .into(),
        ],
    };
    // Saltos de posicion con la camara del juego.
    let jumps: [(&str, f64, f64); 4] = [
        ("cerca-der90", 90.0, 2.5),
        ("cerca-atras180", 180.0, 2.5),
        ("lejos-izq90", -90.0, 8.0),
        ("lejos-atras180", 180.0, 8.0),
    ];
    for (name, azimuth, radii) in jumps {
        reset(&mut pair, EyeTarget::Camera, azimuth * DEG, radii)?;
        let mut now = 0.0;
        for t in [0.0, 3.0, 10.0] {
            wait(&mut pair, t - now);
            now = t;
            let label = ("salto de posicion", name);
            sheet.pair(&mut pair, None, &format!("{name}-t{t:02}"), label, t, SMALL)?;
            if name == "cerca-der90" && (t == 0.0 || t == 10.0) {
                let prefix = format!("nativa1280-{name}-t{t:02}");
                sheet.pair(&mut pair, None, &prefix, label, t, (1280, 720))?;
            }
        }
    }
    // Wrap: asentado en +170 y salto a -170 (el arco corto pasa por detras, 20 grados).
    reset(&mut pair, EyeTarget::Camera, 170.0 * DEG, 8.0)?;
    wait(&mut pair, 20.0);
    for world in pair.iter_mut() {
        jump(world, -170.0 * DEG, 8.0);
    }
    let mut now = 0.0;
    for t in [0.0, 3.0, 10.0] {
        wait(&mut pair, t - now);
        now = t;
        let label = ("salto de posicion", "wrap 170 -> -170");
        sheet.pair(&mut pair, None, &format!("wrap-t{t:02}"), label, t, SMALL)?;
    }
    // Trayectorias continuas con fisica real, vistas desde un observador fijo.
    for (name, orbit_radii, view_radii) in [("orbita-cerca", 2.8, 4.5), ("orbita-lejos", 8.0, 4.5)]
    {
        for world in pair.iter_mut() {
            let preset = if orbit_radii < 4.0 {
                CreaturePreset::Near
            } else {
                CreaturePreset::Far
            };
            world.place_creature_preset(preset)?;
            world.set_creature_eye_target(EyeTarget::Ship)?;
        }
        let view = observer(&pair[0], view_radii);
        let r0 = orbit_radii
            * pair[0]
                .creature()
                .expect("criatura")
                .placement()
                .world_radius();
        let samples = [0.0, 6.0, 12.0, 18.0, 24.0];
        let mut next = 0;
        for frame in 0..=(24.0 / DT).round() as usize {
            let t = frame as f64 * DT;
            if frame > 0 {
                for world in pair.iter_mut() {
                    let input = orbit_input(world, r0);
                    world.update(&input, DT);
                    world.update_creature_eye_follow(DT);
                }
            }
            if next < samples.len() && (t - samples[next]).abs() < DT / 2.0 {
                let label = ("trayectoria continua (observador fijo)", name);
                let prefix = format!("{name}-t{:02}", samples[next] as usize);
                sheet.pair(&mut pair, Some(view), &prefix, label, samples[next], SMALL)?;
                next += 1;
            }
        }
    }
    std::fs::write(sheet.out.join("manifest.tsv"), sheet.rows.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    println!(
        "Preview: {} imagenes, dt {DT:.6} s (base = comparacion sin seguimiento; candidata = con seguimiento)",
        sheet.rows.len() - 1
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
