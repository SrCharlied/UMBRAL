//! UMB-311 PERSECUCION ESPACIAL 3D del tentaculo (variante opt-in). Pruebas NUEVAS de la ruta espacial:
//! nucleo puro (`SpatialRig`), puertos EXACTOS de LP-R01/LP-R02 con control RED nativo del solucionador
//! plano en el mismo fixture, garantias reales portadas (ver TEST_CONTRACT_MIGRATION.json), cadena
//! nave -> World -> persecucion -> cuadro y linea de comandos. Fixture sintetico: copia literal en
//! `tests/support/spatial_pursuit_fixture.rs`; los assets reales son opt-in (`#[ignore]`).
// Bucles por indice sobre componentes xyz: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use umbral::{
    app::advance_frame,
    app::{GameOptions, PerceptionRoute, run_game_with_tentacle_spatial_pursuit},
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
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
        staged_creature_reveal::CREATURE_EYE_REGION,
        static_creature::{CreatureOptions, StaticCreature},
        tentacle_growth_cpu::{
            GrowthVertex, Part, Side, VertexSource,
            capture::{fnv1a64, geometry_fnv},
        },
        tentacle_live_pursuit::*,
        tentacle_solar_game::*,
        tentacle_spatial_pursuit::*,
        watching_stars_game::render_game_frame_with,
    },
};

fn copy_model(m: &Model) -> Model {
    Model {
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
    }
}

// ---------------- nucleo puro: deformacion espacial sobre la topologia sintetica ----------------

type V = [f64; 3];

fn v3(a: V) -> Vec3 {
    Vec3::from_array(a)
}

fn rig() -> (SolarCaptureDebug, SpatialRig) {
    let d = debug();
    let r = ok(
        SpatialRig::new(d.package().data(), &tube(1).vertices),
        "rig espacial",
    );
    (d, r)
}

/// El solucionador PLANO heredado, intacto, sobre el MISMO fixture (control RED nativo de LP-R01/R02).
fn legacy_rig(d: &SolarCaptureDebug) -> ShaftRig {
    ok(
        ShaftRig::new(d.package().data(), &tube(1).vertices),
        "rig plano",
    )
}

fn mem(side: V, tip: V) -> SpatialMemory {
    SpatialMemory { side, tip_obj: tip }
}

/// Posiciones de reposo EXACTAS calculadas aparte (Orig = brazo; Cut = a + (b - a) t).
fn exact_rest(d: &SolarCaptureDebug) -> Vec<Option<V>> {
    let arm = tube(1).vertices;
    d.package()
        .data()
        .topology()
        .vertices()
        .iter()
        .map(|v| match v {
            GrowthVertex::Retained { source, .. } => Some(match *source {
                VertexSource::Orig { orig, .. } => arm[orig],
                VertexSource::Cut { a, b, t } => {
                    std::array::from_fn(|k| arm[a][k] + (arm[b][k] - arm[a][k]) * t)
                }
            }),
            GrowthVertex::Ring { .. } => None,
        })
        .collect()
}

fn buffers(d: &SolarCaptureDebug) -> (Vec<V>, Vec<V>) {
    let t = d.package().data().template();
    (
        vec![[7.0; 3]; t.vertices.len()],
        vec![[7.0; 3]; t.normals.len()],
    )
}

/// Rotacion rigida que lleva el distal de reposo al deformado (marco de tres vertices D).
fn distal_rotation(d: &SolarCaptureDebug, rest: &[V], out: &[V]) -> [Vec3; 3] {
    let topo = d.package().data().topology();
    let ids: Vec<usize> = topo
        .vertices()
        .iter()
        .enumerate()
        .filter(|(_, v)| matches!(v, GrowthVertex::Retained { side: Side::D, .. }))
        .map(|(i, _)| i)
        .collect();
    let (i0, i1, i2) = (ids[0], ids[ids.len() / 3], ids[2 * ids.len() / 3]);
    let frame = |p: &[V]| {
        let e1 = (v3(p[i1]) - v3(p[i0])).normalized();
        let e2 = (v3(p[i2]) - v3(p[i0]) - e1 * (v3(p[i2]) - v3(p[i0])).dot(e1)).normalized();
        [e1, e2, e1.cross(e2)]
    };
    let (a, b) = (frame(rest), frame(out));
    let col = |x: Vec3| b[0] * a[0].dot(x) + b[1] * a[1].dot(x) + b[2] * a[2].dot(x);
    [col(Vec3::X), col(Vec3::Y), col(Vec3::Z)]
}

fn apply(r: &[Vec3; 3], x: Vec3) -> Vec3 {
    r[0] * x.x + r[1] * x.y + r[2] * x.z
}

fn max_jump(a: &[V], b: &[V]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (v3(*a) - v3(*b)).length())
        .fold(0.0, f64::max)
}

#[test]
fn sp01_rest_tip_reproduces_the_exact_rest_arm() {
    let (d, rig) = rig();
    let data = d.package().data();
    let topo = data.topology();
    let exact = exact_rest(&d);
    let mut reference = copy_model(data.template());
    ok(
        data.evaluate(1, [0.0; 3], &mut reference).map(|_| ()),
        "ley o = 0",
    );
    // Con o sin memoria (cualquier lado): el reposo no depende de ella.
    for memory in [
        None,
        Some(mem([0.0, 0.0, 1.0], rig.rest_tip())),
        Some(mem([0.0, -0.6, 0.8], [0.0; 3])),
    ] {
        let (mut v, mut n) = buffers(&d);
        let pose = ok(rig.deform(rig.rest_tip(), memory, &mut v, &mut n), "reposo");
        for (i, e) in exact.iter().enumerate() {
            if let Some(e) = e {
                assert_eq!(v[i], *e, "vertice retenido {i} EXACTO");
            }
        }
        let lp = topo.loop_p();
        for ring in &topo.rings()[1..topo.rings().len() - 1] {
            for (k, &vi) in ring.iter().enumerate() {
                assert_eq!(v[vi], v[lp[k]], "anillo colapsado en el lazo P");
            }
        }
        assert_eq!(n, reference.normals, "normales de reposo de la ley");
        assert_eq!(pose.shaft_length, 0.0);
        assert_eq!(
            (pose.first_elbow, pose.second_elbow, pose.turn),
            (0.0, 0.0, 0.0)
        );
        assert_eq!(v3(pose.axial_tip), v3(rig.rest_tip()), "punta de reposo");
    }
    let rest = rig.rest_loop();
    assert!((v3(rest.tangent) - Vec3::X).length() < 1e-9);
    assert!(rig.tip_offset() >= 2.0 * rig.bend_radius() && rig.bend_radius() > rest.contour_radius);
    // Dominio declarado: el radio exacto espacial es el plano mas 2 rho (el primer codo).
    let (rho, a) = (rig.bend_radius(), rig.tip_offset());
    assert!((rig.min_radius() - (3.0 * rho + (rho * rho + a * a).sqrt())).abs() < 1e-15);
    assert!(rig.min_radius() < MAX_REACH_OBJ);
}

