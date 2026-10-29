//! Preview SEPARADO del Sol con ojo (candidata 03, no integrado al juego): el `World` real
//! (camara de persecucion, framebuffer, z-buffer, estrellas, planetas, nave) con la criatura
//! anclada en el centro del Sol y `World::render_solar_reveal_preview`.
//!
//! Matriz declarada (fija, antes de generar), 1280x720, orbitas congeladas en t = 0:
//! - Criatura: el OBJ y la textura pedidos, `CreaturePlacement` en el centro del Sol con la
//!   escala 80 de la 02 (punto de partida, no aprobacion), yaw 0; etapas, mirada a la CAMARA y
//!   seguimiento horizontal 02 aprobados sin cambios. Luz de la criatura: `--light camera`
//!   (candidata, por defecto) o `world` (baseline).
//! - Perfiles: `frontal320` (nave en +Z a 298, camara a ~320 del centro) y `oblicuo600` (nave a
//!   70 grados hacia +X, 578 de distancia y 60 de altura; camara a ~600). Antes de cada perfil,
//!   20 s reales de mirada y seguimiento (asentados hacia la camara).
//! - Por perfil: `baseline` (el mismo cuadro del juego sin criatura) y O2 100/50/40/30/28/25/15/5 %
//!   estaticos (revelacion saltada al objetivo).
//! - Temporal (`frontal320`): O2 0.34 -> 0.21 en 8 s y 0.21 -> 0.34 en 8 s, pasos de 1/60 s con
//!   `update_creature_reveal` y `update_creature_eye_follow`; PNG cada 0.5 s y `temporal-60hz.tsv`
//!   con TODOS los pasos (para detectar saltos del Sol).
//! - Diagnosticos (4): planeta Antena en transito delante del Sol (`frontal320`) con O2 40 % y
//!   5 %, y la luz global (`world`) con O2 30 % y 5 %.
//!
//! Escribe los PNG, `manifest.tsv` y `manifest.json` en `--out`, que no debe existir.
//!
//! ```bash
//! cargo run --release --example solar_reveal_preview -- --creature-model m.obj --creature-texture t.png --out dir
//! ```

use std::path::{Path, PathBuf};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::Vec3,
    renderer::{framebuffer::Framebuffer, image::save_png, stats::RenderStats},
    scene::world::{SPACE_COLOR, World},
    visual::{
        creature_eye_comparison::EyeTarget,
        creature_eye_follow::EyeFollowSettings,
        solar_reveal_preview::SolarRevealLight,
        staged_creature_reveal::{CREATURE_EYE_REGION, StagedRevealSettings},
        static_creature::{CreaturePlacement, StaticCreature},
    },
};

const DT: f64 = 1.0 / 60.0;
const DEG: f64 = std::f64::consts::PI / 180.0;
const SIZE: (usize, usize) = (1280, 720);
/// Escala de partida (la de la 02); el encuadre se documenta, no se aprueba aqui.
const CREATURE_RADIUS: f64 = 80.0;
const LEVELS: [f64; 8] = [1.0, 0.50, 0.40, 0.30, 0.28, 0.25, 0.15, 0.05];

struct Profile {
    name: &'static str,
    azimuth: f64,
    distance: f64,
    height: f64,
}

const PROFILES: [Profile; 2] = [
    Profile {
        name: "frontal320",
        azimuth: 0.0,
        distance: 298.0,
        height: 0.0,
    },
    Profile {
        name: "oblicuo600",
        azimuth: 70.0 * DEG,
        distance: 578.0,
        height: 60.0,
    },
];

/// Mundo del juego con la criatura en el centro del Sol y el preview activo.
fn build(model: &Path, texture: &Path, light: SolarRevealLight) -> Result<World, String> {
    let mut world = World::new(
        load_model(&ship_model_path())?,
        SIZE.0 as f64 / SIZE.1 as f64,
    )?;
    world.toggles.freeze_orbits = true;
    let sun = world
        .system
        .bodies
        .iter()
        .find(|b| b.orbit.radius == 0.0)
        .ok_or("sin Sol")?
        .position(world.time);
    world.set_creature(StaticCreature::load(
        model,
        texture,
        CreaturePlacement::new(sun, CREATURE_RADIUS, 0.0)?,
    )?)?;
    world.enable_staged_creature_reveal(StagedRevealSettings::default(), CREATURE_EYE_REGION)?;
    world.enable_creature_eye_follow_targeting(EyeFollowSettings::default(), EyeTarget::Camera)?;
    world.enable_creature_horizontal_follow()?;
    world.enable_solar_reveal_preview(light)?;
    Ok(world)
}

