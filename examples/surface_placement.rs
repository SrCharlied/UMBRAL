//! Vista tecnica de UMB-305, sin ventana: marcadores direccionales anclados a una esfera
//! que se traslada, escala y gira en Y. No son props definitivos ni un planeta aprobado.
//! Ver `docs/procedural.md`.
//!
//! `cargo run --release --example surface_placement -- --angle 1`
//! Opciones: `--angle RAD --seed N --output archivo.png`
//! (por defecto `renders/smoke/surface_placement_<angulo>.png`).

use std::{collections::BTreeMap, path::PathBuf};
use umbral::{
    assets::{
        obj::{Face, Model},
        primitives::uv_sphere,
    },
    config::{DEFAULT_HEIGHT, DEFAULT_WIDTH},
    debug::overlay::draw_text,
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer,
        image::save_png,
        pipeline::draw_model,
        shader::{Material, ShaderContext, ShaderKind},
    },
    scene::{camera::Camera, world::SPACE_COLOR},
    visual::{
        procedural_surface::{DistributionSettings, distribute},
        surface_placement::{PropPlacement, SurfaceAnchor},
    },
};

const PLANET_CENTER: Vec3 = Vec3::new(3.0, -1.0, -2.0);
const PLANET_RADIUS: f64 = 1.6;
/// Giro reproducible por muestra: angulo aureo por indice de intento.
const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

/// Pieza convexa del marcador: vertices, caras y nombre de material.
type MarkerPart<'a> = (&'a [[f64; 3]], &'a [[usize; 3]], &'a str);

fn value<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<T, String> {
    args.next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| format!("{flag} necesita un valor valido"))
}

/// Marcador de diagnostico de altura 1 con el pie en y = 0: obelisco claro, cuna naranja
/// hacia +Z local y aleta cian hacia +X local.
fn direction_marker() -> Model {
    let mut model = Model {
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        faces: Vec::new(),
        material_libraries: Vec::new(),
        materials: BTreeMap::from([
            ("cuerpo".to_string(), [0.85, 0.85, 0.8]),
            ("frente".to_string(), [1.0, 0.4, 0.05]),
            ("lado".to_string(), [0.1, 0.8, 0.9]),
        ]),
        bounds_center: [0.0; 3],
        bounds_radius: 1.0,
    };
    let (w, t) = (0.12, 0.06);
    let obelisk = [
        [-w, 0.0, -w],
        [w, 0.0, -w],
        [w, 0.0, w],
        [-w, 0.0, w],
        [-t, 0.75, -t],
        [t, 0.75, -t],
        [t, 0.75, t],
        [-t, 0.75, t],
        [0.0, 1.0, 0.0],
    ];
    let obelisk_faces = [
        [0, 2, 1],
        [0, 3, 2],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
        [4, 5, 8],
        [5, 6, 8],
        [6, 7, 8],
        [7, 4, 8],
    ];
    let tetra = [[0, 1, 2], [0, 1, 3], [1, 2, 3], [2, 0, 3]];
    let front = [
        [-0.1, 0.45, 0.06],
        [0.1, 0.45, 0.06],
        [0.0, 0.65, 0.06],
        [0.0, 0.55, 0.45],
    ];
    let side = [
        [0.08, 0.2, -0.07],
        [0.08, 0.2, 0.07],
        [0.08, 0.38, 0.0],
        [0.3, 0.29, 0.0],
    ];
    let parts: [MarkerPart; 3] = [
        (&obelisk, &obelisk_faces, "cuerpo"),
        (&front, &tetra, "frente"),
        (&side, &tetra, "lado"),
    ];
    for (vertices, faces, material) in parts {
        let offset = model.vertices.len();
        let centroid = vertices
            .iter()
            .fold(Vec3::ZERO, |acc, &v| acc + Vec3::from_array(v))
            / vertices.len() as f64;
        model.vertices.extend_from_slice(vertices);
        for &[a, b, c] in faces {
            let p = |i: usize| Vec3::from_array(vertices[i]);
            // Cada pieza es convexa: la normal plana debe alejarse de su centroide.
            let outward = (p(b) - p(a)).cross(p(c) - p(a)).dot(p(a) - centroid) > 0.0;
            let [a, b, c] = if outward { [a, b, c] } else { [a, c, b] };
            model.faces.push(Face {
                vertex_indices: [offset + a, offset + b, offset + c],
                uv_indices: None,
                normal_indices: None,
                material: Some(material.to_string()),
            });
        }
    }
    model.update_bounds();
    model
}

