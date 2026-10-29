//! Preview del giro global lejano (candidata) con el `World` real del juego: framebuffer,
//! camara de persecucion y z-buffer del mundo (no el render del visor). Compara, con el mismo
//! encuadre y tiempo, la mirada sola (ojo, sin giro) contra el giro global (ojo + giro).
//!
//! Matriz declarada (fija, antes de contar):
//! - Posiciones de la nave en el marco de reposo de la criatura (la camara va detras):
//!   `l8-yaw+25`, `l8-yaw-25`, `l8-pitch+20`, `l8-pitch-20` (8 radios), `m5-yaw+25` (5) y
//!   `c2.5-yaw+25` (2.5), a 0/2/8/16 s; `c2.5-frente` (el preset CERCA) a 0 y 16 s.
//! - `l8-yaw+25` a 8 s con O2 0.25 (etapa parcial).
//! - Transicion lejos -> cerca: 16 s en `l8-yaw+25`, 4 s de acercamiento radial hasta 2.5
//!   radios y 8 s quieta; muestras en 0/1/2/3/4/6/8/12 s desde el inicio del acercamiento.
//! - Vista nativa 1280x720 de `l8-yaw+25` a 16 s y su detalle (recorte 320x180 ampliado x2).
//!
//! Orbitas congeladas, O2 0.05 salvo donde se dice, pasos de 1/60 s. Escribe los PNG y
//! `manifest.tsv` en `--out` (que no debe existir).
//!
//! ```bash
//! cargo run --example creature_distant_turn_preview -- --creature-model m.obj --creature-texture t.png --out dir
//! ```

