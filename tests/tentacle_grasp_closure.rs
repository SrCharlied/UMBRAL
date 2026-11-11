//! GT1 P1: preparacion local autorada + cierre (`GraspClosure`). Target PROPIO; ningun test heredado se
//! edita. Cadena real: World -> controlador -> SpatialPursuit::step (llegada) -> B1 (con el adaptador del
//! clip) -> Contact -> P1 -> malla. Assets privados opt-in (`#[ignore]`, claves de BASELINE.json).
use umbral::{
    app::advance_frame,
    assets::{
        obj::{Face, Model, load_model},
        ship_model_path,
    },
    math::{Mat4, Vec3},
    scene::{
        ship::ShipInput,
        world::{SHIP_MODEL_YAW, World},
    },
    visual::{
        solar_game::{SolarGame, solar_creature_placement},
        static_creature::StaticCreature,
        tentacle_grasp_closure::*,
        tentacle_grasp_handoff::*,
        tentacle_grasp_transition::CONTACT_TOLERANCE_OBJ,
        tentacle_growth_cpu::{GrowthPackage, GrowthVertex, Part, Side, VertexSource},
        tentacle_live_pursuit::{LivePursuitRequest, MAX_PURSUIT_DT, PursuitMode},
        tentacle_spatial_pursuit::SpatialPursuit,
    },
};

type V = [f64; 3];
const STEP: f64 = 1.0 / 30.0;
const PRIVATE: &str = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE";

fn ok<T>(r: Result<T, String>, what: &str) -> T {
    assert!(r.is_ok(), "{what}: {:?}", r.as_ref().err());
    r.unwrap()
}
fn v3(a: V) -> Vec3 {
    Vec3::from_array(a)
}
fn real_env(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k} ({PRIVATE})")))
}
fn finite(a: &[V]) -> bool {
    a.iter().all(|p| p.iter().all(|x| x.is_finite()))
}