fn main() -> Result<(), String> {
    let (mut angle, mut seed, mut output) = (0.0_f64, 24531_u64, None);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--angle" => angle = value(&mut args, &flag)?,
            "--seed" => seed = value(&mut args, &flag)?,
            "--output" => output = Some(PathBuf::from(value::<String>(&mut args, &flag)?)),
            _ => return Err(format!("Argumento desconocido: {flag}")),
        }
    }
    if !angle.is_finite() {
        return Err("--angle debe ser finito".into());
    }
    let output = output
        .unwrap_or_else(|| PathBuf::from(format!("renders/smoke/surface_placement_{angle}.png")));

    // Anclajes locales: se crean una vez y no dependen del angulo del planeta.
    let mut anchors = Vec::new();
    for normal in [Vec3::Y, -Vec3::Y] {
        anchors.push(SurfaceAnchor::new(
            normal,
            PropPlacement {
                twist: 0.0,
                elevation: 0.0,
                scale: 0.3,
            },
        )?);
    }
    for k in 0..12 {
        let (sin, cos) = (f64::from(k) * std::f64::consts::TAU / 12.0).sin_cos();
        anchors.push(SurfaceAnchor::new(
            Vec3::new(cos, 0.0, sin),
            PropPlacement {
                twist: 0.0,
                elevation: 0.0,
                scale: 0.2,
            },
        )?);
    }
    let samples = distribute(&DistributionSettings {
        seed,
        attempts: 256,
        noise_scale: 2.5,
        threshold: 0.55,
        min_spacing: 0.35,
    })?;
    for s in &samples {
        anchors.push(SurfaceAnchor::new(
            s.normal,
            PropPlacement {
                twist: f64::from(s.attempt) * GOLDEN_ANGLE,
                elevation: 0.0,
                scale: 0.14,
            },
        )?);
    }

    let planet = Transform {
        position: PLANET_CENTER,
        rotation: Vec3::new(0.0, angle, 0.0),
        scale: Vec3::splat(PLANET_RADIUS),
    };
    let (width, height) = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
    let mut fb = Framebuffer::new(width, height, SPACE_COLOR)?;
    let mut camera = Camera::new(40f64.to_radians(), width as f64 / height as f64, 0.1, 100.0)?;
    camera.position = PLANET_CENTER + Vec3::new(0.0, 0.9, 4.2) * PLANET_RADIUS;
    camera.target = PLANET_CENTER;
    let context = ShaderContext {
        elapsed_time: 0.0,
        oxygen01: 1.0,
        camera_position: camera.position,
        ship_position: camera.position,
        light_position: PLANET_CENTER + Vec3::new(4.0, 5.0, 6.0) * PLANET_RADIUS,
    };
    let planet_material = Material {
        cull_backfaces: true,
        ..Material::solid(0.22, 0.23, 0.26)
    };
    draw_model(
        &mut fb,
        &uv_sphere(48, 96),
        &planet,
        &planet_material,
        ShaderKind::LitPlanet,
        &context,
        &camera,
    );
    let marker = direction_marker();
    let marker_material = Material {
        ambient: 0.35,
        ..Material::from_model()
    };
    let (mut contact_error, mut up_error, mut draw_error) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut pixels = 0;
    for anchor in &anchors {
        let exact = anchor.world_matrix(&planet)?;
        let contact = exact.transform_point(Vec3::ZERO);
        let radial = contact - PLANET_CENTER;
        contact_error = contact_error.max((radial.length() - PLANET_RADIUS).abs());
        up_error = up_error.max((exact.column(1).normalized() - radial.normalized()).length());
        let transform = anchor.world_transform(&planet)?;
        let drawn = transform.matrix();
        for i in 0..3 {
            for j in 0..4 {
                draw_error = draw_error.max((drawn.m[i][j] - exact.m[i][j]).abs());
            }
        }
        pixels += draw_model(
            &mut fb,
            &marker,
            &transform,
            &marker_material,
            ShaderKind::LitPlanet,
            &context,
            &camera,
        )
        .pixels_written;
    }

    let lines = [
        "UMB-305 VISTA TECNICA - NO ES ARTE FINAL".to_string(),
        format!(
            "ANGULO {angle:.3} RAD | SEMILLA {seed} | MARCADORES {}",
            anchors.len()
        ),
        "NARANJA = +Z LOCAL | CIAN = +X LOCAL | PIE EN Y = 0".to_string(),
    ];
    for (i, line) in lines.iter().enumerate() {
        draw_text(&mut fb, 16, 16 + i * 22, 2, 0xC8D2D2, line);
    }
    save_png(&output, &fb)?;

    println!(
        "Angulo {angle} rad | semilla {seed} | marcadores {} (2 polos, 12 ecuador, {} sembrados)",
        anchors.len(),
        samples.len()
    );
    println!(
        "Error max: contacto {contact_error:.2e} | eje arriba {up_error:.2e} | Transform vs matriz {draw_error:.2e}"
    );
    println!("Pixeles de marcadores: {pixels}");
    println!("PNG: {} ({width}x{height})", output.display());
    Ok(())
}