/// Verifica una pose: punta exacta, proximal bit a bit, distal rigido con la rotacion publicada,
/// anillos rigidos, caras G sanas sin invertir respecto al tubo recto y normales unitarias.
fn check_pose(
    d: &SolarCaptureDebug,
    rig: &SpatialRig,
    what: &str,
    tip: Vec3,
    memory: Option<SpatialMemory>,
) -> SpatialPose {
    let topo = d.package().data().topology();
    let rest = rig.rest_loop();
    let (cp, t, a) = (v3(rest.center), v3(rest.tangent), rig.tip_offset());
    let (mut rv, mut rn) = buffers(d);
    ok(rig.deform(rig.rest_tip(), None, &mut rv, &mut rn), "reposo");
    let faces = topo.faces();
    let straight = cp + t * 1.5;
    let (mut sv, mut sn) = buffers(d);
    ok(
        rig.deform([straight.x, straight.y, straight.z], None, &mut sv, &mut sn),
        "recto",
    );
    let orientation = |v: &[V], n: &[V], fi: usize| {
        let f = &faces[fi];
        let [p0, p1, p2] = f.v.map(|i| v3(v[i]));
        let g = (p1 - p0).cross(p2 - p0);
        let c = v3(n[3 * fi]) + v3(n[3 * fi + 1]) + v3(n[3 * fi + 2]);
        (g.length(), g.dot(c) > 0.0)
    };
    let (mut v, mut n) = buffers(d);
    let pose = ok(
        rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
        what,
    );
    assert!(
        (v3(pose.axial_tip) - tip).length() < 1e-9,
        "{what}: punta alcanzada"
    );
    assert!(
        pose.shaft_length > 0.0 && pose.line_length >= a - 1e-12,
        "{what}: recta >= a"
    );
    for (i, tv) in topo.vertices().iter().enumerate() {
        if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
            assert_eq!(v[i], rv[i], "{what}: proximal {i}");
        }
    }
    let r = distal_rotation(d, &rv, &v);
    let published = pose.end_rotation;
    let cd = v3(pose.distal_center);
    for (i, tv) in topo.vertices().iter().enumerate() {
        if matches!(tv, GrowthVertex::Retained { side: Side::D, .. }) {
            let expect = cd + apply(&r, v3(rv[i]) - cp);
            assert!(
                (v3(v[i]) - expect).length() < 1e-9,
                "{what}: distal {i} rigido"
            );
            let x = v3(rv[i]) - cp;
            let by_pose = cd
                + Vec3::new(
                    v3(published[0]).dot(x),
                    v3(published[1]).dot(x),
                    v3(published[2]).dot(x),
                );
            assert!(
                (v3(v[i]) - by_pose).length() < 1e-9,
                "{what}: distal {i} = rotacion publicada"
            );
        }
    }
    assert!(
        (cd + apply(&r, t) * a - tip).length() < 1e-9,
        "{what}: tangente final a la punta"
    );
    let lp = topo.loop_p();
    for ring in &topo.rings()[1..topo.rings().len() - 1] {
        for k in 0..ring.len() {
            for k1 in k + 1..ring.len() {
                let l = (v3(v[ring[k]]) - v3(v[ring[k1]])).length();
                let l0 = (v3(rv[lp[k]]) - v3(rv[lp[k1]])).length();
                assert!((l - l0).abs() < 1e-9, "{what}: anillo rigido");
            }
        }
    }
    for (fi, f) in faces.iter().enumerate() {
        if f.part != Part::G {
            continue;
        }
        let (area, sign) = orientation(&v, &n, fi);
        assert!(area > 1e-10, "{what}: cara G {fi} degenerada");
        assert_eq!(
            sign,
            orientation(&sv, &sn, fi).1,
            "{what}: cara G {fi} invertida respecto al tubo recto"
        );
    }
    assert!(
        n.iter().all(|x| (v3(*x).length() - 1.0).abs() < 1e-6),
        "{what}: normales unitarias"
    );
    pose
}

#[test]
fn sp02_tip_is_reached_in_every_direction_with_rigid_rings_and_distal() {
    let (d, rig) = rig();
    let cp = v3(rig.rest_loop().center);
    let dirs = [
        Vec3::X,
        -Vec3::X,
        Vec3::Y,
        -Vec3::Y,
        Vec3::Z,
        -Vec3::Z,
        Vec3::new(1.0, 1.0, 1.0).normalized(),
        Vec3::new(-1.0, 0.3, -0.5).normalized(),
    ];
    // Las mismas 8 direcciones x 3 distancias de S3 sin memoria.
    for dir in dirs {
        for dist in [0.5, 1.5, 4.0] {
            check_pose(
                &d,
                &rig,
                &format!("dir {dir:?} dist {dist}"),
                cp + dir * dist,
                None,
            );
        }
    }
    // Memoria ADVERSARIA (lado arbitrario, tambien opuesto a la punta) dentro del dominio declarado.
    let sides = [
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.6, -0.8],
    ];
    for dir in dirs {
        for dist in [rig.min_radius(), 1.5, 4.0, 11.5] {
            for side in sides {
                let tip = cp + dir * dist;
                let what = format!("dir {dir:?} dist {dist} memoria {side:?}");
                let pose = check_pose(&d, &rig, &what, tip, Some(mem(side, [tip.x, tip.y, tip.z])));
                assert!(pose.singular_margin > 0.0, "{what}");
            }
        }
    }
}

#[test]
fn sp03_invalid_tips_and_memory_change_nothing() {
    let (d, rig) = rig();
    let rest = rig.rest_loop();
    let (cp, t) = (v3(rest.center), v3(rest.tangent));
    let inside = cp + t * (0.5 * rig.tip_offset());
    let beyond = cp + t * (MAX_REACH_OBJ * 1.01);
    for tip in [
        [f64::NAN, 0.0, 0.0],
        [f64::INFINITY, 0.0, 0.0],
        [inside.x, inside.y, inside.z],
        [beyond.x, beyond.y, beyond.z],
    ] {
        let (mut v, mut n) = buffers(&d);
        assert!(rig.deform(tip, None, &mut v, &mut n).is_err(), "{tip:?}");
        assert!(
            v.iter().chain(&n).all(|x| *x == [7.0; 3]),
            "{tip:?}: sin escribir"
        );
    }
    let good = cp - Vec3::X * 1.5;
    let good = [good.x, good.y, good.z];
    for bad in [
        mem([f64::NAN, 1.0, 0.0], good),
        mem([0.0, f64::INFINITY, 0.0], good),
        mem([0.0, 0.0, 0.0], good),
        mem([1.0, 0.0, 0.0], good),
        mem([0.0, 1.0, 0.0], [f64::NAN, 0.0, 0.0]),
        mem([0.0, 1.0, 0.0], [0.0, f64::NEG_INFINITY, 0.0]),
    ] {
        let (mut v, mut n) = buffers(&d);
        assert!(
            rig.deform(good, Some(bad), &mut v, &mut n).is_err(),
            "{bad:?}"
        );
        assert!(
            v.iter().chain(&n).all(|x| *x == [7.0; 3]),
            "{bad:?}: sin escribir"
        );
    }
    let (mut v, mut n) = buffers(&d);
    v.pop();
    assert!(
        rig.deform(rig.rest_tip(), None, &mut v, &mut n).is_err(),
        "buffer corto"
    );
    assert!(v.iter().chain(&n).all(|x| *x == [7.0; 3]));
    let mut bad_arm = tube(1).vertices;
    bad_arm[3][1] = f64::NAN;
    assert!(
        SpatialRig::new(d.package().data(), &bad_arm).is_err(),
        "reposo no finito"
    );
    assert!(
        SpatialRig::new(d.package().data(), &bad_arm[..4]).is_err(),
        "brazo corto"
    );
}

/// LP-R01 sobre un solucionador: mismas entradas, eps, ganancia y medida que el regresor del padre.
fn lp_r01_violations(
    d: &SolarCaptureDebug,
    cp: Vec3,
    mut solve: impl FnMut(Vec3, Vec3) -> (Vec<V>, Vec<V>, bool),
) -> Vec<(f64, f64, f64, bool)> {
    let mut violations = Vec::new();
    for eps in [1e-2_f64, 1e-3, 1e-4, 1e-5] {
        let a = cp + Vec3::new(-eps.cos(), eps.sin(), 0.0);
        let b = cp + Vec3::new(-eps.cos(), 0.0, eps.sin());
        let (av, bv, switched) = solve(a, b);
        let target_step = (b - a).length();
        let rings = d.package().data().topology().rings();
        let jump = rings[1..rings.len() - 1]
            .iter()
            .flatten()
            .map(|&i| (v3(av[i]) - v3(bv[i])).length())
            .fold(0.0, f64::max);
        if jump > 8.0 * target_step + 1e-5 {
            violations.push((eps, target_step, jump, switched));
        }
    }
    violations
}

#[test]
fn sp04_lp_r01_near_antipode_3d_azimuth_tracks_target_step() {
    let (d, r) = rig();
    let legacy = legacy_rig(&d);
    let cp = v3(r.rest_loop().center);
    // Control RED nativo: el solucionador plano conserva el defecto en este mismo fixture.
    let control = lp_r01_violations(&d, cp, |a, b| {
        let (mut av, mut an) = buffers(&d);
        let pa = ok(
            legacy.deform([a.x, a.y, a.z], None, &mut av, &mut an),
            "plano Y",
        );
        let (mut bv, mut bn) = buffers(&d);
        let pb = ok(
            legacy.deform([b.x, b.y, b.z], Some(pa.bend_side), &mut bv, &mut bn),
            "plano Z",
        );
        (av, bv, pb.side_switched)
    });
    assert!(
        control.iter().any(|c| c.0 == 1e-5),
        "control: el solucionador plano salta incluso con eps 1e-5: {control:?}"
    );
    // Puerto EXACTO sobre la API espacial (la memoria publicada sustituye a `bend_side`).
    let mut all_vertices = Vec::new();
    let violations = lp_r01_violations(&d, cp, |a, b| {
        assert!((a - cp).length() >= r.min_radius() && (b - cp).length() < MAX_REACH_OBJ);
        let (mut av, mut an) = buffers(&d);
        let pa = ok(
            r.deform([a.x, a.y, a.z], None, &mut av, &mut an),
            "azimuth Y",
        );
        let (mut bv, mut bn) = buffers(&d);
        ok(
            r.deform([b.x, b.y, b.z], Some(pa.memory), &mut bv, &mut bn),
            "azimuth Z",
        );
        // Ampliacion: TODOS los vertices (raiz, anillos, distal y espiral).
        all_vertices.push(((b - a).length(), max_jump(&av, &bv)));
        (av, bv, false)
    });
    assert!(
        violations.is_empty(),
        "LP-R01 finite mesh jump for vanishing 3D target step: {violations:?}"
    );
    for (step, jump) in all_vertices {
        assert!(
            jump <= 8.0 * step + 1e-5,
            "LP-R01 todos los vertices: paso {step}, salto {jump}"
        );
    }
}