fn real_game(oxygen: f64) -> (World, SolarGame, SpatialPursuit) {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 1280.0 / 720.0).unwrap();
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
    assert_eq!(report.capture.shared_root_vertices, 27);
    (world, game, p)
}
fn arm_rest(world: &World, p: &SpatialPursuit) -> Vec<V> {
    p.debug()
        .correspondence()
        .arm_vertex()
        .iter()
        .map(|&i| world.creature().unwrap().model().vertices[i])
        .collect()
}
fn ship_matrix(world: &World) -> Mat4 {
    world.ship.transform(SHIP_MODEL_YAW).matrix()
}
fn body(world: &World) -> Mat4 {
    world.creature_render_transform().unwrap().matrix()
}
fn centroid(points: &[V], indices: &[usize]) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for &i in indices {
        sum += v3(points[i]);
    }
    sum * (1.0 / indices.len() as f64)
}
fn max_jump(a: &[V], b: &[V]) -> f64 {
    let mut m: f64 = 0.0;
    for (x, y) in a.iter().zip(b) {
        let d = (v3(*x) - v3(*y)).length();
        assert!(d.is_finite(), "salto no finito");
        m = m.max(d);
    }
    m
}
/// Inversa EXACTA de la parte afin (Cramer), independiente del producto.
fn exact_local(m: &Mat4, p: Vec3) -> Vec3 {
    let (a, b, c) = (m.column(0), m.column(1), m.column(2));
    let r = p - m.column(3);
    let det = a.dot(b.cross(c));
    Vec3::new(
        r.dot(b.cross(c)) / det,
        a.dot(r.cross(c)) / det,
        a.dot(b.cross(r)) / det,
    )
}
fn template_model(package: &GrowthPackage) -> Model {
    let t = package.data().template();
    Model {
        vertices: t.vertices.clone(),
        uvs: t.uvs.clone(),
        normals: t.normals.clone(),
        faces: t
            .faces
            .iter()
            .map(|f| Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: t.material_libraries.clone(),
        materials: t.materials.clone(),
        bounds_center: t.bounds_center,
        bounds_radius: t.bounds_radius,
    }
}
/// Ancla ship-local derivada por la prueba (evaluate del grip_frame en una malla temporal + Cramer).
fn test_anchor(package: &GrowthPackage) -> Vec3 {
    let manifest = package.capture().manifest();
    let frame = manifest.grip_frame();
    let mut model = template_model(package);
    ok(
        package.data().evaluate(frame, [0.0; 3], &mut model),
        "evaluate grip",
    );
    let contact = centroid(&model.vertices, package.data().topology().grip());
    exact_local(&manifest.ship_obj_matrix(frame).unwrap(), contact)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    Scene,
    Straight,
    Oblique,
    Turned,
}

/// Escena real con la persecucion LLEGADA (10 cuadros `reached` seguidos) y su instantanea publicada.
struct Scene {
    world: World,
    p: SpatialPursuit,
    reached: ReachedPose,
    ship: Mat4,
    body: Mat4,
}

fn scene(case: Case) -> Scene {
    let (mut world, _g, mut p) = real_game(0.05);
    let rest = arm_rest(&world, &p);
    let probe = ok(ClosureClip::new(p.debug().package(), &rest), "clip sonda");
    let r = probe.adapter().rig().rest_loop();
    let (cp, t) = (v3(r.center), v3(r.tangent));
    let side = t.cross(Vec3::Y).normalized();
    let up = side.cross(t).normalized();
    let place = match case {
        Case::Straight => Some(cp + t * 3.0),
        Case::Oblique => Some(cp + (-t * 0.4 + side * 0.8 + up * 0.2).normalized() * 3.0),
        _ => None,
    };
    drop(probe);
    if let Some(q) = place {
        world.ship.position = body(&world).transform_point(q);
    }
    let (body0, ship0) = (body(&world), ship_matrix(&world));
    if case == Case::Turned {
        for (input, n) in [
            (
                ShipInput {
                    yaw: 0.7,
                    ..ShipInput::default()
                },
                90,
            ),
            (
                ShipInput {
                    pitch: 0.6,
                    roll: -0.8,
                    ..ShipInput::default()
                },
                90,
            ),
        ] {
            for _ in 0..n {
                advance_frame(&mut world, &input, STEP);
                ok(p.step(&world, STEP), "paso real");
            }
        }
    }
    let (mut streak, mut frames) = (0, 0);
    while streak < 10 {
        advance_frame(&mut world, &ShipInput::default(), STEP);
        let s = ok(p.step(&world, STEP), "paso real");
        frames += 1;
        assert!(frames < 2000, "{case:?}: la persecucion no llego");
        streak = if s.step.reached { streak + 1 } else { 0 };
    }
    assert_eq!(p.last().unwrap().step.mode, PursuitMode::Pursuing);
    if case == Case::Turned {
        assert_ne!(body(&world), body0, "el cuerpo giro");
        let s = ship_matrix(&world);
        let turned = (s.transform_dir(Vec3::X) - ship0.transform_dir(Vec3::X)).length()
            + (s.transform_dir(Vec3::Y) - ship0.transform_dir(Vec3::Y)).length();
        assert!(turned > 0.2, "orientacion de nave no trivial: {turned}");
    }
    let reached = ok(ReachedPose::capture(&p), "instantanea");
    Scene {
        ship: ship_matrix(&world),
        body: body(&world),
        world,
        p,
        reached,
    }
}

fn clip_of<'p>(sc: &'p Scene) -> ClosureClip<'p> {
    ok(
        ClosureClip::new(sc.p.debug().package(), &arm_rest(&sc.world, &sc.p)),
        "clip",
    )
}