/// Pose de perfil y 20 s reales de mirada y seguimiento hacia la camara.
fn place(world: &mut World, profile: &Profile) -> Result<(), String> {
    world.place_solar_reveal_camera(profile.azimuth, profile.distance, profile.height)?;
    for _ in 0..(20.0 / DT).round() as usize {
        world.update_creature_eye_follow(DT);
    }
    Ok(())
}

fn set_oxygen(world: &mut World, oxygen: f64) {
    world.debug_oxygen01 = oxygen;
    world.snap_creature_reveal();
}

struct Sheet {
    out: PathBuf,
    tsv: Vec<String>,
    json: Vec<String>,
}

impl Sheet {
    /// Dibuja (preview o, sin criatura, el cuadro base), guarda el PNG y anota la fila.
    #[allow(clippy::too_many_arguments)]
    fn shot(
        &mut self,
        world: &World,
        file: &str,
        kind: &str,
        profile: &str,
        light: &str,
        real_seconds: f64,
        baseline: bool,
    ) -> Result<(), String> {
        let mut fb = Framebuffer::new(SIZE.0, SIZE.1, SPACE_COLOR)?;
        let stats: RenderStats = if baseline {
            world.render(&mut fb)
        } else {
            world.render_solar_reveal_preview(&mut fb)?
        };
        save_png(&self.out.join(file), &fb)?;
        let alphas = world.solar_reveal_alphas();
        let (progress, sun, eye, body) = alphas.map_or((f64::NAN, 1.0, 0.0, 0.0), |a| {
            (a.progress, a.sun, a.eye, a.body)
        });
        let creature = world.creature().map(|c| c.placement());
        let body_yaw = world.creature_horizontal_follow().map_or(0.0, |f| f.yaw());
        let gaze = world.creature_eye_follow().map(|f| f.current());
        let (gaze_yaw, gaze_pitch) = gaze.map_or((0.0, 0.0), |g| (g.yaw(), g.pitch()));
        let cam = world.camera;
        let sun_body = &world.system.bodies[0];
        let center = sun_body.position(world.time);
        let row = [
            file.to_string(),
            kind.into(),
            profile.into(),
            light.into(),
            format!("{:.4}", world.debug_oxygen01),
            format!("{progress:.6}"),
            format!("{sun:.6}"),
            format!("{eye:.6}"),
            format!("{body:.6}"),
            format!("{real_seconds:.4}"),
            format!("{:.4}", world.time),
            format!("{}x{}", SIZE.0, SIZE.1),
            format!("{:.4}", cam.fov_y / DEG),
            vec3(cam.position),
            vec3(cam.target),
            format!("{:.3}", (cam.position - center).length()),
            creature.map_or("-".into(), |p| vec3(p.center())),
            creature.map_or("-".into(), |p| format!("{:.3}", p.world_radius())),
            creature.map_or("-".into(), |p| format!("{:.6}", p.yaw())),
            format!("{:.6}", body_yaw / DEG),
            format!("{:.6}", gaze_yaw / DEG),
            format!("{:.6}", gaze_pitch / DEG),
            format!("{:.6}", sun_body.rotation_angle(world.time) / DEG),
            format!("{}", stats.objects_submitted),
            format!("{}", stats.objects_culled),
            format!("{}", stats.fragments_evaluated),
        ];
        self.tsv.push(row.join("\t"));
        let keys = TSV_HEADER.split('\t');
        let fields: Vec<String> = keys
            .zip(&row)
            .map(|(k, v)| match v.parse::<f64>() {
                Ok(n) if n.is_finite() && !v.contains('x') => format!("\"{k}\": {v}"),
                _ => format!("\"{k}\": \"{v}\""),
            })
            .collect();
        self.json.push(format!("    {{{}}}", fields.join(", ")));
        Ok(())
    }
}

const TSV_HEADER: &str = "archivo\ttipo\tperfil\tluz\to2\tprogreso_mostrado\talfa_sol\talfa_ojo\talfa_cuerpo\tt_real_s\tt_mundo_s\tres\tfov_deg\tcamara_pos\tcamara_objetivo\tcamara_al_sol\tcriatura_centro\tcriatura_radio\tcriatura_yaw_colocacion\tcuerpo_yaw_02_deg\tojo_yaw_deg\tojo_pitch_deg\tsol_giro_axial_deg\tobjetos\tobjetos_descartados\tfragmentos";

fn vec3(v: Vec3) -> String {
    format!("{:.3},{:.3},{:.3}", v.x, v.y, v.z)
}

fn light_name(light: SolarRevealLight) -> &'static str {
    match light {
        SolarRevealLight::Camera => "camara(candidata)",
        SolarRevealLight::World => "mundo(baseline)",
    }
}