/// LP-R02: barrido plano 60..300 grados, subdivisiones 1/4/20, TODOS los vertices, sin exencion. `solve`
/// recibe `true` en el primer cuadro de cada subdivision (el estado entre cuadros se reinicia, como en el
/// regresor del padre).
fn lp_r02_violations(
    radius: f64,
    cp: Vec3,
    mut solve: impl FnMut(Vec3, bool) -> (Vec<V>, bool),
) -> Vec<(usize, f64, bool, f64, f64)> {
    let mut violations = Vec::new();
    for subdivisions in [1_usize, 4, 20] {
        let mut previous: Option<(Vec<V>, Vec3)> = None;
        for i in 0..=240 * subdivisions {
            let degrees = 60.0 + i as f64 / subdivisions as f64;
            let a = degrees.to_radians();
            let tip = cp + Vec3::new(a.cos(), a.sin(), 0.0) * radius;
            let (v, switched) = solve(tip, i == 0);
            if let Some((pv, pt)) = &previous {
                let jump = max_jump(pv, &v);
                let step = (tip - *pt).length();
                if jump > 8.0 * step + 1e-5 {
                    violations.push((subdivisions, degrees, switched, step, jump));
                }
            }
            previous = Some((v, tip));
        }
    }
    violations
}

#[test]
fn sp05_lp_r02_planar_sweep_checks_every_frame_including_former_switch() {
    let (d, r) = rig();
    let legacy = legacy_rig(&d);
    let cp = v3(r.rest_loop().center);
    let mut side = None;
    let control = lp_r02_violations(1.0, cp, |tip, first| {
        if first {
            side = None;
        }
        let (mut v, mut n) = buffers(&d);
        let pose = ok(
            legacy.deform([tip.x, tip.y, tip.z], side, &mut v, &mut n),
            "plano",
        );
        side = Some(pose.bend_side);
        (v, pose.side_switched)
    });
    assert!(
        control.iter().any(|c| c.0 == 20 && c.2),
        "control: el plano salta en su cambio de rama con subdivision 20: {control:?}"
    );
    // Puerto EXACTO (radio 1) y ampliacion a la bola minima y a distancia lejana; punta exacta siempre.
    for radius in [1.0, r.min_radius() * (1.0 + 1e-9), 4.0] {
        let mut memory = None;
        let violations = lp_r02_violations(radius, cp, |tip, first| {
            if first {
                memory = None;
            }
            let (mut v, mut n) = buffers(&d);
            let pose = ok(
                r.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                "planar sweep",
            );
            assert!((v3(pose.axial_tip) - tip).length() < 1e-9, "punta exacta");
            memory = Some(pose.memory);
            (v, false)
        });
        assert!(
            violations.is_empty(),
            "LP-R02 discontinuity at radius {radius}, including former switch: {violations:?}"
        );
    }
}

#[test]
fn sp06_rear_yaw_around_the_opposite_pole_is_continuous() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    let mut worst: f64 = 0.0;
    for radius in [1.0, r.min_radius() * (1.0 + 1e-9)] {
        // Vuelta de azimut alrededor del polo trasero, dos vueltas, paso de 1 grado.
        for eps in [1e-2_f64, 1e-3, 1e-4, 1e-5] {
            let mut memory: Option<SpatialMemory> = None;
            let mut previous: Option<(Vec<V>, Vec3)> = None;
            for i in 0..=720 {
                let phi = (i as f64).to_radians();
                let tip = cp
                    + Vec3::new(-eps.cos(), eps.sin() * phi.cos(), eps.sin() * phi.sin()) * radius;
                let (mut v, mut n) = buffers(&d);
                let pose = ok(
                    r.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                    "yaw",
                );
                assert!((v3(pose.axial_tip) - tip).length() < 1e-9);
                if let Some((pv, pt)) = &previous {
                    let step = (tip - *pt).length();
                    let jump = max_jump(pv, &v);
                    worst = worst.max(jump / step);
                    assert!(
                        jump <= 8.0 * step + 1e-5,
                        "eps {eps} r {radius} phi {i}: paso {step} salto {jump}"
                    );
                }
                memory = Some(pose.memory);
                previous = Some((v, tip));
            }
        }
        // Cruces 3D por el polo (circulos maximos en varios azimuts), paso de 0.1 grados.
        for azimuth in [0.0_f64, 37.0, 90.0, 133.0] {
            let w = Vec3::new(0.0, azimuth.to_radians().cos(), azimuth.to_radians().sin());
            let mut memory: Option<SpatialMemory> = None;
            let mut previous: Option<(Vec<V>, Vec3)> = None;
            for i in 0..=3600 {
                let q = (30.0 + i as f64 * 0.1).to_radians();
                let tip = cp + (Vec3::X * q.cos() + w * q.sin()) * radius;
                let (mut v, mut n) = buffers(&d);
                let pose = ok(
                    r.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                    "cruce",
                );
                assert!((v3(pose.axial_tip) - tip).length() < 1e-9);
                if let Some((pv, pt)) = &previous {
                    let step = (tip - *pt).length();
                    let jump = max_jump(pv, &v);
                    worst = worst.max(jump / step);
                    assert!(
                        jump <= 8.0 * step + 1e-5,
                        "azimut {azimuth} r {radius} i {i}: paso {step} salto {jump}"
                    );
                }
                memory = Some(pose.memory);
                previous = Some((v, tip));
            }
        }
    }
    println!("ganancia maxima salto/paso (todos los vertices): {worst}");
}

#[test]
fn sp07_planar_sweep_is_deterministic_and_continuous_without_branch_jump() {
    let (d, rig) = rig();
    let cp = v3(rig.rest_loop().center);
    let run = || {
        let mut previous: Option<(Vec<V>, Vec3)> = None;
        let mut memory = None;
        let mut log = Vec::new();
        for deg in 60..=300 {
            let a = (deg as f64).to_radians();
            let tip = cp + (Vec3::X * a.cos() + Vec3::Y * a.sin()) * 1.0;
            let (mut v, mut n) = buffers(&d);
            let pose = ok(
                rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                "deform",
            );
            let (jump, step) = previous.as_ref().map_or((0.0, 0.0), |(pv, pt)| {
                (max_jump(pv, &v), (tip - *pt).length())
            });
            log.push((deg, jump, step, pose.turn, pose.singular_margin));
            memory = Some(pose.memory);
            previous = Some((v, tip));
        }
        log
    };
    let log = run();
    assert_eq!(log, run(), "determinista");
    for &(deg, jump, step, turn, margin) in &log {
        // Sin exencion de ningun cuadro: no hay ramas ni cambio de lado declarado.
        assert!(
            jump <= 8.0 * step + 1e-5,
            "continuo a {deg}: salto {jump} paso {step}"
        );
        assert!(
            turn.is_finite() && turn < 2.0 * std::f64::consts::PI,
            "{deg}: giro {turn}"
        );
        assert!(margin > 0.5, "{deg}: margen a la singularidad {margin}");
    }
}

#[test]
fn sp08_lp_r03_finite_huge_distal_vertex_outputs_finite_or_rejects_atomically() {
    let (d, healthy) = rig();
    let cp = v3(healthy.rest_loop().center);
    let tip = cp + Vec3::new(-1.0, 1.0, 0.0).normalized();
    let tip = [tip.x, tip.y, tip.z];
    let (mut cv, mut cn) = buffers(&d);
    ok(
        healthy.deform(tip, None, &mut cv, &mut cn),
        "healthy control same valid tip",
    );
    let mut arm = tube(1).vertices;
    arm[24] = [1.7e308, 1.7e308, 0.0]; // Finite, distal, NOT in loop_p or grip.
    assert!(arm.iter().flatten().all(|x| x.is_finite()));
    let (mut v, mut n) = buffers(&d);
    let before = (v.clone(), n.clone());
    match SpatialRig::new(d.package().data(), &arm) {
        Err(_) => assert_eq!((v, n), before, "constructor rejection has no output writes"),
        Ok(r) => match r.deform(tip, None, &mut v, &mut n) {
            Err(_) => assert_eq!((v, n), before, "LP-R03 Err must preserve both buffers"),
            Ok(pose) => {
                assert!(
                    v.iter().chain(&n).flatten().all(|x| x.is_finite()),
                    "LP-R03 Ok published nonfinite derived vertex/normal from finite input"
                );
                assert!(
                    pose.axial_tip
                        .iter()
                        .chain(&pose.distal_center)
                        .chain(&pose.tip_obj)
                        .chain(&pose.memory.side)
                        .all(|x| x.is_finite())
                );
                assert!(
                    [pose.turn, pose.shaft_length, pose.line_length]
                        .iter()
                        .all(|x| x.is_finite()),
                    "LP-R03 nonfinite pose output"
                );
            }
        },
    }
}