/// B1 creado con el adaptador DEL CLIP y llevado hasta Contact.
fn contact<'a>(sc: &Scene, clip: &'a ClosureClip) -> GraspHandoff<'a> {
    let mut h = ok(
        GraspHandoff::start(
            clip.adapter(),
            &sc.reached,
            &sc.ship,
            &sc.body,
            HANDOFF_CANDIDATE_DURATION_S,
        ),
        "B1",
    );
    while h.status().phase == HandoffPhase::Aligning {
        ok(h.advance(&sc.ship, &sc.body, STEP), "B1 paso");
    }
    h
}

fn snapshot(c: &GraspClosure) -> (String, Vec<V>, Vec<V>) {
    (
        format!("{:?} {:?}", c.status(), c.pose()),
        c.vertices().to_vec(),
        c.normals().to_vec(),
    )
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c01_first_frame_is_exactly_the_b1_contact_and_dt0_changes_nothing() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let h = contact(&sc, &clip);
    let mut c = ok(GraspClosure::start(&clip, &h), "P1");
    let jump = max_jump(c.vertices(), h.vertices());
    println!("GT_P1 primer cuadro: salto contra Contact B1 {jump:e} OBJ");
    assert!(
        c.vertices() == h.vertices() && c.normals() == h.normals(),
        "GT-P1-C01: el primer cuadro no es el Contact de B1 (salto {jump} OBJ)"
    );
    assert_eq!(c.pose(), h.pose());
    assert_eq!(c.memory(), h.memory());
    let s = c.status();
    assert_eq!(
        (s.phase, s.frame, s.elapsed_s),
        (ClosurePhase::Preparing, clip.reference_frame(), 0.0)
    );
    let before = snapshot(&c);
    for _ in 0..3 {
        ok(c.advance(&sc.ship, &sc.body, 0.0), "dt 0");
        assert_eq!(snapshot(&c), before, "dt 0 no cambia nada");
    }
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c02_start_requires_the_clip_own_adapter_and_a_contact_handoff() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let other = clip_of(&sc);
    // Otro clip del MISMO paquete y reposo: adaptador distinto, no es una capacidad de identidad.
    let foreign = contact(&sc, &other);
    let r = GraspClosure::start(&clip, &foreign);
    assert!(r.is_err(), "GT-P1-C02: adaptador ajeno aceptado");
    println!("GT_P1 adaptador ajeno: {}", r.err().unwrap());
    // Traspaso aun alineando: no hay contacto que cerrar.
    let aligning = ok(
        GraspHandoff::start(
            clip.adapter(),
            &sc.reached,
            &sc.ship,
            &sc.body,
            HANDOFF_CANDIDATE_DURATION_S,
        ),
        "B1",
    );
    let r = GraspClosure::start(&clip, &aligning);
    assert!(r.is_err(), "GT-P1-C02: traspaso sin Contact aceptado");
    println!("GT_P1 sin contacto: {}", r.err().unwrap());
    let _ = (MAX_PURSUIT_DT, CONTACT_TOLERANCE_OBJ, finite(&[]));
}

/// Posiciones autoradas del cuadro `f` para los vertices retenidos (ruta INDEPENDIENTE del producto:
/// `base_positions` + `VertexSource`, sin `evaluate` ni marcos).
fn authored(package: &GrowthPackage, f: usize, ids: &[usize]) -> Vec<Vec3> {
    let base = package.data().base_positions(f).unwrap();
    let topo = package.data().topology();
    ids.iter()
        .map(|&i| match topo.vertices()[i] {
            GrowthVertex::Retained { source, .. } => match source {
                VertexSource::Orig { orig, .. } => v3(base[orig]),
                VertexSource::Cut { a, b, t } => v3(base[a]) + (v3(base[b]) - v3(base[a])) * t,
            },
            GrowthVertex::Ring { .. } => panic!("no retenido"),
        })
        .collect()
}

