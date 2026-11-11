// COPIA LITERAL (bytes exactos) de tests/tentacle_live_pursuit.rs lineas 1469-1951 de baseline-source
// (archivo SHA-256 4a7044ff2e2194fd88eb198cd8bb148890c669072d17718ac27453cb55cd88bd; bloque SHA-256 bbc284aaa7954e14f04d997963eac313f2ab0d385c89e5c65329a26ad099cb13).
// Incluido con include! desde tests/tentacle_spatial_pursuit.rs; el archivo heredado no se edita.
// =====================================================================================================
// FIXTURE SINTETICO: COPIA LITERAL de tests/tentacle_solar_game.rs (lineas 33-515 del FIX02, salvo `bound`,
// sin uso aqui), sin editar
// el archivo heredado. Criatura con cupula calibrada + collar + brazo tubo +X, captura, tramo y
// correspondencia publicos.
// =====================================================================================================

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