#[test]
fn sp09_lp_r07_huge_finite_memory_side_is_unit_equivalent_or_atomic_error() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    let tip = cp - Vec3::X * 1.5; // Valid rear target, beyond the spatial exact ball and below max12.
    assert!(1.5 >= r.min_radius() && 1.5 < MAX_REACH_OBJ);
    let tip = [tip.x, tip.y, tip.z];
    let (mut reference_v, mut reference_n) = buffers(&d);
    ok(
        r.deform(
            tip,
            Some(mem([0.0, 1.0, 0.0], tip)),
            &mut reference_v,
            &mut reference_n,
        ),
        "unit Y control",
    );
    let (mut v, mut n) = buffers(&d);
    let before = (v.clone(), n.clone());
    match r.deform(tip, Some(mem([0.0, 1e308, 0.0], tip)), &mut v, &mut n) {
        Err(_) => assert_eq!((v, n), before, "LP-R07 Err writes neither buffer"),
        Ok(_) => {
            let finite = v.iter().chain(&n).flatten().all(|x| x.is_finite());
            let unit_normals = n.iter().all(|x| (v3(*x).length() - 1.0).abs() < 1e-6);
            let geometry_error = max_jump(&v, &reference_v);
            let normal_error = max_jump(&n, &reference_n);
            assert!(
                finite && unit_normals && geometry_error < 1e-9 && normal_error < 1e-9,
                "LP-R07 overflowing memory side norm: finite={finite}, unit_normals={unit_normals}, geometry_error={geometry_error}, normal_error={normal_error}"
            );
        }
    }
}

#[test]
fn sp10_memory_follows_the_target_side_with_a_bounded_rate() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    // Sin memoria y fuera del eje: el lado es el de la punta (curva = la plana de la familia anterior).
    let tip = cp + Vec3::new(-0.3, 0.8, 0.2).normalized();
    let (mut v, mut n) = buffers(&d);
    let pose = ok(
        r.deform([tip.x, tip.y, tip.z], None, &mut v, &mut n),
        "sin memoria",
    );
    let expect = Vec3::new(0.0, 0.8, 0.2).normalized();
    assert!((v3(pose.memory.side) - expect).length() < 1e-12 && pose.side_rotation == 0.0);
    // Detras y del lado +Y; la punta cruza al lado -Y en pasos de 0.5 grados: el lado gira acotado por el
    // paso y termina alineado con el de la punta (sin saltos ni cambios de rama).
    let mut memory = Some(mem([0.0, 1.0, 0.0], [cp.x - 1.0, cp.y, cp.z]));
    let mut previous = cp - Vec3::X;
    let mut caught = None;
    for i in 1..=200 {
        let q = (180.0 + 0.5 * i as f64).to_radians();
        let tip = cp + Vec3::new(q.cos(), q.sin(), 0.0);
        let (mut v, mut n) = buffers(&d);
        let pose = ok(
            r.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
            "cruce",
        );
        let allow = SPATIAL_SIDE_RATE * (tip - previous).length() / (tip - cp).length();
        assert!(
            pose.side_rotation.abs() <= allow + 1e-12,
            "{i}: giro {} > {allow}",
            pose.side_rotation
        );
        // Giro REAL de la memoria (entrada -> salida), no solo el declarado.
        let before = v3(memory.unwrap().side);
        let real = before.dot(v3(pose.memory.side)).clamp(-1.0, 1.0).acos();
        assert!(
            real <= allow + 1e-9,
            "{i}: la memoria salto {real} rad > {allow}"
        );
        assert!(
            (v3(pose.memory.side).dot(Vec3::X)).abs() < 1e-12,
            "{i}: lado perpendicular a t"
        );
        if caught.is_none() && (v3(pose.memory.side) + Vec3::Y).length() < 1e-9 {
            caught = Some(i);
        }
        memory = Some(pose.memory);
        previous = tip;
    }
    let caught = caught.expect("la memoria alcanza el lado de la punta");
    assert!(
        caught < 120,
        "alcanzado antes de 60 grados de recorrido: {caught}"
    );
}

// ---------------- cadena real nave -> World -> persecucion espacial -> cuadro (sintetico) ----------------

const STEP: f64 = 1.0 / 30.0;

fn pursuit(world: &World) -> SpatialPursuit {
    let mut p = SpatialPursuit::new(debug());
    ok(p.bind(world), "bind");
    p
}

/// Un cuadro del juego con la nave volada por el controlador REAL (`advance_frame`) y la persecucion.
fn fly(world: &mut World, p: &mut SpatialPursuit, input: ShipInput) -> SpatialFrameStatus {
    advance_frame(world, &input, STEP);
    ok(p.step(world, STEP), "paso")
}

fn thrust(t: f64, yaw: f64) -> ShipInput {
    ShipInput {
        thrust: t,
        yaw,
        ..ShipInput::default()
    }
}

fn pixels_and_stats(fb: &Framebuffer, s: &RenderStats) -> (Vec<u32>, String) {
    (fb.pixels.clone(), format!("{s:?}"))
}

fn d_correspondence() -> Vec<usize> {
    debug().correspondence().arm_vertex().to_vec()
}

/// Todo lo publicado por el wrapper (controlador, memoria, capa, rig, debug y su bind).
fn publication(p: &SpatialPursuit) -> String {
    let rig = p.rig().map(|r| {
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            r.rest_loop(),
            r.tip_offset(),
            r.bend_radius(),
            r.min_radius(),
            r.rest_vertices(),
            r.rest_normals()
        )
    });
    let d = p.debug();
    format!(
        "{:?}",
        (
            p.controller(),
            p.last(),
            p.memory(),
            p.composite(),
            p.body_prefix(),
            rig,
            p.body_override()
                .map(|b| (format!("{:?}", b.model), b.extra_bounds_obj)),
            p.title(),
            d.bind_report(),
            d.bound_layer()
                .map(|(m, nv, nn)| (format!("{m:?}"), nv, nn)),
            (
                d.composite(),
                d.growth_mesh(),
                d.is_active(),
                d.snapshot(),
                d.timeline(),
                d.clock(),
                d.last_frame()
            )
        )
    )
}

#[test]
fn sp11_idle_and_above_fifteen_percent_frames_are_exactly_the_original_frames() {
    for o2 in [1.0, 0.5, 0.25, 0.16] {
        let (mut w, g) = game(o2);
        let d = debug();
        place_ship(&mut w, &d, ALONG);
        let mut p = pursuit(&w);
        for i in 0..8 {
            let s = fly(
                &mut w,
                &mut p,
                thrust(1.0, if i % 2 == 0 { 0.5 } else { 0.0 }),
            );
            assert_eq!(s.step.mode, PursuitMode::Idle, "o2 {o2}");
            assert!(
                p.body_override().is_none() && p.memory().is_none(),
                "o2 {o2}: sin capa propia"
            );
            let (a, sa) = frame(&w, &g, true, p.body_override().as_ref());
            let (b, sb) = frame(&w, &g, true, None);
            assert_eq!(
                pixels_and_stats(&a, &sa),
                pixels_and_stats(&b, &sb),
                "o2 {o2}"
            );
        }
    }
}

#[test]
fn sp12_root_and_proximal_coincide_with_the_body_every_frame_while_the_body_turns() {
    let (mut w, _g) = game(0.05);
    let mut p = pursuit(&w);
    let meshy = creature_model();
    let corr = d_correspondence();
    let topo_vertices: Vec<GrowthVertex> = debug().package().data().topology().vertices().to_vec();
    let start = w.creature_render_transform().unwrap().matrix();
    let mut extended = 0;
    for i in 0..120 {
        let s = fly(&mut w, &mut p, thrust(1.0, if i < 60 { 1.0 } else { -0.6 }));
        let Some(model) = p.composite() else {
            continue;
        };
        if s.shaft.is_some_and(|sh| sh.shaft_length > 0.0) {
            extended += 1;
        }
        let (nv, _) = p.body_prefix().expect("prefijo");
        let c = w.creature_render_transform().unwrap().matrix();
        for (k, v) in topo_vertices.iter().enumerate() {
            if let GrowthVertex::Retained {
                side: Side::P,
                source: VertexSource::Orig { orig, .. },
            } = v
            {
                let body = meshy.vertices[corr[*orig]];
                assert_eq!(
                    model.vertices[nv + k],
                    body,
                    "cuadro {i}: proximal {k} = Meshy EXACTO"
                );
                let (a, b) = (
                    c.transform_point(Vec3::from_array(model.vertices[nv + k])),
                    c.transform_point(Vec3::from_array(body)),
                );
                assert_eq!(a, b, "cuadro {i}: mismo punto en MUNDO");
            }
        }
    }
    assert!(extended > 60, "el brazo se extendio: {extended}");
    let end = w.creature_render_transform().unwrap().matrix();
    assert_ne!(
        start, end,
        "el cuerpo giro (seguimiento horizontal) durante la prueba"
    );
}