fn run() -> Result<(), String> {
    let (mut model, mut texture, mut out, mut light) = (None, None, None, SolarRevealLight::Camera);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("falta el valor de {a}"));
        match a.as_str() {
            "--creature-model" => model = Some(PathBuf::from(value()?)),
            "--creature-texture" => texture = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--light" => {
                light = match value()?.as_str() {
                    "camera" => SolarRevealLight::Camera,
                    "world" => SolarRevealLight::World,
                    other => return Err(format!("--light camera|world: {other:?}")),
                }
            }
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
    let mut sheet = Sheet {
        out,
        tsv: vec![TSV_HEADER.into()],
        json: Vec::new(),
    };
    let candidate = light_name(light);
    let mut world = build(&model, &texture, light)?;
    let mut plain = build(&model, &texture, light)?;
    plain.clear_creature();
    for profile in &PROFILES {
        place(&mut world, profile)?;
        (plain.ship, plain.camera) = (world.ship, world.camera);
        let file = format!("{}-baseline-sol.png", profile.name);
        sheet.shot(&plain, &file, "baseline", profile.name, "-", 0.0, true)?;
        for oxygen in LEVELS {
            set_oxygen(&mut world, oxygen);
            let file = format!("{}-o2-{:03}.png", profile.name, (oxygen * 100.0).round());
            sheet.shot(
                &world,
                &file,
                "estatico",
                profile.name,
                candidate,
                0.0,
                false,
            )?;
        }
    }
    // Temporal en el perfil frontal: bajada y subida suaves del O2 con segundos reales.
    let frontal = &PROFILES[0];
    place(&mut world, frontal)?;
    set_oxygen(&mut world, 0.34);
    let oxygen_at = |t: f64| {
        if t <= 8.0 {
            0.34 - 0.13 * t / 8.0
        } else {
            0.21 + 0.13 * (t - 8.0) / 8.0
        }
    };
    let mut trace = vec!["paso\tt_s\to2\tprogreso\tsol\tojo\tcuerpo\tdelta_sol".to_string()];
    let mut previous_sun = world.solar_reveal_alphas().map_or(1.0, |a| a.sun);
    let steps = (16.0 / DT).round() as usize;
    for step in 0..=steps {
        let t = step as f64 * DT;
        if step > 0 {
            world.debug_oxygen01 = oxygen_at(t);
            world.update_creature_reveal(DT);
            world.update_creature_eye_follow(DT);
        }
        let a = world.solar_reveal_alphas().ok_or("sin preview")?;
        trace.push(format!(
            "{step}\t{t:.4}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}",
            world.debug_oxygen01,
            a.progress,
            a.sun,
            a.eye,
            a.body,
            a.sun - previous_sun
        ));
        previous_sun = a.sun;
        if step % 30 == 0 {
            let file = format!("temporal-{}-t{:05.2}.png", frontal.name, t);
            sheet.shot(&world, &file, "temporal", frontal.name, candidate, t, false)?;
        }
    }
    std::fs::write(sheet.out.join("temporal-60hz.tsv"), trace.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    // Diagnosticos: planeta real delante y luz global.
    place(&mut world, frontal)?;
    let time = world.time;
    world.time = (std::f64::consts::FRAC_PI_2 - 0.6) / 0.060;
    for oxygen in [0.40, 0.05] {
        set_oxygen(&mut world, oxygen);
        let file = format!(
            "diag-planeta-delante-o2-{:03}.png",
            (oxygen * 100.0).round()
        );
        sheet.shot(
            &world,
            &file,
            "diagnostico",
            frontal.name,
            candidate,
            0.0,
            false,
        )?;
    }
    world.time = time;
    world.enable_solar_reveal_preview(SolarRevealLight::World)?;
    for oxygen in [0.30, 0.05] {
        set_oxygen(&mut world, oxygen);
        let file = format!("diag-luz-mundo-o2-{:03}.png", (oxygen * 100.0).round());
        let name = light_name(SolarRevealLight::World);
        sheet.shot(&world, &file, "diagnostico", frontal.name, name, 0.0, false)?;
    }
    std::fs::write(sheet.out.join("manifest.tsv"), sheet.tsv.join("\n") + "\n")
        .map_err(|e| e.to_string())?;
    let json = format!("[\n{}\n]\n", sheet.json.join(",\n"));
    std::fs::write(sheet.out.join("manifest.json"), json).map_err(|e| e.to_string())?;
    println!(
        "Preview solar: {} PNG en {}",
        sheet.json.len(),
        sheet.out.display()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