use std::path::{Path, PathBuf};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::{Mat4, Vec3},
    renderer::{framebuffer::Framebuffer, image::save_png},
    scene::{
        ship::Ship,
        world::{SPACE_COLOR, World},
    },
    visual::{
        creature_distant_turn::TurnAngles,
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::{EyeFollowSettings, GazeAngles},
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const DT: f64 = 1.0 / 60.0;
const DEG: f64 = std::f64::consts::PI / 180.0;

/// Mundo del juego con la criatura, etapas y mirada a la camara (como la comparacion); con
/// `turn`, ademas el giro global. Orbitas congeladas y O2 bajo (criatura opaca).
fn build(model: &Path, texture: &Path, turn: bool) -> Result<World, String> {
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
    if turn {
        world.enable_creature_distant_turn()?;
    }
    Ok(world)
}

/// Direccion `(yaw, elevacion)` del marco de reposo de la criatura, en mundo.
fn direction(world: &World, yaw: f64, elevation: f64) -> Vec3 {
    let placement = world.creature().expect("criatura").placement();
    Mat4::rotation_y(placement.yaw()).transform_dir(Vec3::new(
        yaw.sin() * elevation.cos(),
        elevation.sin(),
        yaw.cos() * elevation.cos(),
    ))
}

/// Nave a `radii` radios del centro en `dir`, mirando al centro, y camara de persecucion como
/// la coloca el juego al recolocar la nave (detras y arriba, mirando por delante de la nave).
fn place(world: &mut World, dir: Vec3, radii: f64) {
    let placement = world.creature().expect("criatura").placement();
    let position = placement.center() + dir * (radii * placement.world_radius());
    world.ship = Ship::new(position, -dir);
    let basis = world.ship.orientation;
    let (back, up) = (basis.transform_dir(Vec3::Z), basis.transform_dir(Vec3::Y));
    world.camera.position = position + back * world.chase_distance + up * world.chase_height;
    world.camera.target = position - back * 30.0;
    world.camera.up = up;
}

/// Encuadre fijo en los dos mundos; el ojo llega a su objetivo **sin** giro (el giro global
/// se activa despues, en reposo): estado t = 0 identico en los dos.
fn reset_pair(
    pair: &mut [World; 2],
    dir_yaw: f64,
    elevation: f64,
    radii: f64,
) -> Result<(), String> {
    for (i, world) in pair.iter_mut().enumerate() {
        world.disable_creature_distant_turn();
        let dir = direction(world, dir_yaw, elevation);
        place(world, dir, radii);
        world.update_creature_eye_follow(1000.0);
        if i == 1 {
            world.enable_creature_distant_turn()?;
        }
    }
    Ok(())
}

struct Sheet {
    out: PathBuf,
    rows: Vec<String>,
}

impl Sheet {
    fn shot(
        &mut self,
        world: &World,
        name: &str,
        label: &str,
        t: f64,
        (width, height): (usize, usize),
    ) -> Result<Framebuffer, String> {
        let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
        world.render(&mut fb);
        save_png(&self.out.join(format!("{name}.png")), &fb)?;
        self.row(world, name, label, t, (width, height));
        Ok(fb)
    }
    fn row(&mut self, world: &World, name: &str, label: &str, t: f64, (w, h): (usize, usize)) {
        let placement = world.creature().expect("criatura").placement();
        let ship = (world.ship.position - placement.center()).length() / placement.world_radius();
        let status = world.creature_distant_turn_status();
        let turn = world
            .creature_distant_turn()
            .map_or(TurnAngles::NEUTRAL, |t| t.current());
        let eye = world
            .creature_eye_follow()
            .map_or(GazeAngles::NEUTRAL, |f| f.current());
        let camera =
            (world.camera.position - placement.center()).length() / placement.world_radius();
        self.rows.push(format!(
            "{name}.png\t{}\t{label}\t{t:.3}\t{DT:.6}\t{w}x{h}\t{:.2}\t{ship:.3}\t{camera:.3}\t{}\t{:+.3}\t{:+.3}\t{:+.3}\t{:+.3}\t{:.1},{:.1},{:.1}",
            if status.is_some() { "global" } else { "ojo-solo" },
            world.debug_oxygen01,
            status
                .and_then(|s| s.weight)
                .map_or("-".to_string(), |w| format!("{w:.3}")),
            turn.yaw().to_degrees(),
            turn.pitch().to_degrees(),
            eye.yaw().to_degrees(),
            eye.pitch().to_degrees(),
            world.camera.position.x,
            world.camera.position.y,
            world.camera.position.z,
        ));
    }
}

fn step(pair: &mut [World; 2], seconds: f64) {
    let steps = (seconds / DT).round() as usize;
    for _ in 0..steps {
        for world in pair.iter_mut() {
            world.update_creature_eye_follow(DT);
        }
    }
}

/// Recorte de `w x h` alrededor de `(cx, cy)` ampliado `k` veces (vecino mas cercano).
fn detail(
    fb: &Framebuffer,
    (cx, cy): (f64, f64),
    (w, h): (usize, usize),
    k: usize,
) -> Result<Framebuffer, String> {
    let x0 = (cx as isize - w as isize / 2).clamp(0, (fb.width - w) as isize) as usize;
    let y0 = (cy as isize - h as isize / 2).clamp(0, (fb.height - h) as isize) as usize;
    let mut out = Framebuffer::new(w * k, h * k, SPACE_COLOR)?;
    for y in 0..h * k {
        for x in 0..w * k {
            out.pixels[y * w * k + x] = fb.pixels[(y0 + y / k) * fb.width + x0 + x / k];
        }
    }
    Ok(out)
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
            "archivo\tmodo\tposicion\tt_s\tdt_s\tres\to2\tnave_radios\tcamara_radios\tpeso\tgiro_yaw\tgiro_pitch\tojo_yaw\tojo_pitch\tcamara_xyz"
                .into(),
        ],
    };
    let small = (640, 360);
    let mut identical_at_zero = 0;
    let positions: [(&str, f64, f64, f64, &[f64]); 7] = [
        ("l8-yaw+25", 25.0, 0.0, 8.0, &[0.0, 2.0, 8.0, 16.0]),
        ("l8-yaw-25", -25.0, 0.0, 8.0, &[0.0, 2.0, 8.0, 16.0]),
        ("l8-pitch+20", 0.0, 20.0, 8.0, &[0.0, 2.0, 8.0, 16.0]),
        ("l8-pitch-20", 0.0, -20.0, 8.0, &[0.0, 2.0, 8.0, 16.0]),
        ("m5-yaw+25", 25.0, 0.0, 5.0, &[0.0, 2.0, 8.0, 16.0]),
        ("c2.5-yaw+25", 25.0, 0.0, 2.5, &[0.0, 2.0, 8.0, 16.0]),
        ("c2.5-frente", 0.0, 0.0, 2.5, &[0.0, 16.0]),
    ];
    for (label, yaw, elevation, radii, times) in positions {
        reset_pair(&mut pair, yaw * DEG, elevation * DEG, radii)?;
        let mut now = 0.0;
        for &t in times {
            step(&mut pair, t - now);
            now = t;
            let a = sheet.shot(&pair[0], &format!("{label}-t{t:02}-ojo"), label, t, small)?;
            let b = sheet.shot(
                &pair[1],
                &format!("{label}-t{t:02}-global"),
                label,
                t,
                small,
            )?;
            if t == 0.0 {
                if a.pixels != b.pixels {
                    return Err(format!("{label}: t = 0 deberia ser identico"));
                }
                identical_at_zero += 1;
            }
        }
    }
    // Etapa parcial (O2 0.25) a 8 s.
    reset_pair(&mut pair, 25.0 * DEG, 0.0, 8.0)?;
    for world in pair.iter_mut() {
        world.debug_oxygen01 = 0.25;
        world.snap_creature_reveal();
    }
    step(&mut pair, 8.0);
    sheet.shot(&pair[0], "l8-yaw+25-o025-t08-ojo", "l8-yaw+25", 8.0, small)?;
    sheet.shot(
        &pair[1],
        "l8-yaw+25-o025-t08-global",
        "l8-yaw+25",
        8.0,
        small,
    )?;
    for world in pair.iter_mut() {
        world.debug_oxygen01 = 0.05;
        world.snap_creature_reveal();
    }
    // Vista nativa 1280x720 y detalle, a 16 s.
    reset_pair(&mut pair, 25.0 * DEG, 0.0, 8.0)?;
    step(&mut pair, 16.0);
    for (i, mode) in ["ojo", "global"].into_iter().enumerate() {
        let fb = sheet.shot(
            &pair[i],
            &format!("nativa1280-l8-yaw+25-t16-{mode}"),
            "l8-yaw+25",
            16.0,
            (1280, 720),
        )?;
        let center = pair[i].creature().expect("criatura").placement().center();
        let c = pair[i]
            .camera
            .view_projection()
            .transform(center.extend(1.0));
        let screen = (
            (c.x / c.w * 0.5 + 0.5) * 1280.0,
            (0.5 - c.y / c.w * 0.5) * 720.0,
        );
        let name = format!("detalle-x2-l8-yaw+25-t16-{mode}");
        save_png(
            &sheet.out.join(format!("{name}.png")),
            &detail(&fb, screen, (320, 180), 2)?,
        )?;
        sheet.row(
            &pair[i],
            &name,
            "recorte 320x180 de la nativa",
            16.0,
            (640, 360),
        );
    }
    // Transicion lejos -> cerca: 16 s lejos, 4 s de acercamiento radial, 8 s quieta.
    reset_pair(&mut pair, 25.0 * DEG, 0.0, 8.0)?;
    step(&mut pair, 16.0);
    let samples = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0];
    let mut next = 0;
    let steps = (12.0 / DT).round() as usize;
    for k in 0..=steps {
        let t = k as f64 * DT;
        let radii = 8.0 - (8.0 - 2.5) * (t / 4.0).min(1.0);
        for world in pair.iter_mut() {
            let dir = direction(world, 25.0 * DEG, 0.0);
            place(world, dir, radii);
            if k > 0 {
                world.update_creature_eye_follow(DT);
            }
        }
        if next < samples.len() && (t - samples[next]).abs() < DT / 2.0 {
            for (i, mode) in ["ojo", "global"].into_iter().enumerate() {
                let name = format!("transicion-t{:04.1}-{mode}", samples[next]);
                sheet.shot(&pair[i], &name, "lejos->cerca", samples[next], small)?;
            }
            next += 1;
        }
    }
    std::fs::write(sheet.out.join("manifest.tsv"), sheet.rows.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    println!(
        "Preview: {} imagenes, {identical_at_zero} pares identicos en t = 0, dt {DT:.6} s",
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