#[test]
fn sp13_tip_follows_the_current_ship_position_from_the_real_controller() {
    let (mut w, g) = game(0.05);
    let mut p = pursuit(&w);
    let mut last_ship = w.ship.position;
    let mut max_far: f64 = 0.0;
    let mut previous: Option<(Vec<[f64; 3]>, Vec3)> = None;
    let mut worst_gain: f64 = 0.0;
    // 1) La nave acelera y gira: el objetivo es SIEMPRE su posicion actual.
    for i in 0..150 {
        let s = fly(&mut w, &mut p, thrust(1.0, if i < 75 { 0.7 } else { -0.7 }));
        assert_eq!(s.ship_world, w.ship.position);
        if !s.step.target_clamped {
            assert_eq!(
                s.step.target, w.ship.position,
                "cuadro {i}: objetivo = nave actual"
            );
        }
        assert!(
            s.step.moved <= 48.0 * STEP + 1e-6,
            "cuadro {i}: {}",
            s.step.moved
        );
        let c = w.creature_render_transform().unwrap().matrix();
        let shaft = s.shaft.expect("persiguiendo con O2 5 %");
        let axial = c.transform_point(Vec3::from_array(shaft.axial_tip));
        assert!(
            (axial - s.tip_world).length() < 1e-6,
            "cuadro {i}: la malla sigue al controlador"
        );
        assert!(w.ship.position != last_ship, "la nave se movio");
        last_ship = w.ship.position;
        max_far = max_far.max(s.tip_ship_distance);
        // Continuidad temporal del tramo EN OBJ respecto al paso OBJ de la punta (sin camara ni FOV).
        let (nv, _) = p.body_prefix().unwrap();
        let shaft_vertices = p.composite().unwrap().vertices[nv..].to_vec();
        let tip = Vec3::from_array(shaft.tip_obj);
        if let Some((pv, pt)) = &previous {
            let step = (tip - *pt).length();
            let jump = max_jump(pv, &shaft_vertices);
            assert!(
                jump <= 8.0 * step + 1e-5,
                "cuadro {i}: salto {jump} para paso {step}"
            );
            if step > 0.0 {
                worst_gain = worst_gain.max(jump / step);
            }
        }
        previous = Some((shaft_vertices, tip));
    }
    // 2) Solo orientacion (sin empuje, la nave frena): la punta sigue la POSICION, no el giro.
    let mut s = fly(&mut w, &mut p, thrust(0.0, 1.0));
    for _ in 0..600 {
        s = fly(&mut w, &mut p, thrust(0.0, 1.0));
    }
    assert!(
        s.step.reached,
        "alcanza la nave casi quieta: {}",
        s.tip_ship_distance
    );
    assert!(s.tip_ship_distance < 1e-6);
    assert!(max_far > 50.0);
    println!("ganancia maxima salto/paso en vuelo: {worst_gain}");
    let (with, _) = frame(&w, &g, false, p.body_override().as_ref());
    let (without, _) = frame(&w, &g, false, None);
    assert_ne!(
        with.pixels, without.pixels,
        "el brazo extendido se dibuja en la capa Body"
    );
}

#[test]
fn sp14_errors_are_atomic_and_keep_the_last_frame() {
    let (mut w, g) = game(0.05);
    let mut p = pursuit(&w);
    for _ in 0..40 {
        fly(&mut w, &mut p, thrust(1.0, 0.4));
    }
    let before = publication(&p);
    let (fb_before, st_before) = frame(&w, &g, true, p.body_override().as_ref());
    for (name, dt) in [
        ("dt nan", f64::NAN),
        ("dt negativo", -STEP),
        ("dt enorme", 5.0),
    ] {
        assert!(p.step(&w, dt).is_err(), "{name}");
        assert_eq!(publication(&p), before, "{name}");
    }
    let o2 = w.debug_oxygen01;
    w.debug_oxygen01 = f64::NAN;
    assert!(p.step(&w, STEP).is_err(), "O2 NaN");
    w.debug_oxygen01 = o2;
    let ship = w.ship.position;
    w.ship.position = Vec3::new(2.0e9, 0.0, 0.0);
    assert!(p.step(&w, STEP).is_err(), "nave extrema");
    w.ship.position = ship;
    assert_eq!(publication(&p), before, "nada cambio");
    let (fb_after, st_after) = frame(&w, &g, true, p.body_override().as_ref());
    assert_eq!(
        pixels_and_stats(&fb_before, &st_before),
        pixels_and_stats(&fb_after, &st_after)
    );
    let mut unbound = SpatialPursuit::new(debug());
    assert!(unbound.step(&w, STEP).is_err(), "sin bind");
    assert!(unbound.body_override().is_none());
}

#[test]
fn sp15_bind_reuses_the_identity_guards() {
    let mut model = creature_model();
    model.vertices[DOME_VERTICES + 3][1] += 1e-3;
    let (w, _g) = game_with(model, 0.05);
    let mut p = SpatialPursuit::new(debug());
    assert!(p.bind(&w).is_err(), "otra criatura");
    assert!(p.rig().is_none() && p.composite().is_none() && p.body_prefix().is_none());
    let (w, _g) = game(0.05);
    let report = ok(p.bind(&w), "bind");
    assert_eq!(
        report.capture.arm_faces_removed,
        debug().correspondence().arm_face().len()
    );
    assert!(report.exact_rest_vertices > 0);
    assert!(report.tip_offset_obj >= 2.0 * report.bend_radius_obj);
    assert!(
        report.min_radius_obj > report.tip_offset_obj
            && report.min_radius_obj < report.max_reach_obj
    );
}

#[test]
fn sp16_lp_r04_replaced_valid_creature_rejected_without_publication() {
    let (mut w, _g) = game(0.05);
    let mut p = pursuit(&w);
    for _ in 0..3 {
        ok(p.step(&w, STEP), "initial healthy publication");
    }
    assert!(p.controller().is_some() && p.last().is_some() && p.body_override().is_some());
    let before = publication(&p);
    let placement = w.creature().unwrap().placement();
    let mut replacement = creature_model();
    replacement.vertices[129][0] += 1e-3; // Non-eye BODY vertex, no arm/cut deformation.
    replacement.update_bounds();
    let changed_geometry = geometry_fnv(&replacement);
    let creature = ok(
        StaticCreature::new(replacement, texture(), placement),
        "valid replacement",
    );
    ok(
        w.set_creature(creature),
        "legal World replacement with refreshed partition",
    );
    assert_eq!(
        geometry_fnv(w.creature().unwrap().model()),
        changed_geometry
    );
    assert!(
        debug().bind(&w).is_err(),
        "control: incompatible capture identity"
    );
    let result = p.step(&w, STEP);
    let after = publication(&p);
    assert!(
        result.is_err(),
        "LP-R04 incompatible cached identity accepted: {result:?}; publication_unchanged={}",
        before == after
    );
    assert_eq!(
        after, before,
        "LP-R04 rejected step changed controller/memory/composite/debug/bind status"
    );
}

#[test]
fn sp17_lp_r06_thick_inherited_bind_control_is_healthy_and_spatial_bind_is_atomic() {
    let (w, _g) = thick::game(0.05);
    // Control sano heredado dentro de la misma prueba (un filtro por nombre no puede saltarlo).
    let mut control = thick::debug();
    let report = ok(
        control.bind(&w),
        "mandatory healthy inherited bind radius .09",
    );
    assert!(report.max_position_error <= 1e-9 && report.arm_faces_removed > 0);
    assert!(control.bind_report().is_some() && control.bound_layer().is_some());
    let arm = thick::tube(1).vertices;
    let error = SpatialRig::new(control.package().data(), &arm)
        .err()
        .expect("thick rig must reject");
    assert!(
        error.contains("reposo invalido para el codo"),
        "wrong failure stage: {error}"
    );
    let mut p = SpatialPursuit::new(thick::debug());
    let before = publication(&p);
    let result = p.bind(&w);
    let after = publication(&p);
    assert!(result.is_err(), "thick spatial rig must reject");
    assert_eq!(
        after, before,
        "LP-R06 Err partially published debug bind_report/bound_layer or wrapper state"
    );
}