struct Groups {
    sample: Vec<usize>,
    loop_probe: Vec<usize>,
    dset: std::collections::HashSet<usize>,
    loopset: std::collections::HashSet<usize>,
}
fn groups(package: &GrowthPackage) -> Groups {
    let topo = package.data().topology();
    let loopset: std::collections::HashSet<usize> = topo.loop_d().iter().copied().collect();
    let d: Vec<usize> = topo
        .vertices()
        .iter()
        .enumerate()
        .filter(|(i, v)| {
            matches!(v, GrowthVertex::Retained { side: Side::D, .. }) && !loopset.contains(i)
        })
        .map(|(i, _)| i)
        .collect();
    let sample: Vec<usize> = d.iter().copied().step_by(11).collect();
    let l = topo.loop_d();
    let loop_probe = vec![l[0], l[l.len() / 3], l[2 * l.len() / 3]];
    let dset = topo
        .vertices()
        .iter()
        .enumerate()
        .filter(|(_, v)| matches!(v, GrowthVertex::Retained { side: Side::D, .. }))
        .map(|(i, _)| i)
        .collect();
    Groups {
        sample,
        loop_probe,
        dset,
        loopset,
    }
}

/// Oraculo de forma LOCAL: (error max de distancias internas del distal, error max de distancias distal-
/// corte) del cuadro publicado frente al cuadro autorado `f`.
fn local_shape_errors(package: &GrowthPackage, g: &Groups, f: usize, v: &[V]) -> (f64, f64) {
    let a = authored(package, f, &g.sample);
    let la = authored(package, f, &g.loop_probe);
    let (mut inner, mut attach) = (0.0f64, 0.0f64);
    for x in 0..g.sample.len() {
        for y in x + 1..g.sample.len() {
            let ours = (v3(v[g.sample[x]]) - v3(v[g.sample[y]])).length();
            let e = (ours - (a[x] - a[y]).length()).abs();
            assert!(e.is_finite());
            inner = inner.max(e);
        }
        for (k, &l) in g.loop_probe.iter().enumerate() {
            let ours = (v3(v[g.sample[x]]) - v3(v[l])).length();
            attach = attach.max((ours - (a[x] - la[k]).length()).abs());
        }
    }
    (inner, attach)
}

fn fnorm(v: &[V], f: [usize; 3]) -> Vec3 {
    let [a, b, c] = f.map(|i| v3(v[i]));
    (b - a).cross(c - a)
}
/// Caras D cuya normal geometrica se opone a la suma de sus normales de esquina.
fn incoherent_d(package: &GrowthPackage, v: &[V], n: &[V]) -> usize {
    package
        .data()
        .topology()
        .faces()
        .iter()
        .enumerate()
        .filter(|(_, f)| f.part == Part::D)
        .filter(|(fi, f)| {
            let cn = v3(n[3 * fi]) + v3(n[3 * fi + 1]) + v3(n[3 * fi + 2]);
            fnorm(v, f.v).dot(cn) <= 0.0
        })
        .count()
}

/// Estructura del cuadro publicado: igual a `deform(punta, memoria publicada)` salvo los vertices D fuera
/// del corte y las normales de esquina D; raiz/proximal/corte/anillos/normales G exactos; todo finito.
fn check_structure(sc: &Scene, clip: &ClosureClip, g: &Groups, c: &GraspClosure, what: &str) {
    let rig = clip.adapter().rig();
    let (v, n) = (c.vertices(), c.normals());
    assert!(finite(v) && finite(n), "{what}: no finito");
    let (mut bv, mut bn) = (v.to_vec(), n.to_vec());
    ok(
        rig.deform(c.pose().tip_obj, Some(c.memory()), &mut bv, &mut bn),
        "reproduccion",
    );
    let topo = sc.p.debug().package().data().topology();
    for &i in topo.root() {
        assert_eq!(v[i], rig.rest_vertices()[i], "{what}: raiz {i}");
    }
    for (i, p) in v.iter().enumerate() {
        if !g.dset.contains(&i) || g.loopset.contains(&i) {
            assert_eq!(*p, bv[i], "{what}: vertice {i} fuera del distal movido");
        }
    }
    for (fi, f) in topo.faces().iter().enumerate() {
        for k in 0..3 {
            let c = 3 * fi + k;
            assert!(
                (v3(n[c]).length() - v3(rig.rest_normals()[c]).length()).abs() <= 1e-9,
                "{what}: norma de normal {c}"
            );
            if f.part != Part::D {
                assert_eq!(n[c], bn[c], "{what}: normal no distal {c}");
            }
        }
    }
}

