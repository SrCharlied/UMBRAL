//! UMB-311 composicion OBSERVADORA del tentaculo en el Sol con ojo (debug opt-in). Criatura, captura,
//! tramo y correspondencia SINTETICOS publicos (la cupula calibrada del ojo de `tests/solar_game.rs` mas
//! un collar de cuerpo y un brazo tubo con caras intercaladas) y la nave PUBLICA en un `World` real con
//! `SolarGame`. Los assets reales y el oraculo Python son opt-in (`#[ignore]`, rutas por entorno).
// Bucles por indice sobre componentes xyz: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use std::process::Command;
use umbral::{
    assets::{
        obj::{Face, Model, load_model, parse_obj},
        ship_model_path,
    },
    math::{Mat4, Vec3},
    renderer::{framebuffer::Framebuffer, stats::RenderStats, texture::Texture},
    scene::{
        ship::ShipInput,
        world::{SHIP_MODEL_YAW, SPACE_COLOR, SolarBodyOverride, World},
    },
    visual::{
        solar_game::{SolarGame, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::StaticCreature,
        tentacle_growth_cpu::{
            GrowthPackage, GrowthPreview,
            capture::{fnv1a64, geometry_fnv},
        },
        tentacle_solar_game::*,
        watching_stars_game::{render_game_frame, render_game_frame_with},
    },
};

const W: usize = 320;
const H: usize = 180;
const FRAMES: usize = 52;
const FPS: usize = 4;
const GRIP: usize = 28;
const RINGS: usize = 8;
const SEGS: usize = 6;
const CUT: usize = 3;
const N: usize = 4;
const STAGES: [(usize, &str); 7] = [
    (1, "reposo"),
    (5, "recogimiento"),
    (9, "alcance"),
    (23, "cierre"),
    (29, "agarre"),
    (31, "arrastre"),
    (45, "final"),
];
const CAMERA: &str = "camera -1.2 0.2 2 0.5 -0.1 -0.8 0 1 0 0.6";
const TEXELS: [u8; 12] = [200, 200, 200, 120, 30, 160, 200, 200, 200, 120, 30, 160];
/// Delta OBJ dentro del dominio de la ley para el brazo +X sintetico.
const ALONG: [f64; 3] = [0.4, 0.05, 0.0];

fn ok<T>(r: Result<T, String>, what: &str) -> T {
    assert!(r.is_ok(), "{what}: {:?}", r.as_ref().err());
    r.unwrap()
}

// ---------------- criatura sintetica: cupula del ojo (de tests/solar_game.rs) + cuerpo + brazo ----------------

fn dome_source() -> String {
    let segments = 16;
    let mut s = String::from("vt 0.5 0.5\nv 0 0 1\nvn 0 0 1\n");
    let mut index = vec![vec![1usize]];
    let mut next = 2;
    for k in 1..=8 {
        let phi = (k as f64 * 10.0).to_radians();
        let mut ring = Vec::new();
        for j in 0..segments {
            let t = (j as f64 / segments as f64) * std::f64::consts::TAU;
            let (x, y, z) = (phi.sin() * t.cos(), phi.sin() * t.sin(), phi.cos());
            s += &format!("v {x} {y} {z}\nvn {x} {y} {z}\n");
            ring.push(next);
            next += 1;
        }
        index.push(ring);
    }
    let f = |a: usize, b: usize, c: usize| format!("f {a}/1/{a} {b}/1/{b} {c}/1/{c}\n");
    for j in 0..segments {
        s += &f(1, index[1][j], index[1][(j + 1) % segments]);
    }
    for k in 1..8 {
        for j in 0..segments {
            let (a, b) = (index[k][j], index[k][(j + 1) % segments]);
            let (c, d) = (index[k + 1][j], index[k + 1][(j + 1) % segments]);
            s += &f(a, c, d);
            s += &f(a, d, b);
        }
    }
    s
}

fn calibrated_source() -> String {
    let source = dome_source()
        + "v 1.6 -1.3 0.2\nv 1.7 -1.3 0.2\nv 1.6 -1.2 0.2\nf 130/1/1 131/1/1 132/1/1\n";
    let (c, s) = (
        CREATURE_EYE_REGION.center(),
        CREATURE_EYE_REGION.radius() / 0.85,
    );
    let mut out = String::new();
    for line in source.lines() {
        match line.strip_prefix("v ") {
            Some(rest) => {
                let p: Vec<f64> = rest.split(' ').map(|x| x.parse().unwrap()).collect();
                let q = c + (Vec3::new(p[0], p[1], p[2]) - Vec3::Z) * s;
                out += &format!("v {} {} {}\n", q.x, q.y, q.z);
            }
            None => out += &format!("{line}\n"),
        }
    }
    out
}

const DOME_VERTICES: usize = 132;
const DOME_FACES: usize = 241;

fn root_center() -> Vec3 {
    CREATURE_EYE_REGION.center() + Vec3::new(0.25, -0.30, 0.0)
}

fn ship_offset(k: usize) -> [f64; 3] {
    let t = k.saturating_sub(GRIP) as f64;
    [-0.01 * t, 0.004 * t, -0.008 * t]
}

/// Brazo tubo a lo largo de +X desde `root_center` (anillo 0 = raiz compartida con el collar).
fn tube(k: usize) -> Model {
    let reach = 0.004 * k.min(GRIP) as f64;
    let off = ship_offset(k);
    let r0 = root_center();
    let (mut vertices, mut normals, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
    for r in 0..RINGS {
        let u = r as f64 / (RINGS - 1) as f64;
        for s in 0..SEGS {
            let a = std::f64::consts::TAU * s as f64 / SEGS as f64;
            let (sa, ca) = a.sin_cos();
            vertices.push([
                r0.x + 0.08 * r as f64 + off[0] * u,
                r0.y + 0.03 * ca + reach * u * u + off[1] * u,
                r0.z + 0.03 * sa + off[2] * u,
            ]);
            normals.push([0.0, ca, sa]);
            uvs.push([s as f64 / SEGS as f64, u]);
        }
    }
    let mut faces = Vec::new();
    for r in 0..RINGS - 1 {
        for s in 0..SEGS {
            let (a, b) = (r * SEGS + s, r * SEGS + (s + 1) % SEGS);
            for tri in [[a, a + SEGS, b + SEGS], [a, b + SEGS, b]] {
                faces.push(Face {
                    vertex_indices: tri,
                    uv_indices: Some(tri),
                    normal_indices: Some(tri),
                    material: None,
                });
            }
        }
    }
    let mut m = Model {
        vertices,
        uvs,
        normals,
        faces,
        material_libraries: Vec::new(),
        materials: Default::default(),
        bounds_center: [0.0; 3],
        bounds_radius: 0.0,
    };
    m.update_bounds();
    m
}

/// OBJ de la criatura completa y, para cada cara del brazo, su indice en el modelo (intercalado con
/// las caras del collar para que el mapa no sea un rango).
fn creature_source() -> (String, Vec<usize>) {
    let mut s = calibrated_source();
    let arm = tube(1);
    for v in &arm.vertices {
        s += &format!("v {} {} {}\n", v[0], v[1], v[2]);
    }
    let center = root_center() + Vec3::new(-0.03, 0.0, 0.0);
    s += &format!("v {} {} {}\n", center.x, center.y, center.z);
    let collar_center = DOME_VERTICES + arm.vertices.len() + 1; // 1-based
    let collar: Vec<String> = (0..SEGS)
        .map(|k| {
            let (a, b) = (DOME_VERTICES + k + 1, DOME_VERTICES + (k + 1) % SEGS + 1);
            format!("f {collar_center}/1/1 {b}/1/1 {a}/1/1\n")
        })
        .collect();
    let mut arm_faces = Vec::new();
    let mut next_face = DOME_FACES;
    let mut collar_iter = collar.iter();
    for (k, f) in arm.faces.iter().enumerate() {
        let [a, b, c] = f.vertex_indices.map(|i| DOME_VERTICES + i + 1);
        s += &format!("f {a}/1/1 {b}/1/1 {c}/1/1\n");
        arm_faces.push(next_face);
        next_face += 1;
        if k % 5 == 4
            && let Some(line) = collar_iter.next()
        {
            s += line;
            next_face += 1;
        }
    }
    for line in collar_iter {
        s += line;
    }
    (s, arm_faces)
}

fn creature_model() -> Model {
    parse_obj(&creature_source().0).unwrap()
}

fn texture() -> Texture {
    Texture::from_rgb8(2, 2, TEXELS.to_vec()).unwrap()
}

fn ship_model() -> Model {
    load_model(&ship_model_path()).unwrap()
}

fn ship_obj(k: usize) -> Mat4 {
    let s = creature_model().bounds_radius / 80.0;
    let o = ship_offset(k);
    let r0 = root_center();
    let mut m = Mat4::rotation_y(SHIP_MODEL_YAW) * Mat4::scaling(Vec3::new(s, s, s));
    m.m[0][3] = r0.x + 0.62 + o[0];
    m.m[1][3] = r0.y + o[1];
    m.m[2][3] = r0.z + o[2];
    m
}

fn manifest() -> String {
    let ship = ship_model();
    let c = ship.bounds_center;
    let cm = creature_model();
    let a = tube(1);
    let list = |name: &str, v: Vec<usize>| {
        let ids: Vec<String> = v.iter().map(|i| i.to_string()).collect();
        format!("{name} {} {}", v.len(), ids.join(" "))
    };
    let mut l = vec![
        "UMB311_CAPTURE_PACKAGE 1".to_string(),
        format!("fps {FPS}"),
        format!("frames {FRAMES}"),
        format!("arm_counts {} {}", a.vertices.len(), a.faces.len()),
        format!("body_counts {} {}", a.vertices.len(), a.faces.len()),
        format!("grip_frame {GRIP}"),
        format!(
            "creature_bounds {} {} {} {}",
            cm.bounds_center[0], cm.bounds_center[1], cm.bounds_center[2], cm.bounds_radius
        ),
        format!(
            "ship_bounds {} {} {} {}",
            c[0], c[1], c[2], ship.bounds_radius
        ),
        format!("ship_geometry {:016x}", geometry_fnv(&ship)),
        CAMERA.to_string(),
    ];
    l.extend(STAGES.iter().map(|(s, n)| format!("stage {s} {n}")));
    l.push(list("root", (0..SEGS).collect()));
    l.push(list("grip", ((RINGS - 1) * SEGS..RINGS * SEGS).collect()));
    l.push("file body body/cuerpo_presentacion.obj 1 0000000000000001".into());
    for k in 1..=FRAMES {
        l.push(format!("file arm arm/brazo{k:04}.obj 1 0000000000000001"));
    }
    for k in 1..=FRAMES {
        let m = ship_obj(k);
        let mut s = format!("ship {k}");
        for r in 0..3 {
            for c in 0..4 {
                s += &format!(" {}", m.m[r][c]);
            }
        }
        l.push(s);
    }
    l.push("end".into());
    l.join("\n") + "\n"
}

fn pidx(r: usize, s: usize) -> usize {
    r * SEGS + s
}
fn didx(r: usize, s: usize) -> usize {
    (CUT + 1) * SEGS + (r - CUT) * SEGS + s
}
fn gidx(j: usize, k: usize) -> usize {
    (CUT + 1) * SEGS + (RINGS - CUT) * SEGS + (j - 1) * SEGS + k
}
fn ring_index(j: usize, k: usize) -> usize {
    if j == 0 {
        pidx(CUT, k)
    } else if j == N {
        didx(CUT, k)
    } else {
        gidx(j, k)
    }
}

/// Paquete de crecimiento sintetico (mismo formato que el exportador real).
fn growth_text(capture: &str) -> String {
    let a = tube(1);
    let mut l = vec![
        "UMB311_GROWTH_PACKAGE 1".to_string(),
        format!(
            "capture_package_fnv1a64 {:016x}",
            fnv1a64(capture.as_bytes())
        ),
        format!("topology_sha256 {}", "a".repeat(64)),
        format!("law_sha256 {}", "b".repeat(64)),
        format!("approval_sha256 {}", "c".repeat(64)),
        format!("frames {FRAMES}"),
    ];
    let nv = (CUT + 1) * SEGS + (RINGS - CUT) * SEGS + (N - 1) * SEGS;
    l.push(format!(
        "counts {nv} {} {SEGS} {N}",
        a.faces.len() + 2 * SEGS * N
    ));
    for r in 0..=CUT {
        for s in 0..SEGS {
            let kind = if r == CUT { "orig_on_cut" } else { "orig" };
            l.push(format!("vertex {kind} P {}", r * SEGS + s));
        }
    }
    for r in CUT..RINGS {
        for s in 0..SEGS {
            let kind = if r == CUT { "orig_on_cut" } else { "orig" };
            l.push(format!("vertex {kind} D {}", r * SEGS + s));
        }
    }
    for j in 1..N {
        for k in 0..SEGS {
            l.push(format!("vertex ring {j} {k}"));
        }
    }
    let ids = |v: Vec<usize>| {
        v.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    l.push(format!(
        "loop_p {}",
        ids((0..SEGS).map(|k| pidx(CUT, k)).collect())
    ));
    l.push(format!(
        "loop_d {}",
        ids((0..SEGS).map(|k| didx(CUT, k)).collect())
    ));
    for j in 0..=N {
        l.push(format!(
            "ring {j} {}",
            ids((0..SEGS).map(|k| ring_index(j, k)).collect())
        ));
    }
    for (fi, f) in a.faces.iter().enumerate() {
        let v = f.vertex_indices;
        let r = v.iter().map(|i| i / SEGS).min().unwrap();
        let (part, map): (&str, fn(usize, usize) -> usize) =
            if r < CUT { ("P", pidx) } else { ("D", didx) };
        let tv: Vec<usize> = v.iter().map(|&i| map(i / SEGS, i % SEGS)).collect();
        let uv: Vec<String> = v
            .iter()
            .flat_map(|&i| a.uvs[i])
            .map(|x| format!("{x:?}"))
            .collect();
        l.push(format!(
            "face {part} {} {} {} {} c:{fi}:0 c:{fi}:1 c:{fi}:2",
            tv[0],
            tv[1],
            tv[2],
            uv.join(" ")
        ));
    }
    for j in 0..N {
        for k in 0..SEGS {
            let k1 = (k + 1) % SEGS;
            for tri in [
                [(k1, j), (k, j), (k, j + 1)],
                [(k1, j), (k, j + 1), (k1, j + 1)],
            ] {
                let v: Vec<String> = tri
                    .iter()
                    .map(|&(k, j)| ring_index(j, k).to_string())
                    .collect();
                let uv: Vec<String> = tri
                    .iter()
                    .flat_map(|&(k, j)| [k as f64 / SEGS as f64, j as f64 / N as f64])
                    .map(|x| format!("{x:?}"))
                    .collect();
                let src: Vec<String> = tri.iter().map(|&(k, j)| format!("g:{j}:{k}")).collect();
                l.push(format!(
                    "face G {} {} {}",
                    v.join(" "),
                    uv.join(" "),
                    src.join(" ")
                ));
            }
        }
    }
    let mask = |name: &str, v: Vec<usize>| format!("mask {name} {} {}", v.len(), ids(v));
    l.push(mask("root", (0..SEGS).map(|s| pidx(0, s)).collect()));
    l.push(mask(
        "grip",
        (0..SEGS).map(|s| didx(RINGS - 1, s)).collect(),
    ));
    l.push(mask(
        "coil",
        (0..SEGS).map(|s| didx(RINGS - 2, s)).collect(),
    ));
    for f in 1..=FRAMES {
        l.push(format!("guide {f} 1.0 0.0 0.0"));
    }
    l.push("end".into());
    l.join("\n") + "\n"
}

fn correspondence(capture: &str, growth: &str) -> String {
    let (_, arm_faces) = creature_source();
    let m = creature_model();
    let n = RINGS * SEGS;
    let ids = |v: Vec<usize>| {
        v.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    [
        "UMB311_ARM_CORRESPONDENCE 1".to_string(),
        format!(
            "capture_package_fnv1a64 {:016x}",
            fnv1a64(capture.as_bytes())
        ),
        format!("growth_package_fnv1a64 {:016x}", fnv1a64(growth.as_bytes())),
        format!("meshy_geometry_fnv {:016x}", geometry_fnv(&m)),
        format!("meshy_obj_sha256 {}", "d".repeat(64)),
        format!("neutral_obj_sha256 {}", "e".repeat(64)),
        format!(
            "counts {} {} {n} {}",
            m.vertices.len(),
            m.faces.len(),
            arm_faces.len()
        ),
        format!(
            "arm_vertex {}",
            ids((0..n).map(|i| DOME_VERTICES + i).collect())
        ),
        format!("arm_face {}", ids(arm_faces)),
        "end".to_string(),
    ]
    .join("\n")
        + "\n"
}

fn texts() -> (String, String, String) {
    let m = manifest();
    let g = growth_text(&m);
    let c = correspondence(&m, &g);
    (m, g, c)
}

fn debug_from(corr: &str) -> Result<SolarCaptureDebug, String> {
    let (m, g, _) = texts();
    SolarCaptureDebug::from_parts(&m, (1..=FRAMES).map(tube).collect(), tube(1), &g, corr)
}

fn debug() -> SolarCaptureDebug {
    ok(debug_from(&texts().2), "synthetic capture debug")
}

/// Juego solar con la criatura `model` (bounds ORIGINALES impuestos si se piden).
fn game_with(model: Model, oxygen: f64) -> (World, SolarGame) {
    let mut world = World::new(ship_model(), W as f64 / H as f64).unwrap();
    world.debug_oxygen01 = oxygen;
    let placement = solar_creature_placement(&world).unwrap();
    let creature = StaticCreature::new(model, texture(), placement).unwrap();
    let game = ok(SolarGame::start(&mut world, creature), "solar start");
    (world, game)
}

fn game(oxygen: f64) -> (World, SolarGame) {
    game_with(creature_model(), oxygen)
}

fn bound(oxygen: f64) -> (World, SolarGame, SolarCaptureDebug) {
    let (w, g) = game(oxygen);
    let mut d = debug();
    ok(d.bind(&w), "bind");
    (w, g, d)
}

/// Coloca la nave en `referencia_actual + C_actual * delta_obj` (preparacion de la prueba, no la API).
fn place_ship(world: &mut World, d: &SolarCaptureDebug, delta_obj: [f64; 3]) {
    let c = world.creature_render_transform().unwrap().matrix();
    let reference = (c * d.package().capture().manifest().ship_obj_matrix(1).unwrap()).column(3);
    world.ship.position = reference + c.transform_dir(Vec3::from_array(delta_obj));
}

fn frame(
    world: &World,
    game: &SolarGame,
    watching: bool,
    body: Option<&SolarBodyOverride<'_>>,
) -> (Framebuffer, RenderStats) {
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let f = ok(
        render_game_frame_with(world, &mut fb, Some(game), watching, body),
        "frame",
    );
    (fb, f.stats)
}

fn seeded() -> Framebuffer {
    let mut fb = Framebuffer::new(W, H, 0x123456).unwrap();
    for (i, p) in fb.pixels.iter_mut().enumerate() {
        *p = ((i * 7919) & 0xffffff) as u32;
    }
    for i in 0..200 {
        fb.write(
            (i * 13 % W) as isize,
            (i * 7 % H) as isize,
            -0.5 + i as f64 * 0.001,
            0xabcdef,
        );
    }
    fb
}

fn depth_signature(fb: &Framebuffer) -> Vec<u8> {
    let thresholds = [-0.45, 0.0, 0.5, 0.9, 0.99, 0.999, 0.9999, 1.0];
    (0..fb.height)
        .flat_map(|y| (0..fb.width).map(move |x| (x, y)))
        .map(|(x, y)| {
            thresholds.iter().enumerate().fold(0u8, |acc, (i, &t)| {
                acc | (u8::from(fb.depth_passes(x as isize, y as isize, t)) << i)
            })
        })
        .collect()
}

fn world_state(w: &World) -> String {
    format!(
        "{:?} {:?} {:?} {:?} {:?} {} {} {:?}",
        w.ship.position,
        w.ship.orientation,
        w.camera.position,
        w.camera.target,
        w.camera.view_projection(),
        w.debug_oxygen01,
        w.time,
        w.creature_render_transform().map(|t| t.matrix())
    )
}

/// Criatura de REFERENCIA: el modelo completo SIN las caras del brazo y CON el tramo evaluado anadido,
/// con los bounds ORIGINALES. En su juego solar el brazo es literalmente parte del cuerpo.
fn reference_model(full: &Model, d: &SolarCaptureDebug) -> Model {
    let arm: std::collections::HashSet<usize> =
        d.correspondence().arm_face().iter().copied().collect();
    let g = d.growth_mesh();
    let (nv, nuv, nn) = (full.vertices.len(), full.uvs.len(), full.normals.len());
    let material = full.faces[d.correspondence().arm_face()[0]]
        .material
        .clone();
    let mut faces: Vec<Face> = full
        .faces
        .iter()
        .enumerate()
        .filter(|(i, _)| !arm.contains(i))
        .map(|(_, f)| Face {
            vertex_indices: f.vertex_indices,
            uv_indices: f.uv_indices,
            normal_indices: f.normal_indices,
            material: f.material.clone(),
        })
        .collect();
    for f in &g.faces {
        faces.push(Face {
            vertex_indices: f.vertex_indices.map(|i| i + nv),
            uv_indices: f.uv_indices.map(|u| u.map(|i| i + nuv)),
            normal_indices: f.normal_indices.map(|n| n.map(|i| i + nn)),
            material: material.clone(),
        });
    }
    Model {
        vertices: full.vertices.iter().chain(&g.vertices).copied().collect(),
        uvs: full.uvs.iter().chain(&g.uvs).copied().collect(),
        normals: full.normals.iter().chain(&g.normals).copied().collect(),
        faces,
        material_libraries: full.material_libraries.clone(),
        materials: full.materials.clone(),
        bounds_center: full.bounds_center,
        bounds_radius: full.bounds_radius,
    }
}

// =====================================================================================================

#[test]
fn correspondence_and_cli_flags_are_strict() {
    let (m, g, c) = texts();
    ok(ArmCorrespondence::parse(&c), "valid");
    let edit = |from: &str, to: &str| c.replacen(from, to, 1);
    let arm_face_line = c
        .lines()
        .find(|l| l.starts_with("arm_face "))
        .unwrap()
        .to_string();
    let tokens = arm_face_line
        .split(' ')
        .map(String::from)
        .collect::<Vec<_>>();
    let mut dup = tokens.clone();
    dup[2] = dup[1].clone();
    let mut far = tokens.clone();
    far[1] = "99999999".into();
    let bad = [
        (
            "magic",
            edit("UMB311_ARM_CORRESPONDENCE 1", "UMB311_ARM_CORRESPONDENCE 2"),
        ),
        ("crlf", c.replace('\n', "\r\n")),
        ("after end", c.clone() + "x\n"),
        ("counts", edit("counts ", "counts 0 ")),
        ("duplicate face", c.replace(&arm_face_line, &dup.join(" "))),
        ("out of range", c.replace(&arm_face_line, &far.join(" "))),
        ("sha", edit(&"d".repeat(64), "xyz")),
    ];
    for (name, text) in bad {
        assert!(ArmCorrespondence::parse(&text).is_err(), "{name}: rejected");
    }
    // vinculo con OTRA captura u OTRO paquete de crecimiento
    let wrong_growth = correspondence(&m, &(g.clone() + " "));
    assert!(debug_from(&wrong_growth).is_err(), "growth FNV binding");
    let wrong_capture = correspondence(&(m.clone() + " "), &g);
    assert!(debug_from(&wrong_capture).is_err(), "capture FNV binding");
    // flags: sin el flag nada; opciones sueltas, sin Sol, legacy, rutas faltantes y reloj invalido -> Err
    assert_eq!(
        validate_tentacle_capture_flags(&TentacleCaptureFlags::default()),
        Ok(None)
    );
    let dir = std::env::temp_dir();
    let file = std::env::current_exe().unwrap();
    let full = TentacleCaptureFlags {
        enabled: true,
        capture_dir: Some(dir.clone()),
        growth_package: Some(file.clone()),
        correspondence: Some(file.clone()),
        start_time: None,
        solar: true,
        legacy: false,
    };
    assert!(validate_tentacle_capture_flags(&full).unwrap().is_some());
    let cases = [
        (
            "option without flag",
            TentacleCaptureFlags {
                enabled: false,
                ..full.clone()
            },
            "requieren --tentacle-capture-debug",
        ),
        (
            "no solar",
            TentacleCaptureFlags {
                solar: false,
                ..full.clone()
            },
            "requiere --creature-solar-reveal",
        ),
        (
            "legacy",
            TentacleCaptureFlags {
                legacy: true,
                ..full.clone()
            },
            "--legacy",
        ),
        (
            "no package",
            TentacleCaptureFlags {
                capture_dir: None,
                ..full.clone()
            },
            "--tentacle-capture-package",
        ),
        (
            "no growth",
            TentacleCaptureFlags {
                growth_package: None,
                ..full.clone()
            },
            "--tentacle-growth-package",
        ),
        (
            "no correspondence",
            TentacleCaptureFlags {
                correspondence: None,
                ..full.clone()
            },
            "--tentacle-arm-correspondence",
        ),
        (
            "package is a file",
            TentacleCaptureFlags {
                capture_dir: Some(file.clone()),
                ..full.clone()
            },
            "no es una carpeta",
        ),
        (
            "nan time",
            TentacleCaptureFlags {
                start_time: Some(f64::NAN),
                ..full.clone()
            },
            "segundos finitos",
        ),
        (
            "negative time",
            TentacleCaptureFlags {
                start_time: Some(-1.0),
                ..full.clone()
            },
            "segundos finitos",
        ),
    ];
    for (name, flags, needle) in cases {
        let e = validate_tentacle_capture_flags(&flags).unwrap_err();
        assert!(e.contains(needle), "{name}: {e}");
    }
}

#[test]
fn bind_verifies_provenance_and_builds_one_body_layer() {
    let (w, _g) = game(0.05);
    let mut d = debug();
    assert!(d.body_override().is_none());
    let r = ok(d.bind(&w), "bind");
    let arm_faces = 2 * SEGS * (RINGS - 1);
    assert_eq!(r.arm_faces_removed, arm_faces);
    assert_eq!(
        r.shared_root_vertices, SEGS,
        "only the root ring is shared with the body"
    );
    assert_eq!(r.max_position_error, 0.0, "synthetic arm is bit-equal");
    let parts = w.staged_creature_parts().unwrap();
    assert_eq!(r.eye_faces, parts.eye().faces.len());
    let growth_faces = d.package().data().template().faces.len();
    assert_eq!(
        r.composite_faces,
        parts.body().faces.len() - arm_faces + growth_faces
    );
    let composite = d.composite().unwrap();
    assert_eq!(
        composite.vertices[..parts.body().vertices.len()],
        parts.body().vertices[..],
        "body arrays are an exact prefix"
    );
    assert_eq!(
        w.creature().unwrap().model().faces.len(),
        creature_model().faces.len(),
        "the World creature is untouched"
    );
    // procedencia rota: otra criatura, otra cara (winding), bounds distintos -> Err y sin enlazar
    let mut moved = creature_model();
    moved.vertices[DOME_VERTICES + 7][1] += 1e-3;
    let (w2, _) = game_with(moved, 0.05);
    let mut d2 = debug();
    assert!(
        d2.bind(&w2).unwrap_err().contains("huella"),
        "other geometry"
    );
    assert!(d2.bind_report().is_none());
    let (_, g, c) = texts();
    let _ = g;
    let line = c
        .lines()
        .find(|l| l.starts_with("arm_face "))
        .unwrap()
        .to_string();
    let mut f: Vec<String> = line.split(' ').map(String::from).collect();
    f.swap(1, 2);
    let mut d3 = ok(debug_from(&c.replace(&line, &f.join(" "))), "parse swapped");
    assert!(
        d3.bind(&w).unwrap_err().contains("no coincide"),
        "swapped arm faces"
    );
    let (mut w4, _) = game(0.05);
    w4.clear_creature();
    assert!(debug().bind(&w4).is_err(), "no creature");
}

#[test]
fn inactive_frames_are_exactly_the_original_frames() {
    for oxygen in [1.0, 0.5, 0.49, 0.25, 0.05, 0.0] {
        let (w, g, mut d) = bound(oxygen);
        assert_eq!(
            ok(d.update_frame(&w), "update"),
            None,
            "inactive: nothing to prepare"
        );
        assert!(d.body_override().is_none());
        for watching in [false, true] {
            let mut a = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
            let fa = ok(
                render_game_frame(&w, &mut a, Some(&g), watching),
                "original",
            );
            let (mut b, sb) = frame(&w, &g, watching, d.body_override().as_ref());
            assert!(
                a.pixels == b.pixels && depth_signature(&a) == depth_signature(&b),
                "O2 {oxygen}: same pixels/depth"
            );
            assert_eq!(fa.stats, sb, "O2 {oxygen}: same stats");
            let mut c = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
            assert_eq!(
                ok(g.render(&w, &mut c), "render"),
                ok(g.render_with(&w, &mut b, None), "render_with")
            );
            assert!(c.pixels == b.pixels);
        }
    }
}

#[test]
fn the_arm_is_drawn_inside_the_body_layer_with_body_alpha() {
    for oxygen in [0.25, 0.05] {
        for clock in [3.6, 6.9, 8.4, 13.0] {
            let (mut w, g, mut d) = bound(oxygen);
            place_ship(&mut w, &d, ALONG);
            ok(d.activate(&w), "activate");
            ok(d.set_clock(clock), "clock");
            let status = ok(d.update_frame(&w), "update").unwrap();
            assert!(d.body_override().is_some());
            let (fa, sa) = frame(&w, &g, false, d.body_override().as_ref());
            // Referencia: el brazo es parte de la criatura (misma capa Body, alfa, shader, transform)
            let reference = reference_model(&creature_model(), &d);
            let (mut rw, rg) = game_with(reference, oxygen);
            rw.ship.position = w.ship.position;
            let (fr, sr) = frame(&rw, &rg, false, None);
            assert!(
                fa.pixels == fr.pixels,
                "O2 {oxygen} t {clock} f{}: same pixels as the creature with the arm in its body",
                status.frame
            );
            assert_eq!(depth_signature(&fa), depth_signature(&fr));
            // Unica diferencia admitida y exacta: en la referencia la capa OJO tambien copia los vertices
            // anadidos del tramo (las particiones copian todos los vertices del modelo); se cuentan como
            // procesados aunque ninguna cara del ojo los use. Todo lo demas, identico.
            let extra = d.growth_mesh().vertices.len();
            assert_eq!(
                RenderStats {
                    vertices_processed: sa.vertices_processed + extra,
                    ..sa
                },
                sr,
                "same complete stats"
            );
            let (fo, _) = frame(&w, &g, false, None);
            if oxygen < 0.5 && status.reach_weight > 0.0 {
                assert!(
                    fa.pixels != fo.pixels,
                    "O2 {oxygen} t {clock}: the extended arm is visible"
                );
            }
        }
    }
}

#[test]
fn high_oxygen_keeps_creature_and_arm_invisible() {
    for oxygen in [1.0, 0.5] {
        let (mut w, g, mut d) = bound(oxygen);
        place_ship(&mut w, &d, ALONG);
        ok(d.activate(&w), "activate");
        ok(d.set_clock(6.9), "clock");
        ok(d.update_frame(&w), "update");
        let (fa, sa) = frame(&w, &g, true, d.body_override().as_ref());
        let (fb, sb) = frame(&w, &g, true, None);
        assert!(
            fa.pixels == fb.pixels && sa == sb,
            "O2 {oxygen}: eye hidden -> no arm either"
        );
    }
}

#[test]
fn the_extended_arm_survives_culling_and_occludes_watching_stars() {
    let (mut w, g, mut d) = bound(0.05);
    place_ship(&mut w, &d, [2.5, 0.2, 0.0]);
    ok(d.activate(&w), "activate");
    let hold = d.timeline().unwrap().reach_window().1 + 0.2;
    ok(d.set_clock(hold), "grip hold");
    ok(d.update_frame(&w), "update");
    let o = d.body_override().unwrap();
    let t = w.creature_render_transform().unwrap();
    let tip = t.matrix().transform_point(o.extra_bounds_obj.0);
    let (center, radius) = (
        w.creature().unwrap().placement().center(),
        w.creature().unwrap().placement().world_radius(),
    );
    // Camara mirando al tramo extendido con la criatura FUERA del frustum
    let away = (tip - center).normalized();
    w.camera.position = center + away * (radius + 30.0);
    w.camera.target = w.camera.position + away * 100.0;
    assert!(
        !w.camera.frustum().sphere_visible(center, radius),
        "creature sphere out of view"
    );
    let (with_arm, s_arm) = frame(&w, &g, false, Some(&o));
    let (without, s_none) = frame(&w, &g, false, None);
    assert!(
        s_arm.objects_culled < s_none.objects_culled,
        "the appendix keeps the body layer from being culled"
    );
    assert!(
        s_arm.triangles_rasterized > s_none.triangles_rasterized
            && with_arm.pixels != without.pixels
    );
    // estrellas observadoras: el brazo escribe profundidad ANTES de los ojos
    let mut fa = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let a = ok(
        render_game_frame_with(&w, &mut fa, Some(&g), true, Some(&o)),
        "stars with arm",
    );
    let mut fb = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let b = ok(
        render_game_frame_with(&w, &mut fb, Some(&g), true, None),
        "stars without arm",
    );
    let (ra, rb) = (a.watching_stars.unwrap(), b.watching_stars.unwrap());
    assert!(rb.occluded_ids.iter().all(|i| ra.occluded_ids.contains(i)));
    assert!(
        ra.occluded_ids.len() > rb.occluded_ids.len(),
        "{} vs {} occluded stars",
        ra.occluded_ids.len(),
        rb.occluded_ids.len()
    );
}

#[test]
fn the_capture_never_writes_the_world_and_input_stays_free() {
    let (mut w, g, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    let before = world_state(&w);
    let snap = ok(d.activate(&w), "activate");
    ok(d.advance(0.5), "advance");
    ok(d.update_frame(&w), "update");
    let _ = frame(&w, &g, true, d.body_override().as_ref());
    assert_eq!(
        before,
        world_state(&w),
        "ship, camera, O2, clock and creature untouched"
    );
    // input real: la nave se mueve libre mientras la composicion sigue; el objetivo NO la persigue
    let input = ShipInput {
        thrust: 1.0,
        yaw: 0.5,
        pitch: 0.0,
        roll: 0.0,
        boost: false,
    };
    for _ in 0..30 {
        w.update(&input, 1.0 / 30.0);
        ok(d.advance(1.0 / 30.0), "advance");
        ok(d.update_frame(&w), "update");
    }
    assert!(
        (w.ship.position - snap.target_world).length() > 1.0,
        "the ship flew away freely"
    );
    assert_eq!(
        d.snapshot().unwrap().target_world,
        snap.target_world,
        "snapshot target, no pursuit"
    );
    assert_eq!(d.snapshot().unwrap().delta_obj, snap.delta_obj);
}

#[test]
fn the_timeline_matches_the_accepted_growth_plan() {
    let (m, gtext, _) = texts();
    let package = ok(
        GrowthPackage::from_parts(&m, (1..=FRAMES).map(tube).collect(), tube(1), &gtext),
        "package",
    );
    let mut preview = ok(GrowthPreview::new(package, || Ok(texture())), "preview");
    for delta in [ALONG, [1.6, 0.2, 0.0], [0.1, 0.01, 0.0]] {
        let mut w = World::new(ship_model(), W as f64 / H as f64).unwrap();
        let (r, o) = preview.reference_pose();
        w.ship.position = r + preview.obj_to_world(delta);
        w.ship.orientation = o;
        let plan = ok(preview.plan(&w), "accepted plan");
        let base = ok(
            capture_base_timing(preview.capture_package().manifest()),
            "base",
        );
        let tl = ok(
            CaptureTimeline::new(
                base,
                FPS,
                FRAMES,
                plan.distance(),
                plan.reference_distance(),
            ),
            "timeline",
        );
        assert_eq!(
            (
                tl.factor(),
                tl.duration_s(),
                tl.reach_window(),
                tl.carry_window()
            ),
            (
                plan.factor(),
                plan.duration_s(),
                plan.reach_window(),
                plan.carry_window()
            )
        );
        let (ra, rc) = (plan.reach_window(), plan.carry_window());
        let mut times: Vec<f64> = (0..=4000)
            .map(|i| plan.duration_s() * 1.2 * i as f64 / 4000.0)
            .collect();
        times.extend([ra.0, ra.1, rc.0, rc.1, plan.duration_s(), 1e9, 0.0]);
        for t in times {
            let s = ok(plan.sample(t), "plan sample");
            assert_eq!(
                ok(tl.sample(t), "timeline"),
                (s.base_seconds, s.frame),
                "t {t}"
            );
        }
        for f in 1..=FRAMES {
            assert_eq!(
                ok(preview.data().offset(f, plan.delta_obj()), "law"),
                *ok(plan.law(f), "plan law")
            );
        }
    }
    assert!(
        CaptureTimeline::new(
            capture_base_timing(preview.capture_package().manifest()).unwrap(),
            FPS,
            FRAMES,
            f64::NAN,
            1.0
        )
        .is_err()
    );
}

#[test]
fn out_of_domain_and_overflow_activations_change_nothing() {
    let (mut w, _g, mut d) = bound(0.05);
    for (name, delta, needle) in [
        ("lateral contact", [0.0, 0.3, 0.0], "agarre"),
        ("antiparallel", [-0.3, 0.0, 0.0], "antiparalelo"),
    ] {
        place_ship(&mut w, &d, delta);
        let e = d.activate(&w).unwrap_err();
        assert!(e.contains(needle), "{name}: {e}");
        assert!(
            !d.is_active() && d.body_override().is_none(),
            "{name}: still inactive"
        );
    }
    for (name, p) in [
        ("nan", Vec3::new(f64::NAN, 0.0, 0.0)),
        ("beyond 1e9", Vec3::new(0.0, 2e9, 0.0)),
        ("overflow", Vec3::new(1e308, 1e308, 0.0)),
    ] {
        w.ship.position = p;
        let e = d.toggle(&w).unwrap_err();
        assert!(e.contains("dominio"), "{name}: {e}");
        assert!(!d.is_active(), "{name}");
    }
    let mut fresh = debug();
    assert!(
        fresh.activate(&w).unwrap_err().contains("bind"),
        "unbound cannot activate"
    );
}

#[test]
fn real_seconds_clock_holds_the_last_frame_and_f7_cancels() {
    let (mut w, g, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    assert!(matches!(
        ok(d.toggle(&w), "F7"),
        CaptureToggle::Activated(_)
    ));
    for bad in [f64::NAN, -0.1, f64::INFINITY] {
        assert!(d.advance(bad).is_err());
        assert_eq!(d.clock(), Some(0.0), "clock untouched by {bad}");
    }
    assert!(d.set_clock(f64::NAN).is_err());
    ok(d.advance(0.75), "advance");
    let s = ok(d.update_frame(&w), "update").unwrap();
    assert_eq!((s.clock, s.frame), (0.75, 4), "floor(0.75 * 4) + 1");
    let duration = d.timeline().unwrap().duration_s();
    ok(d.set_clock(duration + 50.0), "beyond the end");
    let s = ok(d.update_frame(&w), "hold").unwrap();
    assert_eq!(s.frame, FRAMES, "hold on the last frame");
    assert!(matches!(ok(d.toggle(&w), "F7"), CaptureToggle::Cancelled));
    assert!(d.body_override().is_none() && ok(d.update_frame(&w), "inactive").is_none());
    let (a, sa) = frame(&w, &g, true, d.body_override().as_ref());
    let (b, sb) = frame(&w, &g, true, None);
    assert!(
        a.pixels == b.pixels && sa == sb,
        "cancel restores the original composition"
    );
}

#[test]
fn override_rejections_are_atomic() {
    let (mut w, g, mut d) = bound(0.05);
    place_ship(&mut w, &d, ALONG);
    ok(d.activate(&w), "activate");
    ok(d.set_clock(6.9), "clock");
    ok(d.update_frame(&w), "update");
    let good = d.composite().unwrap();
    let (b0, n0) = (
        w.staged_creature_parts().unwrap().body().vertices.len(),
        good.normals.len(),
    );
    let mut cases: Vec<(&str, Model)> = Vec::new();
    let clone = |m: &Model| Model {
        vertices: m.vertices.clone(),
        uvs: m.uvs.clone(),
        normals: m.normals.clone(),
        faces: m
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: m.material_libraries.clone(),
        materials: m.materials.clone(),
        bounds_center: m.bounds_center,
        bounds_radius: m.bounds_radius,
    };
    let mut stale = clone(good);
    stale.vertices[3][0] += 1e-9;
    cases.push(("stale prefix", stale));
    let mut nan = clone(good);
    nan.vertices[b0 + 2][1] = f64::NAN;
    cases.push(("nan tail", nan));
    let mut nan_normal = clone(good);
    nan_normal.normals[n0 - 1][2] = f64::NAN;
    cases.push(("nan normal", nan_normal));
    let mut index = clone(good);
    index.faces.last_mut().unwrap().vertex_indices[0] = 1_000_000;
    cases.push(("index", index));
    let mut no_uv = clone(good);
    no_uv.faces[0].uv_indices = None;
    cases.push(("no uv", no_uv));
    for (name, model) in &cases {
        let o = SolarBodyOverride {
            model,
            extra_bounds_obj: (Vec3::ZERO, 0.1),
        };
        let mut fb = seeded();
        let (p, dsig) = (fb.pixels.clone(), depth_signature(&fb));
        let before = world_state(&w);
        assert!(
            render_game_frame_with(&w, &mut fb, Some(&g), true, Some(&o)).is_err(),
            "{name}: Err"
        );
        assert!(
            fb.pixels == p && depth_signature(&fb) == dsig,
            "{name}: color and depth untouched"
        );
        assert_eq!(before, world_state(&w));
    }
    for (name, bounds) in [
        ("nan sphere", (Vec3::new(f64::NAN, 0.0, 0.0), 1.0)),
        ("negative radius", (Vec3::ZERO, -1.0)),
    ] {
        let o = SolarBodyOverride {
            model: good,
            extra_bounds_obj: bounds,
        };
        let mut fb = seeded();
        let p = fb.pixels.clone();
        assert!(
            w.render_solar_reveal_preview_with(&mut fb, Some(&o))
                .is_err(),
            "{name}"
        );
        assert!(fb.pixels == p, "{name}: untouched");
    }
    let o = d.body_override().unwrap();
    let mut fb = seeded();
    let p = fb.pixels.clone();
    assert!(
        render_game_frame_with(&w, &mut fb, None, false, Some(&o)).is_err(),
        "body layer needs the solar game"
    );
    assert!(fb.pixels == p);
}

#[test]
fn the_cli_rejects_capture_flags_before_any_io() {
    let out =
        std::env::temp_dir().join(format!("umbral_tentacle_guard_{}.png", std::process::id()));
    for (args, needle) in [
        (
            vec!["--tentacle-capture-debug"],
            "requiere --creature-solar-reveal",
        ),
        (
            vec!["--tentacle-capture-time", "1"],
            "requieren --tentacle-capture-debug",
        ),
        (
            vec!["--tentacle-capture-package", "x"],
            "requieren --tentacle-capture-debug",
        ),
        (vec!["--tentacle-capture-debug", "--legacy"], "--legacy"),
        (
            vec!["--tentacle-capture-time", "nan", "--tentacle-capture-debug"],
            "segundos finitos",
        ),
    ] {
        let o = Command::new(env!("CARGO_BIN_EXE_umbral"))
            .args(["--headless", "--output"])
            .arg(&out)
            .args(&args)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(
            !o.status.success() && err.contains(needle),
            "{args:?}: {err}"
        );
        assert!(!out.exists(), "{args:?}: no output written");
    }
    let help = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&help.stdout)
            .contains("--tentacle-capture-debug (requiere --creature-solar-reveal)")
    );
}

// ============================ assets REALES y oraculo Python (opt-in) ============================

fn env_path(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k}")))
}

fn real_request() -> TentacleCaptureRequest {
    TentacleCaptureRequest {
        capture_dir: env_path("UMBRAL_CAPTURE_PACKAGE_DIR"),
        growth_package: env_path("UMBRAL_GROWTH_PACKAGE"),
        correspondence: env_path("UMBRAL_ARM_CORRESPONDENCE"),
        start_time: None,
    }
}

/// Meshy REAL + textura 1024 aceptada + paquetes reales: procedencia (2827 caras, raiz 27, ojo sin
/// brazo, desvio de export medido), activacion desde la vista FRONTAL del juego, equivalencia con la
/// criatura de referencia en el juego solar real y cuadro inactivo identico.
#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_assets_bind_and_compose_in_the_solar_game() {
    let meshy = load_model(&env_path("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
    let real_texture = || Texture::load_png(&env_path("UMBRAL_CREATURE_TEXTURE")).unwrap();
    let new_game = |model: Model| {
        let mut world = World::new(ship_model(), W as f64 / H as f64).unwrap();
        world.debug_oxygen01 = 0.05;
        let placement = solar_creature_placement(&world).unwrap();
        let creature = StaticCreature::new(model, real_texture(), placement).unwrap();
        let game = SolarGame::start(&mut world, creature).unwrap();
        (world, game)
    };
    let clone = |m: &Model| Model {
        vertices: m.vertices.clone(),
        uvs: m.uvs.clone(),
        normals: m.normals.clone(),
        faces: m
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: m.material_libraries.clone(),
        materials: m.materials.clone(),
        bounds_center: m.bounds_center,
        bounds_radius: m.bounds_radius,
    };
    let (w, g) = new_game(clone(&meshy));
    let mut d = ok(SolarCaptureDebug::load(&real_request()), "load");
    let r = ok(d.bind(&w), "bind");
    println!("bind {r:?}");
    assert_eq!(
        (
            r.meshy_vertices,
            r.meshy_faces,
            r.arm_faces_removed,
            r.shared_root_vertices
        ),
        (15464, 31096, 2827, 27)
    );
    assert!(
        r.max_position_error > 0.0 && r.max_position_error <= EXPORT_POSITION_TOLERANCE,
        "export quantum, NOT bit exact: {}",
        r.max_position_error
    );
    let (inactive, si) = frame(&w, &g, true, None);
    let mut base = Framebuffer::new(W, H, SPACE_COLOR).unwrap();
    let sb = render_game_frame(&w, &mut base, Some(&g), true)
        .unwrap()
        .stats;
    assert!(
        inactive.pixels == base.pixels && si == sb,
        "bound but inactive = original frame"
    );
    let snap = ok(d.activate(&w), "F7 from the FRONTAL view");
    println!(
        "snapshot frontal: delta_obj {:?} factor {:.4} duration {:.3} x168 {:?} clamped {}",
        snap.delta_obj,
        snap.factor,
        snap.duration_s,
        snap.cone_fraction_at_grip,
        snap.clamped_frames
    );
    for clock in [4.0, snap.duration_s * 0.45, snap.duration_s * 0.8] {
        ok(d.set_clock(clock), "clock");
        let s = ok(d.update_frame(&w), "update").unwrap();
        let (fa, sa) = frame(&w, &g, false, d.body_override().as_ref());
        let (mut rw, rg) = new_game(reference_model(&meshy, &d));
        rw.ship.position = w.ship.position;
        let (fr, sr) = frame(&rw, &rg, false, None);
        println!(
            "t {clock:.3} f{} r {:.3} rasterized {}",
            s.frame, s.reach_weight, sa.triangles_rasterized
        );
        let extra = d.growth_mesh().vertices.len();
        assert!(
            fa.pixels == fr.pixels,
            "t {clock}: arm composed exactly as part of the real body layer"
        );
        assert_eq!(
            RenderStats {
                vertices_processed: sa.vertices_processed + extra,
                ..sa
            },
            sr,
            "t {clock}: stats (eye layer of the reference copies the appended vertices)"
        );
    }
}

/// Consumidor del oraculo Python de 936 cuadros (`UMBRAL_GROWTH_ORACLE_DIR`) contra el modulo COPIADO en
/// Root: posiciones de todos los cuadros y normales/centros/R_j de los cuadros clave, bit a bit.
#[test]
#[ignore = "paquete privado + oraculo: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_GROWTH_ORACLE_DIR"]
fn real_growth_oracle_is_bit_exact_through_the_root_module() {
    use std::io::Read;
    let pkg = ok(
        GrowthPackage::load(
            &env_path("UMBRAL_CAPTURE_PACKAGE_DIR"),
            &env_path("UMBRAL_GROWTH_PACKAGE"),
        ),
        "package",
    );
    let data = pkg.data();
    let dir = env_path("UMBRAL_GROWTH_ORACLE_DIR");
    let index = std::fs::read_to_string(dir.join("oracle_index.txt")).unwrap();
    let key: Vec<usize> = index
        .lines()
        .nth(3)
        .unwrap()
        .split(' ')
        .skip(1)
        .map(|x| x.parse().unwrap())
        .collect();
    let cases: Vec<(String, [f64; 3])> = index
        .lines()
        .filter(|l| l.starts_with("case "))
        .map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (
                t[1].to_string(),
                [
                    t[2].parse().unwrap(),
                    t[3].parse().unwrap(),
                    t[4].parse().unwrap(),
                ],
            )
        })
        .collect();
    let offsets: std::collections::HashMap<(String, usize), [f64; 3]> = index
        .lines()
        .filter(|l| l.starts_with("frame "))
        .map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (
                (t[1].to_string(), t[2].parse().unwrap()),
                [
                    t[4].parse().unwrap(),
                    t[5].parse().unwrap(),
                    t[6].parse().unwrap(),
                ],
            )
        })
        .collect();
    assert_eq!(cases.len(), 3);
    let mut out = Model {
        vertices: data.template().vertices.clone(),
        uvs: data.template().uvs.clone(),
        normals: data.template().normals.clone(),
        faces: data
            .template()
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: Vec::new(),
        materials: Default::default(),
        bounds_center: [0.0; 3],
        bounds_radius: 0.0,
    };
    let read = |r: &mut std::io::BufReader<std::fs::File>, n: usize| {
        let mut b = vec![0u8; 8 * n];
        r.read_exact(&mut b).unwrap();
        b.chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
            .collect::<Vec<f64>>()
    };
    let (nv, nc) = (out.vertices.len(), out.normals.len());
    let (mut values, mut mismatches) = (0usize, 0usize);
    for (name, delta) in &cases {
        let mut fp = std::io::BufReader::new(
            std::fs::File::open(dir.join(format!("positions_{name}.f64le"))).unwrap(),
        );
        let mut fk = std::io::BufReader::new(
            std::fs::File::open(dir.join(format!("keyframes_{name}.f64le"))).unwrap(),
        );
        for f in 1..=data.frames() {
            let pose = ok(data.evaluate(f, *delta, &mut out), "evaluate");
            let p = read(&mut fp, 3 * nv);
            let o = offsets[&(name.clone(), f)];
            for (a, b) in out
                .vertices
                .iter()
                .flatten()
                .zip(&p)
                .chain(pose.law.offset.iter().zip(&o))
            {
                values += 1;
                mismatches += usize::from(a.to_bits() != b.to_bits());
            }
            if key.contains(&f) {
                let k = read(&mut fk, nc * 3 + 65 * 3 + 65 * 4);
                let ours: Vec<f64> = out
                    .normals
                    .iter()
                    .flatten()
                    .chain(pose.centers.iter().flatten())
                    .chain(pose.rotations.iter().flatten())
                    .copied()
                    .collect();
                for (a, b) in ours.iter().zip(&k) {
                    values += 1;
                    mismatches += usize::from(a.to_bits() != b.to_bits());
                }
            }
        }
    }
    println!("ORACLE root module: values {values} bit_mismatches {mismatches}");
    assert!(values > 13_000_000 && mismatches == 0);
}

// Parent-owned review regressions; the complete original test file is preserved above.
#[path = "support/solar_capture_review_regressions.rs"]
mod parent_review_regressions;

#[path = "support/solar_capture_fix_supplement.rs"]
mod fix_supplement;

#[path = "support/solar_capture_input_graph.rs"]
mod input_graph;