#[test]
fn sp18_dt_zero_small_positive_and_recovery_with_a_turning_body() {
    let (mut w, g) = game(0.05);
    let mut p = pursuit(&w);
    for i in 0..45 {
        fly(&mut w, &mut p, thrust(1.0, if i < 25 { 1.0 } else { -0.8 }));
    }
    assert_eq!(p.last().unwrap().step.mode, PursuitMode::Pursuing);
    // dt 0: el estado publicado (controlador, memoria y capa) no cambia.
    let geometry = |p: &SpatialPursuit| {
        (
            format!("{:?} {:?}", p.controller(), p.memory()),
            p.composite().map(|m| m.vertices.clone()),
        )
    };
    let before = geometry(&p);
    let s = ok(p.step(&w, 0.0), "dt 0");
    assert_eq!(s.step.moved, 0.0);
    assert_eq!(geometry(&p), before, "dt 0 no mueve nada");
    // dt positivo pequeno con el cuerpo girando: avance acotado por la rapidez.
    for _ in 0..5 {
        advance_frame(&mut w, &thrust(0.0, 1.0), 1e-3);
        let s = ok(p.step(&w, 1e-3), "dt pequeno");
        assert!(s.step.moved <= 48.0 * 1e-3 + 1e-9, "{}", s.step.moved);
    }
    // Recuperacion: O2 > 15 % vuelve al reposo con tope 24 u/s SIN reset (la rapidez de persecucion
    // baja con la deceleracion limitada del controlador compartido), despues reposo EXACTO sin capa.
    w.debug_oxygen01 = 1.0;
    let mut retracting = 0;
    let mut speed = p.controller().unwrap().speed();
    for i in 0..900 {
        let s = fly(
            &mut w,
            &mut p,
            thrust(0.0, if i % 90 < 45 { 0.6 } else { -0.6 }),
        );
        if s.step.mode == PursuitMode::Retracting {
            retracting += 1;
            let now = p.controller().unwrap().speed();
            assert_eq!(s.step.max_speed, PURSUIT_RETRACT_SPEED, "{i}");
            assert!(
                s.step.moved <= speed.max(now) * STEP + 1e-9,
                "{i}: {} con {speed}",
                s.step.moved
            );
            assert!(
                now <= (speed - PURSUIT_ACCELERATION * STEP).max(PURSUIT_RETRACT_SPEED) + 1e-9,
                "{i}: {speed} -> {now}"
            );
            assert!(
                speed - now <= PURSUIT_ACCELERATION * STEP + 1e-9 || s.step.reached,
                "{i}: frenado brusco {speed} -> {now}"
            );
            speed = now;
        }
        if s.step.mode == PursuitMode::Idle {
            break;
        }
    }
    assert!(retracting > 0, "recoge antes de reposar");
    assert_eq!(p.last().unwrap().step.mode, PursuitMode::Idle);
    assert!(p.body_override().is_none() && p.memory().is_none());
    let (a, sa) = frame(&w, &g, true, p.body_override().as_ref());
    let (b, sb) = frame(&w, &g, true, None);
    assert_eq!(
        pixels_and_stats(&a, &sa),
        pixels_and_stats(&b, &sb),
        "reposo = cuadro de siempre"
    );
}

// ---------------- linea de comandos y entrada de la app (antes de cualquier E/S) ----------------