/// Recorre P1 entero con `dt`, comprobando cada cuadro nuevo contra el oraculo; devuelve
/// (cuadros publicados, salto max, error interno max, error de union max, contactos por cuadro).
fn run_all(
    sc: &Scene,
    clip: &ClosureClip,
    c: &mut GraspClosure,
    what: &str,
    dt: f64,
) -> (Vec<usize>, f64, f64, f64, Vec<(usize, f64)>) {
    let package = sc.p.debug().package();
    let g = groups(package);
    let cap = package.capture();
    let mut frames = vec![c.status().frame];
    let (mut jump, mut inner, mut attach) = (0.0f64, 0.0f64, 0.0f64);
    let mut gaps = Vec::new();
    let mut prev = c.vertices().to_vec();
    let mut elapsed = 0.0;
    while c.status().phase != ClosurePhase::Gripped {
        assert!(frames.len() < 100_000);
        let s = ok(c.advance(&sc.ship, &sc.body, dt), what);
        assert!(
            s.elapsed_s > elapsed || s.phase == ClosurePhase::Gripped,
            "{what}: reloj"
        );
        elapsed = s.elapsed_s;
        let expect = if s.elapsed_s >= s.duration_s {
            clip.grip_frame()
        } else {
            cap.frame_at((clip.reference_frame() - 1) as f64 / cap.fps() as f64 + s.elapsed_s)
                .unwrap()
                .min(clip.grip_frame())
        };
        assert_eq!(s.frame, expect, "{what}: muestreo floor del paquete");
        let phase = if s.frame == clip.grip_frame() {
            ClosurePhase::Gripped
        } else if s.frame >= clip.closing_frame() {
            ClosurePhase::Closing
        } else {
            ClosurePhase::Preparing
        };
        assert_eq!(s.phase, phase, "{what}: fase del cuadro {}", s.frame);
        if s.frame != *frames.last().unwrap() {
            check_structure(sc, clip, &g, c, &format!("{what} f{}", s.frame));
            let (i, a) = local_shape_errors(package, &g, s.frame, c.vertices());
            inner = inner.max(i);
            attach = attach.max(a);
            let j = max_jump(c.vertices(), &prev);
            assert!(
                (s.last_step_obj - j).abs() <= 1e-12,
                "{what}: salto declarado"
            );
            jump = jump.max(j);
            let mut model = template_model(package);
            ok(
                package.data().evaluate(s.frame, [0.0; 3], &mut model),
                "clip",
            );
            assert_eq!(
                incoherent_d(package, c.vertices(), c.normals()),
                incoherent_d(package, &model.vertices, &model.normals),
                "{what} f{}: caras D incoherentes distintas de las del clip",
                s.frame
            );
            gaps.push((s.frame, s.contact_error_obj));
            frames.push(s.frame);
            prev = c.vertices().to_vec();
        } else {
            assert_eq!(
                c.vertices(),
                &prev[..],
                "{what}: cuadro repetido sin cambios"
            );
        }
    }
    (frames, jump, inner, attach, gaps)
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c03_authored_local_preparation_and_closure_are_reproduced_frame_by_frame() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let h = contact(&sc, &clip);
    let mut c = ok(GraspClosure::start(&clip, &h), "P1");
    let package = sc.p.debug().package();
    let g = groups(package);
    let (frames, jump, inner, attach, _) = run_all(&sc, &clip, &mut c, "scene", STEP);
    // El gesto realmente cambio la forma (no un cierre "a angulo 0"): distancias internas contra B1.
    let (b1_inner, _) = local_shape_errors(package, &g, clip.grip_frame(), h.vertices());
    println!(
        "GT_P1 {{\"case\":\"scene\",\"published_frames\":{},\"max_jump_obj\":{jump:e},\"inner_err_obj\":{inner:e},\"attach_err_obj\":{attach:e},\"b1_shape_vs_grip_frame_obj\":{b1_inner:e}}}",
        frames.len()
    );
    assert_eq!(*frames.first().unwrap(), clip.reference_frame());
    assert_eq!(*frames.last().unwrap(), clip.grip_frame());
    assert!(frames.windows(2).all(|w| w[0] < w[1]));
    assert!(
        frames.contains(&clip.closing_frame()),
        "cierre 133 publicado"
    );
    assert!(
        inner <= 1e-5,
        "GT-P1-C03: forma local autorada no reproducida: {inner}"
    );
    assert!(attach <= 6e-3, "GT-P1-C03: union distal-corte: {attach}");
    assert!(b1_inner > 0.1, "la forma de B1 ya era la final? {b1_inner}");
    assert!(jump <= 0.06, "GT-P1-C03: salto de malla {jump}");
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c04_marker_boundaries_133_168_169_and_deterministic_hold() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let h = contact(&sc, &clip);
    let mut c = ok(GraspClosure::start(&clip, &h), "P1");
    let fps = sc.p.debug().package().capture().fps() as f64;
    let mut seen = Vec::new();
    for _ in 0..(clip.grip_frame() - clip.reference_frame()) {
        let s = ok(
            c.advance(&sc.ship, &sc.body, 1.0 / fps),
            "paso de un cuadro",
        );
        seen.push((s.frame, s.phase));
    }
    let at = |f: usize| seen.iter().find(|(x, _)| *x == f).map(|(_, p)| *p);
    assert_eq!(at(clip.closing_frame() - 1), Some(ClosurePhase::Preparing));
    assert_eq!(at(clip.closing_frame()), Some(ClosurePhase::Closing));
    assert_eq!(at(clip.grip_frame()), Some(ClosurePhase::Gripped));
    assert_eq!(seen.len(), clip.grip_frame() - clip.reference_frame());
    assert!(
        seen.iter().all(|(f, _)| *f <= clip.grip_frame()),
        "nunca agarre 169/arrastre"
    );
    let s = c.status();
    assert_eq!(
        (s.frame, s.phase, s.elapsed_s),
        (clip.grip_frame(), ClosurePhase::Gripped, s.duration_s)
    );
    let held = snapshot(&c);
    for dt in [STEP, 0.0, MAX_PURSUIT_DT, STEP] {
        ok(c.advance(&sc.ship, &sc.body, dt), "hold");
        assert_eq!(snapshot(&c), held, "hold dt {dt}");
    }
    let stages =
        sc.p.debug()
            .package()
            .capture()
            .manifest()
            .stages()
            .to_vec();
    println!(
        "GT_P1 marcadores {stages:?} grip_frame {}",
        clip.grip_frame()
    );
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c05_invalid_dt_and_changed_matrices_are_rejected_atomically_in_every_phase() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let h = contact(&sc, &clip);
    let mut c = ok(GraspClosure::start(&clip, &h), "P1");
    let mut moved = sc.ship;
    moved.m[0][3] += 1e-3;
    let turned = sc.body * Mat4::rotation_y(1e-6);
    let mut nan = sc.ship;
    nan.m[1][3] = f64::NAN;
    let check = |c: &mut GraspClosure, phase: &str| {
        let before = snapshot(c);
        for dt in [
            -1e-3,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            MAX_PURSUIT_DT * 1.5,
        ] {
            assert!(
                c.advance(&sc.ship, &sc.body, dt).is_err(),
                "{phase} dt {dt}"
            );
            assert_eq!(snapshot(c), before, "{phase} dt {dt}: nada cambia");
        }
        for (what, s, b) in [
            ("nave movida", moved, sc.body),
            ("cuerpo girado", sc.ship, turned),
            ("nave NaN", nan, sc.body),
        ] {
            for dt in [STEP, 0.0] {
                assert!(c.advance(&s, &b, dt).is_err(), "{phase} {what}");
                assert_eq!(snapshot(c), before, "{phase} {what}: nada cambia");
            }
        }
    };
    check(&mut c, "inicio");
    let fps = sc.p.debug().package().capture().fps() as f64;
    for _ in 0..40 {
        ok(c.advance(&sc.ship, &sc.body, 1.0 / fps), "preparacion");
    }
    assert_eq!(c.status().phase, ClosurePhase::Preparing);
    check(&mut c, "preparando");
    for _ in 0..100 {
        ok(c.advance(&sc.ship, &sc.body, 1.0 / fps), "hacia cierre");
    }
    assert_eq!(c.status().phase, ClosurePhase::Closing);
    check(&mut c, "cerrando");
    while c.status().phase != ClosurePhase::Gripped {
        ok(c.advance(&sc.ship, &sc.body, 1.0 / fps), "hasta grip");
    }
    check(&mut c, "agarrado");
}