fn temp_inputs(tag: &str) -> (std::path::PathBuf, LivePursuitRequest) {
    let dir = std::env::temp_dir().join(format!(
        "umbral_spatial_pursuit_{tag}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("capture")).unwrap();
    for f in ["growth.txt", "corr.txt", "criatura.obj", "textura.png"] {
        std::fs::write(dir.join(f), "x").unwrap();
    }
    let r = LivePursuitRequest {
        capture_dir: dir.join("capture"),
        growth_package: dir.join("growth.txt"),
        correspondence: dir.join("corr.txt"),
    };
    (dir, r)
}

fn flags(r: &LivePursuitRequest) -> TentacleCaptureFlags {
    TentacleCaptureFlags {
        enabled: false,
        capture_dir: Some(r.capture_dir.clone()),
        growth_package: Some(r.growth_package.clone()),
        correspondence: Some(r.correspondence.clone()),
        start_time: None,
        solar: true,
        legacy: false,
    }
}

#[test]
fn sp19_spatial_flags_are_validated_before_any_io() {
    let (dir, r) = temp_inputs("flags");
    assert_eq!(
        ok(
            validate_spatial_pursuit_flags(false, false, &flags(&r)),
            "sin flag"
        ),
        None
    );
    // Sin el flag nuevo, el validador de la persecucion viva no cambia.
    assert_eq!(
        ok(validate_live_pursuit_flags(true, &flags(&r)), "viva"),
        Some(r.clone())
    );
    let good = ok(
        validate_spatial_pursuit_flags(true, false, &flags(&r)),
        "valido",
    )
    .expect("pedido");
    assert_eq!(good, r);
    assert_eq!(
        good.capture_request().start_time,
        None,
        "sin reloj de la composicion"
    );
    let e = validate_spatial_pursuit_flags(true, true, &flags(&r)).expect_err("con viva");
    assert!(
        e.contains("--tentacle-live-pursuit") && e.contains("incompatibles"),
        "{e}"
    );
    type Case = (
        &'static str,
        Box<dyn Fn(&mut TentacleCaptureFlags)>,
        &'static str,
    );
    let cases: Vec<Case> = vec![
        ("con F7", Box::new(|f| f.enabled = true), "incompatibles"),
        (
            "con reloj",
            Box::new(|f| f.start_time = Some(1.0)),
            "--tentacle-capture-time",
        ),
        ("legacy", Box::new(|f| f.legacy = true), "--legacy"),
        (
            "sin Sol",
            Box::new(|f| f.solar = false),
            "--creature-solar-reveal",
        ),
        (
            "sin captura",
            Box::new(|f| f.capture_dir = None),
            "--tentacle-capture-package",
        ),
        (
            "sin tramo",
            Box::new(|f| f.growth_package = None),
            "--tentacle-growth-package",
        ),
        (
            "sin corr",
            Box::new(|f| f.correspondence = None),
            "--tentacle-arm-correspondence",
        ),
        (
            "ruta inexistente",
            Box::new(|f| f.growth_package = Some(std::path::PathBuf::from("no/existe.txt"))),
            "--tentacle-growth-package",
        ),
    ];
    for (name, change, needle) in cases {
        let mut f = flags(&r);
        change(&mut f);
        let e = validate_spatial_pursuit_flags(true, false, &f).expect_err(name);
        assert!(e.contains(needle), "{name}: {e}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sp20_the_app_entry_rejects_bad_routes_before_any_io() {
    let (dir, r) = temp_inputs("app");
    let options = |output: std::path::PathBuf| GameOptions {
        width: 64,
        height: 36,
        headless: true,
        frame_limit: Some(1),
        output,
        time: 0.0,
        near: None,
        culling: true,
        lod: true,
        overlay: false,
        creature: Some(CreatureOptions {
            model: dir.join("criatura.obj"),
            texture: dir.join("textura.png"),
            near: false,
        }),
        creature_reveal: false,
        creature_reveal_stages: true,
        creature_eye_follow: true,
        debug_oxygen: 1.0,
    };
    let out = dir.join("salida.png");
    let solar = PerceptionRoute {
        solar: true,
        ..PerceptionRoute::default()
    };
    for (name, route, perception, watching, output, needle) in [
        (
            "sin Sol",
            PerceptionRoute::default(),
            false,
            false,
            out.clone(),
            "--creature-solar-reveal",
        ),
        (
            "estrellas sin percepcion",
            solar,
            false,
            true,
            out.clone(),
            "percepcion",
        ),
        (
            "salida = textura",
            solar,
            false,
            false,
            dir.join("textura.png"),
            "--output",
        ),
    ] {
        let e = run_game_with_tentacle_spatial_pursuit(
            options(output),
            route,
            perception,
            watching,
            r.clone(),
        )
        .expect_err(name);
        assert!(e.contains(needle), "{name}: {e}");
    }
    assert!(!out.exists(), "nada escrito");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sp21_the_binary_rejects_spatial_combinations_before_any_io() {
    let out = std::env::temp_dir().join(format!("umbral_spatial_guard_{}.png", std::process::id()));
    for (args, needle) in [
        (
            vec!["--tentacle-spatial-pursuit"],
            "requiere --creature-solar-reveal",
        ),
        (
            vec!["--tentacle-spatial-pursuit", "--tentacle-live-pursuit"],
            "incompatibles",
        ),
        (
            vec!["--tentacle-spatial-pursuit", "--tentacle-capture-debug"],
            "incompatibles",
        ),
        (
            vec!["--tentacle-spatial-pursuit", "--tentacle-capture-time", "1"],
            "--tentacle-capture-time",
        ),
        (vec!["--tentacle-spatial-pursuit", "--legacy"], "--legacy"),
        // Sin el flag nuevo, las reglas de siempre no cambian.
        (
            vec!["--tentacle-live-pursuit"],
            "--tentacle-live-pursuit requiere --creature-solar-reveal",
        ),
        (
            vec!["--tentacle-capture-package", "x"],
            "requieren --tentacle-capture-debug",
        ),
    ] {
        let o = std::process::Command::new(env!("CARGO_BIN_EXE_umbral"))
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
        assert!(!out.exists(), "{args:?}: nada escrito");
    }
    let help = std::process::Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--tentacle-spatial-pursuit (requiere --creature-solar-reveal)"));
    assert!(help.contains("--tentacle-live-pursuit (requiere --creature-solar-reveal)"));
    assert!(help.contains("--tentacle-capture-debug (requiere --creature-solar-reveal)"));
}

// ============================ assets REALES (opt-in, #[ignore]) ============================
// Entorno: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE,
// UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE (claves fixture_environment de BASELINE.json).

fn real_env(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k}")))
}

fn real_game(oxygen: f64) -> (World, SolarGame, SpatialPursuit) {
    let mut world = World::new(ship_model(), 1280.0 / 720.0).unwrap();
    world.debug_oxygen01 = oxygen;
    let placement = solar_creature_placement(&world).unwrap();
    let creature = ok(
        StaticCreature::load(
            &real_env("UMBRAL_CAPTURE_CREATURE_OBJ"),
            &real_env("UMBRAL_CREATURE_TEXTURE"),
            placement,
        ),
        "criatura real",
    );
    let game = ok(SolarGame::start(&mut world, creature), "solar");
    let mut p = ok(
        SpatialPursuit::load(&LivePursuitRequest {
            capture_dir: real_env("UMBRAL_CAPTURE_PACKAGE_DIR"),
            growth_package: real_env("UMBRAL_GROWTH_PACKAGE"),
            correspondence: real_env("UMBRAL_ARM_CORRESPONDENCE"),
        }),
        "paquetes reales",
    );
    let report = ok(p.bind(&world), "bind real");
    println!("bind real: {report:?}");
    assert_eq!(report.capture.arm_faces_removed, 2827);
    assert_eq!(report.capture.shared_root_vertices, 27);
    (world, game, p)
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_spatial_rest_is_the_exact_meshy_arm_and_deforms_in_every_direction() {
    let (world, _g, p) = real_game(0.05);
    let rig = p.rig().unwrap();
    let data = p.debug().package().data();
    let topo = data.topology();
    let meshy = world.creature().unwrap().model();
    let corr = p.debug().correspondence().arm_vertex();
    let mut exact = 0;
    for (i, v) in topo.vertices().iter().enumerate() {
        if let GrowthVertex::Retained {
            source: VertexSource::Orig { orig, .. },
            ..
        } = v
        {
            assert_eq!(
                rig.rest_vertices()[i],
                meshy.vertices[corr[*orig]],
                "vertice {i}"
            );
            exact += 1;
        }
    }
    println!(
        "vertices originales exactos: {exact}; codo {} OBJ, punta axial {} OBJ, contorno {}, bola minima espacial {} OBJ",
        rig.bend_radius(),
        rig.tip_offset(),
        rig.rest_loop().contour_radius,
        rig.min_radius()
    );
    let rest = rig.rest_loop();
    let (cp, t) = (
        Vec3::from_array(rest.center),
        Vec3::from_array(rest.tangent),
    );
    let template = data.template();
    let buffers = || (template.vertices.clone(), template.normals.clone());
    // Reposo: identico al Meshy y a la ley con cualquier memoria.
    let (mut rv, mut rn) = buffers();
    let pose = ok(rig.deform(rig.rest_tip(), None, &mut rv, &mut rn), "reposo");
    // La tangente real no es un eje exacto: la punta de reposo lleva redondeo (~1e-17). El cuadro de
    // reposo es exacto por otra via (sin capa, `None`); aqui se acota el residuo de la malla.
    assert!(
        pose.turn.abs() < 1e-12 && pose.shaft_length.abs() < 1e-12,
        "{pose:?}"
    );
    let rest_residual = rv
        .iter()
        .zip(rig.rest_vertices())
        .map(|(a, b)| (Vec3::from_array(*a) - Vec3::from_array(*b)).length())
        .fold(0.0, f64::max);
    println!(
        "reposo real: giro {:e}, tramo {:e}, residuo maximo de vertices {rest_residual:e}",
        pose.turn, pose.shaft_length
    );
    assert!(rest_residual < 1e-12);
    let faces = topo.faces();
    let orientation = |v: &[[f64; 3]], n: &[[f64; 3]], fi: usize| {
        let [p0, p1, p2] = faces[fi].v.map(|i| Vec3::from_array(v[i]));
        let g = (p1 - p0).cross(p2 - p0);
        let c = Vec3::from_array(n[3 * fi])
            + Vec3::from_array(n[3 * fi + 1])
            + Vec3::from_array(n[3 * fi + 2]);
        (g.length(), g.dot(c) > 0.0)
    };
    let straight = cp + t * 3.0;
    let (mut sv, mut sn) = buffers();
    ok(
        rig.deform([straight.x, straight.y, straight.z], None, &mut sv, &mut sn),
        "recto",
    );
    let side = t.cross(Vec3::Y).normalized();
    let up = side.cross(t).normalized();
    let dirs = [
        ("radial", t),
        ("detras", -t),
        ("lateral+", side),
        ("lateral-", -side),
        ("vertical+", up),
        ("vertical-", -up),
        ("oblicua", (t * -0.4 + side * 0.7 + up * 0.6).normalized()),
        ("casi detras", (-t + side * 0.05).normalized()),
    ];
    let memories = [None, Some(side), Some(-side), Some(up), Some(-up)];
    let mut worst_normal: f64 = 0.0;
    let mut cases = 0;
    for (name, dir) in dirs {
        for dist in [0.5, rig.min_radius(), 2.0, 6.0, 11.0] {
            for m in memories {
                if m.is_some() && dist < rig.min_radius() {
                    continue; // Fuera del dominio exacto con memoria arbitraria (declarado).
                }
                let tip = cp + dir * dist;
                let memory = m.map(|s| SpatialMemory {
                    side: [s.x, s.y, s.z],
                    tip_obj: [tip.x, tip.y, tip.z],
                });
                let what = format!("{name} {dist} memoria {m:?}");
                let (mut v, mut n) = buffers();
                let pose = ok(
                    rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                    &what,
                );
                assert!(
                    (Vec3::from_array(pose.axial_tip) - tip).length() < 1e-9,
                    "{what}: punta"
                );
                for (i, tv) in topo.vertices().iter().enumerate() {
                    if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
                        assert_eq!(v[i], rig.rest_vertices()[i], "{what}: proximal {i}");
                    }
                }
                let mut flips = 0;
                for (fi, f) in faces.iter().enumerate() {
                    if f.part != Part::G {
                        continue;
                    }
                    let (area, sign) = orientation(&v, &n, fi);
                    assert!(area > 0.0, "{what}: cara G {fi} degenerada");
                    if sign != orientation(&sv, &sn, fi).1 {
                        flips += 1;
                    }
                }
                assert_eq!(
                    flips, 0,
                    "{what}: caras G invertidas respecto al tubo recto"
                );
                for (c, x) in n.iter().enumerate() {
                    worst_normal = worst_normal.max(
                        (Vec3::from_array(*x).length() - Vec3::from_array(rn[c]).length()).abs(),
                    );
                }
                cases += 1;
            }
        }
    }
    println!("casos {cases}; error maximo de norma de normales vs reposo {worst_normal:e}");
    assert!(worst_normal < 1e-9);
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_spatial_rear_azimuth_and_planar_sweep_are_continuous_on_the_real_arm() {
    let (_w, _g, p) = real_game(0.05);
    let rig = p.rig().unwrap();
    let legacy = rig.base();
    let template = p.debug().package().data().template();
    let buffers = || (template.vertices.clone(), template.normals.clone());
    let rest = rig.rest_loop();
    let (cp, t) = (
        Vec3::from_array(rest.center),
        Vec3::from_array(rest.tangent),
    );
    let s = t.cross(Vec3::Y).normalized();
    let u = s.cross(t).normalized();
    let jump = |a: &[[f64; 3]], b: &[[f64; 3]]| {
        a.iter()
            .zip(b)
            .map(|(a, b)| (Vec3::from_array(*a) - Vec3::from_array(*b)).length())
            .fold(0.0, f64::max)
    };
    // Control RED nativo: el solucionador plano salta en el polo trasero del brazo REAL.
    let (a, b) = (
        cp + (-t * 1e-5_f64.cos() + s * 1e-5_f64.sin()),
        cp + (-t * 1e-5_f64.cos() + u * 1e-5_f64.sin()),
    );
    let (mut av, mut an) = buffers();
    let pa = ok(
        legacy.deform([a.x, a.y, a.z], None, &mut av, &mut an),
        "plano a",
    );
    let (mut bv, mut bn) = buffers();
    ok(
        legacy.deform([b.x, b.y, b.z], Some(pa.bend_side), &mut bv, &mut bn),
        "plano b",
    );
    let legacy_jump = jump(&av, &bv);
    assert!(
        legacy_jump > 8.0 * (b - a).length() + 1e-5,
        "control plano real: salto {legacy_jump}"
    );
    let mut worst: f64 = 0.0;
    let mut frames = 0;
    let mut violations: Vec<String> = Vec::new();
    for radius in [rig.min_radius() * (1.0 + 1e-9), 1.0, 3.0] {
        // LP-R01 real (todos los vertices) + vuelta de azimut alrededor del polo trasero.
        for eps in [1e-2_f64, 1e-3, 1e-4, 1e-5] {
            let a = cp + (-t * eps.cos() + s * eps.sin()) * radius;
            let b = cp + (-t * eps.cos() + u * eps.sin()) * radius;
            let (mut av, mut an) = buffers();
            let pa = ok(rig.deform([a.x, a.y, a.z], None, &mut av, &mut an), "a");
            let (mut bv, mut bn) = buffers();
            ok(
                rig.deform([b.x, b.y, b.z], Some(pa.memory), &mut bv, &mut bn),
                "b",
            );
            let (step, j) = ((b - a).length(), jump(&av, &bv));
            // Diagnostico: misma memoria SIN giro (paso de memoria nulo) = parte debida solo a la punta.
            let fixed = SpatialMemory {
                side: pa.memory.side,
                tip_obj: [b.x, b.y, b.z],
            };
            let (mut fv, mut fnn) = buffers();
            ok(
                rig.deform([b.x, b.y, b.z], Some(fixed), &mut fv, &mut fnn),
                "b fija",
            );
            let target_only = jump(&av, &fv);
            println!(
                "R01 real r {radius:.4} eps {eps:e}: paso {step:e} salto {j:e} (ganancia {:.3}; solo punta {:.3})",
                j / step,
                target_only / step
            );
            if j > 8.0 * step + 1e-5 {
                violations.push(format!("R01 r {radius} eps {eps}: {j} para {step}"));
            }
            let mut memory = None;
            let mut previous: Option<(Vec<[f64; 3]>, Vec3)> = None;
            for i in 0..=720 {
                let phi = (i as f64).to_radians();
                let tip =
                    cp + (-t * eps.cos() + (s * phi.cos() + u * phi.sin()) * eps.sin()) * radius;
                let (mut v, mut n) = buffers();
                let pose = ok(
                    rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                    "yaw",
                );
                assert!((Vec3::from_array(pose.axial_tip) - tip).length() < 1e-9);
                if let Some((pv, pt)) = &previous {
                    let (step, j) = ((tip - *pt).length(), jump(pv, &v));
                    if j > 8.0 * step + 1e-5 {
                        violations.push(format!("yaw r {radius} eps {eps} {i}: {j} para {step}"));
                    }
                    worst = worst.max(j / step);
                }
                frames += 1;
                memory = Some(pose.memory);
                previous = Some((v, tip));
            }
        }
        // LP-R02 real: barrido plano 60..300 en el plano (t, s), subdivisiones 1/4/20.
        for subdivisions in [1_usize, 4, 20] {
            let mut memory = None;
            let mut previous: Option<(Vec<[f64; 3]>, Vec3)> = None;
            for i in 0..=240 * subdivisions {
                let q = (60.0 + i as f64 / subdivisions as f64).to_radians();
                let tip = cp + (t * q.cos() + s * q.sin()) * radius;
                let (mut v, mut n) = buffers();
                let pose = ok(
                    rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
                    "barrido",
                );
                assert!((Vec3::from_array(pose.axial_tip) - tip).length() < 1e-9);
                if let Some((pv, pt)) = &previous {
                    let (step, j) = ((tip - *pt).length(), jump(pv, &v));
                    if j > 8.0 * step + 1e-5 {
                        violations.push(format!(
                            "R02 r {radius} sub {subdivisions} {i}: {j} para {step}"
                        ));
                    }
                    worst = worst.max(j / step);
                }
                frames += 1;
                memory = Some(pose.memory);
                previous = Some((v, tip));
            }
        }
    }
    println!(
        "control plano real: salto {legacy_jump}; espacial: {frames} cuadros, ganancia maxima salto/paso {worst}"
    );
    for prefix in ["R01 r", "yaw r", "R02 r"] {
        let mut by: std::collections::BTreeMap<String, usize> = Default::default();
        for v in violations.iter().filter(|v| v.starts_with(prefix)) {
            let key = v.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
            *by.entry(key).or_default() += 1;
        }
        println!("violaciones {prefix}: {by:?}");
    }
    println!(
        "violaciones {}: {:?}",
        violations.len(),
        violations.iter().take(12).collect::<Vec<_>>()
    );
    assert!(
        violations.is_empty(),
        "continuidad real (8*paso+1e-5): {} violaciones",
        violations.len()
    );
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_spatial_pursuit_follows_the_ship_and_keeps_the_root_exact() {
    let (mut w, g, mut p) = real_game(0.05);
    let meshy: Vec<[f64; 3]> = w.creature().unwrap().model().vertices.clone();
    let corr = p.debug().correspondence().arm_vertex().to_vec();
    let topo_vertices: Vec<GrowthVertex> =
        p.debug().package().data().topology().vertices().to_vec();
    let root: Vec<usize> = p.debug().package().data().topology().root().to_vec();
    let mut reached_at = None;
    for i in 0..600 {
        let input = if i < 90 {
            thrust(1.0, 0.6)
        } else {
            thrust(0.0, 0.0)
        };
        let s = fly(&mut w, &mut p, input);
        if !s.step.target_clamped {
            assert_eq!(s.step.target, w.ship.position);
        }
        assert!(s.step.moved <= 48.0 * STEP + 1e-6);
        let model = p.composite().unwrap();
        let (nv, _) = p.body_prefix().unwrap();
        for &k in &root {
            if let GrowthVertex::Retained {
                source: VertexSource::Orig { orig, .. },
                ..
            } = topo_vertices[k]
            {
                assert_eq!(
                    model.vertices[nv + k],
                    meshy[corr[orig]],
                    "cuadro {i}: raiz {k}"
                );
            }
        }
        if reached_at.is_none() && s.step.reached && s.step.mode == PursuitMode::Pursuing {
            reached_at = Some(i);
        }
    }
    let s = p.last().unwrap();
    println!(
        "alcanzo la nave en el cuadro {reached_at:?}; distancia final {}",
        s.tip_ship_distance
    );
    assert!(reached_at.is_some() && s.tip_ship_distance < 1e-6);
    let (with, _) = frame(&w, &g, true, p.body_override().as_ref());
    let (without, _) = frame(&w, &g, true, None);
    assert_ne!(with.pixels, without.pixels);
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_spatial_oblique_turning_body_never_freezes_the_pursuit() {
    let (mut w, mut g, mut p) = real_game(0.05);
    ok(
        g.apply(
            &mut w,
            SolarKeys {
                oblique: true,
                ..SolarKeys::default()
            },
        ),
        "oblicua",
    );
    ok(p.bind(&w), "bind tras la vista");
    ok(p.step(&w, 0.0), "arranque");
    for i in 0..300 {
        advance_frame(&mut w, &ShipInput::default(), STEP);
        let s = p.step(&w, STEP);
        assert!(s.is_ok(), "cuadro {i}: {:?}", s.err());
    }
    let s = p.last().unwrap();
    assert_eq!(s.step.mode, PursuitMode::Pursuing);
    println!("OBLIQUA real: punta-nave {} tras 10 s", s.tip_ship_distance);
}

// Fixture sintetico: COPIA LITERAL del heredado (ver cabecera del archivo incluido).
include!("support/spatial_pursuit_fixture.rs");

// Variante gruesa publica del revisor, REUTILIZADA sin editar (control sano heredado de LP-R06).
#[path = "support/live_pursuit_review_thick.rs"]
mod thick;

// FIX01 (reintento acotado): regresion rapida APPEND-ONLY; el prefijo anterior queda byte-exacto.
#[path = "support/spatial_retry_regression.rs"]
mod spatial_retry_regression;

// FIX01: guarda APPEND-ONLY de las clausulas valiosas de sp10 (sp10 original intacto).
#[path = "support/spatial_retry_sp10_clauses.rs"]
mod spatial_retry_sp10_clauses;

// Parent-owned preparation: calibration/clip checks and first contact-contract RED, no product edits.
#[path = "support/grasp_preflight.rs"]
mod grasp_preflight;