/// Contacto en la MALLA PUBLICADA contra el ancla 168 transportada (ruta de la prueba: evaluate del
/// grip_frame + Cramer), proximal exacto y punta en el dominio.
fn check_contact(sc: &Scene, clip: &ClosureClip, c: &GraspClosure, what: &str) -> f64 {
    let package = sc.p.debug().package();
    let topo = package.data().topology();
    let desired = exact_local(&sc.body, sc.ship.transform_point(test_anchor(package)));
    let gap = (centroid(c.vertices(), topo.grip()) - desired).length();
    assert!(
        (c.status().contact_error_obj - gap).abs() <= 1e-9,
        "{what}: error declarado = real"
    );
    let rig = clip.adapter().rig();
    for (i, x) in topo.vertices().iter().enumerate() {
        if matches!(x, GrowthVertex::Retained { side: Side::P, .. }) {
            assert_eq!(
                c.vertices()[i],
                rig.rest_vertices()[i],
                "{what}: proximal {i}"
            );
        }
    }
    let r = (v3(c.pose().tip_obj) - v3(rig.rest_loop().center)).length();
    assert!(r >= rig.min_radius() && r.is_finite(), "{what}: punta {r}");
    gap
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c06_final_grip_lands_on_the_approved_anchor_in_every_real_case() {
    for case in [Case::Scene, Case::Straight, Case::Oblique, Case::Turned] {
        let what = format!("{case:?}");
        let sc = scene(case);
        let clip = clip_of(&sc);
        let h = contact(&sc, &clip);
        let mut c = ok(GraspClosure::start(&clip, &h), "P1");
        let tip0 = v3(c.pose().tip_obj);
        let (frames, jump, inner, attach, gaps) = run_all(&sc, &clip, &mut c, &what, STEP);
        let final_gap = check_contact(&sc, &clip, &c, &what);
        let worst = gaps.iter().map(|(_, g)| *g).fold(0.0, f64::max);
        let at_closing = gaps
            .iter()
            .find(|(f, _)| *f == clip.closing_frame())
            .map(|(_, g)| *g);
        println!(
            "GT_P1 {{\"case\":\"{what}\",\"frames\":{},\"final_gap_obj\":{final_gap:e},\"max_gap_during_gesture_obj\":{worst:e},\"gap_at_closing_obj\":{:e},\"max_jump_obj\":{jump:e},\"inner_err_obj\":{inner:e},\"attach_err_obj\":{attach:e},\"tip_travel_obj\":{:e}}}",
            frames.len(),
            at_closing.unwrap_or(f64::NAN),
            (v3(c.pose().tip_obj) - tip0).length()
        );
        assert!(
            final_gap <= CONTACT_TOLERANCE_OBJ,
            "GT-P1-C06 {what}: el grip final no esta en el ancla aprobada: {final_gap} OBJ"
        );
        assert!(
            inner <= 1e-5 && attach <= 6e-3 && jump <= 0.06,
            "{what}: forma/salto"
        );
    }
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c07_guard_same_authored_frames_give_identical_bits_for_any_sampling_dt() {
    let sc = scene(Case::Scene);
    let clip = clip_of(&sc);
    let h = contact(&sc, &clip);
    let fps = sc.p.debug().package().capture().fps() as f64;
    let mut finals = Vec::new();
    for dt in [1.0 / fps, STEP, STEP / 2.0] {
        let mut c = ok(GraspClosure::start(&clip, &h), "P1");
        let mut frames = vec![c.status().frame];
        while c.status().phase != ClosurePhase::Gripped {
            let s = ok(c.advance(&sc.ship, &sc.body, dt), "paso");
            if s.frame != *frames.last().unwrap() {
                frames.push(s.frame);
            }
        }
        assert_eq!(
            frames.len(),
            clip.grip_frame() - clip.reference_frame() + 1,
            "dt {dt}: todos los cuadros"
        );
        finals.push(snapshot(&c));
    }
    assert_eq!(finals[0].1, finals[1].1, "mismos bits 1/24 vs 1/30");
    assert_eq!(finals[1].1, finals[2].1, "mismos bits 1/30 vs 1/60");
    assert_eq!(finals[0].2, finals[2].2);
    // Paso grande: se saltan cuadros autorados; el contacto final sigue en el ancla.
    let mut c = ok(GraspClosure::start(&clip, &h), "P1");
    let mut published = 0;
    while c.status().phase != ClosurePhase::Gripped {
        let before = c.status().frame;
        let s = ok(c.advance(&sc.ship, &sc.body, 0.5), "paso grande");
        published += usize::from(s.frame != before);
    }
    let gap = check_contact(&sc, &clip, &c, "dt 0.5");
    println!(
        "GT_P1 {{\"case\":\"dt_0.5\",\"published_frames\":{published},\"final_gap_obj\":{gap:e}}}"
    );
    assert!(gap <= CONTACT_TOLERANCE_OBJ);
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn p1_c08_guard_constructor_rejects_a_foreign_rest_and_has_no_world_side_effects() {
    let sc = scene(Case::Scene);
    let ship_before = (sc.world.ship.position, sc.world.camera.view_projection());
    let rest = arm_rest(&sc.world, &sc.p);
    // Una traslacion uniforme del reposo es rigidamente equivalente (el marco del corte la absorbe); un
    // reposo AJENO cambia la forma: un vertice del grip desplazado 0.01 OBJ.
    let mut foreign = rest.clone();
    let g0 = sc.p.debug().package().capture().manifest().grip()[0];
    foreign[g0][0] += 0.01;
    let r = ClosureClip::new(sc.p.debug().package(), &foreign);
    assert!(r.is_err(), "reposo ajeno aceptado");
    let e = r.err().unwrap();
    println!("GT_P1 reposo ajeno: {e}");
    assert!(e.contains("referencia autorada"), "{e}");
    let clip = ok(ClosureClip::new(sc.p.debug().package(), &rest), "clip");
    assert_eq!(
        (
            clip.reference_frame(),
            clip.closing_frame(),
            clip.grip_frame()
        ),
        (1, 133, 168)
    );
    assert_eq!(
        (sc.world.ship.position, sc.world.camera.view_projection()),
        ship_before
    );
}
