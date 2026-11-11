//! GT1 fase B1: traspaso postura alcanzada -> contacto calibrado A (`GraspHandoff`). Target PROPIO; ningun
//! test heredado se edita. Cadena real: World -> controlador -> SpatialPursuit::step (hasta la llegada) ->
//! instantanea publicada -> B1 -> malla. Assets privados opt-in (`#[ignore]`, claves de BASELINE.json).
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
        tentacle_grasp_handoff::*,
        tentacle_grasp_transition::{CONTACT_TOLERANCE_OBJ, ContactAdapter},
        tentacle_growth_cpu::{GrowthPackage, GrowthVertex, Part, Side},
        tentacle_live_pursuit::{LivePursuitRequest, MAX_PURSUIT_DT, PursuitMode},
        tentacle_spatial_pursuit::SpatialPursuit,
    },
};

type V = [f64; 3];
const STEP: f64 = 1.0 / 30.0;
const DURATION: f64 = HANDOFF_CANDIDATE_DURATION_S;
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
/// Reposo REAL del brazo desde el World (misma via que `SpatialPursuit::bind`).
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
    assert!(!indices.is_empty());
    let mut sum = Vec3::ZERO;
    for &i in indices {
        assert!(points[i].iter().all(|x| x.is_finite()));
        sum += v3(points[i]);
    }
    sum * (1.0 / indices.len() as f64)
}
fn max_jump(a: &[V], b: &[V]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (v3(*x) - v3(*y)).length())
        .fold(0.0, f64::max)
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
/// Ancla ship-local derivada por la prueba (evaluate del cuadro aprobado en una malla temporal).
fn test_anchor(package: &GrowthPackage) -> Vec3 {
    let data = package.data();
    let manifest = package.capture().manifest();
    let frame = manifest.grip_frame();
    let t = data.template();
    let mut model = Model {
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
    };
    ok(data.evaluate(frame, [0.0; 3], &mut model), "evaluate grip");
    let contact = centroid(&model.vertices, data.topology().grip());
    exact_local(&manifest.ship_obj_matrix(frame).unwrap(), contact)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    Scene,
    Straight,
    Oblique,
    Turned,
    ObliqueNear,
}

/// Escena real con la persecucion LLEGADA (10 cuadros seguidos `reached`) y su instantanea publicada.
struct Scene {
    world: World,
    p: SpatialPursuit,
    adapter: ContactAdapter,
    reached: ReachedPose,
    ship: Mat4,
    body: Mat4,
    anchor: Vec3,
    straight: (Vec<V>, Vec<V>),
}

fn scene(case: Case) -> Scene {
    let (mut world, _g, mut p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(p.debug().package(), &arm_rest(&world, &p)),
        "adaptador",
    );
    let rest = adapter.rig().rest_loop();
    let (cp, t) = (v3(rest.center), v3(rest.tangent));
    let side = t.cross(Vec3::Y).normalized();
    let up = side.cross(t).normalized();
    let oblique = (-t * 0.4 + side * 0.8 + up * 0.2).normalized();
    let place = match case {
        Case::Straight => Some(cp + t * 3.0),
        Case::Oblique => Some(cp + oblique * 3.0),
        Case::ObliqueNear => Some(cp + oblique * 0.7),
        _ => None,
    };
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
    let mut streak = 0;
    let mut frames = 0;
    while streak < 10 {
        advance_frame(&mut world, &ShipInput::default(), STEP);
        let s = ok(p.step(&world, STEP), "paso real");
        frames += 1;
        assert!(frames < 2000, "{case:?}: la persecucion no llego");
        streak = if s.step.reached { streak + 1 } else { 0 };
    }
    let status = p.last().unwrap();
    assert_eq!(status.step.mode, PursuitMode::Pursuing);
    if case == Case::Turned {
        assert_ne!(body(&world), body0, "el cuerpo giro");
        let s = ship_matrix(&world);
        let turned = (s.transform_dir(Vec3::X) - ship0.transform_dir(Vec3::X)).length()
            + (s.transform_dir(Vec3::Y) - ship0.transform_dir(Vec3::Y)).length();
        assert!(turned > 0.2, "orientacion de nave no trivial: {turned}");
    }
    let reached = ok(ReachedPose::capture(&p), "instantanea");
    let q = cp + t * 3.0;
    let template = p.debug().package().data().template();
    let (mut sv, mut sn) = (template.vertices.clone(), template.normals.clone());
    ok(
        adapter
            .rig()
            .deform([q.x, q.y, q.z], None, &mut sv, &mut sn),
        "tubo recto",
    );
    let anchor = test_anchor(p.debug().package());
    println!(
        "GT_B escena {case:?}: llegada en {frames} cuadros (+{} previos), punta-nave {}",
        if case == Case::Turned { 180 } else { 0 },
        status.tip_ship_distance
    );
    Scene {
        ship: ship_matrix(&world),
        body: body(&world),
        world,
        p,
        adapter,
        reached,
        anchor,
        straight: (sv, sn),
    }
}

/// Comprobaciones de un cuadro publicado: raiz27 y proximal exactos, normales con la norma de reposo,
/// caras G sin invertir respecto al tubo recto, todo finito.
fn check_frame(sc: &Scene, what: &str, v: &[V], n: &[V]) {
    let rig = sc.adapter.rig();
    let topo = sc.p.debug().package().data().topology();
    assert_eq!(topo.root().len(), 27);
    for &i in topo.root() {
        assert_eq!(v[i], rig.rest_vertices()[i], "{what}: raiz {i}");
    }
    for (i, tv) in topo.vertices().iter().enumerate() {
        if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
            assert_eq!(v[i], rig.rest_vertices()[i], "{what}: proximal {i}");
        }
    }
    assert!(v.iter().chain(n).all(|p| p.iter().all(|x| x.is_finite())));
    assert!(
        n.iter()
            .zip(rig.rest_normals())
            .all(|(a, b)| { (v3(*a).length() - v3(*b).length()).abs() <= 1e-9 })
    );
    let faces = topo.faces();
    let orient = |v: &[V], n: &[V], fi: usize| {
        let [p0, p1, p2] = faces[fi].v.map(|i| v3(v[i]));
        let g = (p1 - p0).cross(p2 - p0);
        let c = v3(n[3 * fi]) + v3(n[3 * fi + 1]) + v3(n[3 * fi + 2]);
        (g.length(), g.dot(c) > 0.0)
    };
    for (fi, f) in faces.iter().enumerate() {
        if f.part == Part::G {
            let (area, sign) = orient(v, n, fi);
            assert!(area > 0.0, "{what}: cara G {fi} degenerada");
            assert_eq!(
                sign,
                orient(&sc.straight.0, &sc.straight.1, fi).1,
                "{what}: cara G {fi} invertida"
            );
        }
    }
}

/// Lleva un traspaso hasta el contacto con `dt`, comprobando cada cuadro; devuelve los saltos de TODA la
/// malla por cuadro (el primero contra la postura alcanzada).
fn run_to_contact(sc: &Scene, h: &mut GraspHandoff, what: &str, dt: f64) -> Vec<f64> {
    let mut jumps = Vec::new();
    let mut prev = h.vertices().to_vec();
    let mut elapsed = h.status().elapsed_s;
    while h.status().phase == HandoffPhase::Aligning {
        assert!(jumps.len() < 100_000);
        let s = ok(h.advance(&sc.ship, &sc.body, dt), what);
        assert!(
            s.elapsed_s > elapsed && s.elapsed_s <= DURATION,
            "{what}: reloj"
        );
        elapsed = s.elapsed_s;
        let j = max_jump(h.vertices(), &prev);
        assert!(
            (s.last_step_obj - j).abs() <= 1e-12,
            "{what}: salto declarado = real"
        );
        check_frame(sc, what, h.vertices(), h.normals());
        jumps.push(j);
        prev = h.vertices().to_vec();
    }
    assert!(
        !jumps.is_empty(),
        "GT-B {what}: sin cuadros de alineacion (el traspaso fue instantaneo)"
    );
    jumps
}

/// Final: contacto publicado contra el ancla transportada (calculada por la prueba), estado = buffers y
/// la memoria final describe exactamente los buffers (reproduccion dt0 con el rig del adaptador).
fn check_contact(sc: &Scene, h: &GraspHandoff, what: &str) -> f64 {
    let s = h.status();
    assert_eq!(s.phase, HandoffPhase::Contact);
    assert_eq!(s.elapsed_s, DURATION);
    assert_eq!(s.progress, 1.0);
    let topo = sc.p.debug().package().data().topology();
    assert_eq!(topo.grip().len(), 105);
    let desired = exact_local(&sc.body, sc.ship.transform_point(sc.anchor));
    let gap = (centroid(h.vertices(), topo.grip()) - desired).length();
    let c = s.contact.expect("solucion A publicada");
    println!(
        "GT_B {{\"case\":\"{what}\",\"final_gap_obj\":{gap:e},\"tolerance_obj\":{CONTACT_TOLERANCE_OBJ}}}"
    );
    assert!(
        gap <= CONTACT_TOLERANCE_OBJ,
        "GT-B {what}: contacto final {gap} OBJ"
    );
    assert!(
        (c.contact_error_obj - gap).abs() <= 1e-9,
        "{what}: error A = real"
    );
    assert_eq!(c.pose, h.pose(), "{what}: pose A = publicada");
    assert_eq!(h.pose().memory, h.memory());
    let (mut rv, mut rn) = (h.vertices().to_vec(), h.normals().to_vec());
    ok(
        sc.adapter
            .rig()
            .deform(h.pose().tip_obj, Some(h.memory()), &mut rv, &mut rn),
        "reproduccion final",
    );
    assert!(
        rv == h.vertices() && rn == h.normals(),
        "{what}: la memoria final no describe los buffers publicados"
    );
    gap
}

fn snapshot(h: &GraspHandoff) -> (String, Vec<V>, Vec<V>) {
    (
        format!("{:?} {:?}", h.status(), h.pose()),
        h.vertices().to_vec(),
        h.normals().to_vec(),
    )
}

fn begin<'a>(sc: &'a Scene) -> GraspHandoff<'a> {
    ok(
        GraspHandoff::start(&sc.adapter, &sc.reached, &sc.ship, &sc.body, DURATION),
        "start",
    )
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b01_first_frame_and_dt0_are_exactly_the_published_reached_pose() {
    let sc = scene(Case::Scene);
    let (nv, nn) = sc.p.body_prefix().unwrap();
    let composite = sc.p.composite().unwrap();
    assert_eq!(sc.reached.vertices(), &composite.vertices[nv..]);
    assert_eq!(sc.reached.normals(), &composite.normals[nn..]);
    // Referencia de lo que se evita: conectar directamente a A.
    let (mut av, mut an) = (
        sc.reached.vertices().to_vec(),
        sc.reached.normals().to_vec(),
    );
    let direct = ok(
        sc.adapter.solve(
            sc.adapter.rig(),
            &sc.ship,
            &sc.body,
            Some(sc.reached.pose().memory),
            &mut av,
            &mut an,
        ),
        "A directo",
    );
    let mut h = begin(&sc);
    let first = max_jump(h.vertices(), sc.reached.vertices());
    println!(
        "GT_B primer cuadro: salto contra la postura alcanzada {first:e} OBJ (A directo saltaria {:e}; destino A a {:e} de la punta)",
        max_jump(&av, sc.reached.vertices()),
        (v3(direct.axial_target_obj) - v3(sc.reached.pose().tip_obj)).length()
    );
    assert!(
        h.vertices() == sc.reached.vertices() && h.normals() == sc.reached.normals(),
        "GT-B01: el primer cuadro no es la postura alcanzada (salto {first} OBJ)"
    );
    assert_eq!(h.pose(), sc.reached.pose());
    assert_eq!(Some(h.memory()), sc.p.memory());
    let s = h.status();
    assert_eq!(
        (s.phase, s.elapsed_s, s.progress, s.duration_s),
        (HandoffPhase::Aligning, 0.0, 0.0, DURATION)
    );
    assert_eq!(s.axial_target_obj, sc.reached.pose().tip_obj);
    let before = snapshot(&h);
    for _ in 0..3 {
        ok(h.advance(&sc.ship, &sc.body, 0.0), "dt 0");
        assert_eq!(snapshot(&h), before, "dt 0 no cambia nada");
    }
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b02_scene_handoff_reaches_the_calibrated_contact_without_jumps_then_holds() {
    let sc = scene(Case::Scene);
    let mut h = begin(&sc);
    let jumps = run_to_contact(&sc, &mut h, "scene", STEP);
    let max = jumps.iter().cloned().fold(0.0, f64::max);
    println!(
        "GT_B {{\"case\":\"scene\",\"dt\":{STEP},\"frames\":{},\"max_jump_obj\":{max:e},\"first_jump_obj\":{:e},\"last_jump_obj\":{:e}}}",
        jumps.len(),
        jumps[0],
        jumps[jumps.len() - 1]
    );
    assert!(
        (30..=31).contains(&jumps.len()),
        "cuadros {} para {DURATION} s",
        jumps.len()
    );
    assert!(max < 0.1, "GT-B02: salto de malla {max} OBJ");
    assert!(jumps[0] < 0.01, "primer paso {}", jumps[0]);
    check_contact(&sc, &h, "scene");
    // Hold determinista: ni dt 0 ni dt > 0 cambian nada ni avanzan el reloj.
    let held = snapshot(&h);
    for dt in [STEP, 0.0, MAX_PURSUIT_DT] {
        ok(h.advance(&sc.ship, &sc.body, dt), "hold");
        assert_eq!(snapshot(&h), held, "hold dt {dt}");
    }
    let _ = &sc.world;
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b03_mesh_jumps_shrink_with_dt_from_the_identical_reached_history() {
    let sc = scene(Case::Scene);
    let mut maxes = Vec::new();
    let mut firsts = Vec::new();
    for dt in [STEP, STEP / 2.0, STEP / 4.0] {
        let mut h = begin(&sc);
        let jumps = run_to_contact(&sc, &mut h, &format!("refine dt {dt}"), dt);
        let max = jumps.iter().cloned().fold(0.0, f64::max);
        println!(
            "GT_B {{\"case\":\"refine\",\"dt\":{dt},\"frames\":{},\"max_jump_obj\":{max:e},\"max_over_dt\":{},\"first_jump_obj\":{:e},\"last_jump_obj\":{:e}}}",
            jumps.len(),
            max / dt,
            jumps[0],
            jumps[jumps.len() - 1]
        );
        check_contact(&sc, &h, &format!("refine dt {dt}"));
        maxes.push(max);
        firsts.push(jumps[0]);
    }
    // Sin salto esencial: el maximo escala ~ dt y el primer salto se anula (~ dt^2 por smoothstep).
    assert!(maxes[1] <= 0.55 * maxes[0], "{maxes:?}");
    assert!(maxes[2] <= 0.3 * maxes[0], "{maxes:?}");
    assert!(
        firsts[1] <= 0.3 * firsts[0] && firsts[2] <= 0.1 * firsts[0],
        "{firsts:?}"
    );
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b04_straight_and_oblique_real_targets() {
    for (case, what) in [(Case::Straight, "straight"), (Case::Oblique, "oblique")] {
        let sc = scene(case);
        let mut h = begin(&sc);
        assert!(
            h.vertices() == sc.reached.vertices(),
            "{what}: primer cuadro"
        );
        let jumps = run_to_contact(&sc, &mut h, what, STEP);
        let max = jumps.iter().cloned().fold(0.0, f64::max);
        println!(
            "GT_B {{\"case\":\"{what}\",\"frames\":{},\"max_jump_obj\":{max:e}}}",
            jumps.len()
        );
        assert!(max < 0.1, "GT-B04 {what}: salto {max}");
        check_contact(&sc, &h, what);
    }
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b05_turned_body_and_ship_orientation_from_the_real_game() {
    let sc = scene(Case::Turned);
    let mut h = begin(&sc);
    assert!(h.vertices() == sc.reached.vertices(), "primer cuadro");
    let jumps = run_to_contact(&sc, &mut h, "turned", STEP);
    let max = jumps.iter().cloned().fold(0.0, f64::max);
    println!(
        "GT_B {{\"case\":\"turned\",\"frames\":{},\"max_jump_obj\":{max:e}}}",
        jumps.len()
    );
    assert!(max < 0.1, "GT-B05: salto {max}");
    check_contact(&sc, &h, "turned");
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b06_invalid_dt_and_pause_change_nothing_and_do_not_advance_the_clock() {
    let sc = scene(Case::Scene);
    let mut h = begin(&sc);
    let mut reference = begin(&sc);
    for _ in 0..5 {
        ok(h.advance(&sc.ship, &sc.body, STEP), "paso");
        ok(reference.advance(&sc.ship, &sc.body, STEP), "paso ref");
    }
    let before = snapshot(&h);
    for dt in [
        -1e-3,
        -0.0 - STEP,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        MAX_PURSUIT_DT * 1.5,
    ] {
        let r = h.advance(&sc.ship, &sc.body, dt);
        assert!(r.is_err(), "dt {dt}: Err esperado");
        assert_eq!(snapshot(&h), before, "dt {dt}: nada cambia");
    }
    ok(h.advance(&sc.ship, &sc.body, 0.0), "pausa");
    assert_eq!(snapshot(&h), before, "pausa");
    // El reloj no avanzo: el siguiente paso valido coincide bit a bit con una historia sin errores.
    ok(h.advance(&sc.ship, &sc.body, STEP), "paso tras errores");
    ok(reference.advance(&sc.ship, &sc.body, STEP), "paso ref");
    assert_eq!(snapshot(&h), snapshot(&reference), "determinismo");
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b07_changed_or_invalid_matrices_are_rejected_atomically_in_every_phase() {
    let sc = scene(Case::Scene);
    let mut h = begin(&sc);
    let mut moved = sc.ship;
    moved.m[0][3] += 1e-3;
    let turned = sc.body * Mat4::rotation_y(1e-6);
    let mut nan = sc.ship;
    nan.m[1][3] = f64::NAN;
    let check = |h: &mut GraspHandoff, phase: &str| {
        let before = snapshot(h);
        for (what, s, b) in [
            ("nave movida", moved, sc.body),
            ("cuerpo girado", sc.ship, turned),
            ("nave no finita", nan, sc.body),
        ] {
            for dt in [STEP, 0.0] {
                let r = h.advance(&s, &b, dt);
                assert!(r.is_err(), "{phase} {what} dt {dt}: Err esperado");
                assert_eq!(snapshot(h), before, "{phase} {what}: nada cambia");
            }
        }
    };
    check(&mut h, "inicio");
    for _ in 0..3 {
        ok(h.advance(&sc.ship, &sc.body, STEP), "paso");
    }
    check(&mut h, "alineando");
    run_to_contact(&sc, &mut h, "scene", STEP);
    check(&mut h, "contacto");
    check_contact(&sc, &h, "scene tras rechazos");
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b08_start_rejects_invalid_duration_idle_pursuit_and_foreign_binding() {
    let sc = scene(Case::Scene);
    for d in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            GraspHandoff::start(&sc.adapter, &sc.reached, &sc.ship, &sc.body, d).is_err(),
            "duracion {d}"
        );
    }
    let mut nan = sc.ship;
    nan.m[0][0] = f64::NAN;
    assert!(GraspHandoff::start(&sc.adapter, &sc.reached, &nan, &sc.body, DURATION).is_err());
    // Persecucion en reposo (O2 alto): no hay brazo publicado que capturar.
    let (mut w, _g, mut idle) = real_game(1.0);
    advance_frame(&mut w, &ShipInput::default(), STEP);
    ok(idle.step(&w, STEP), "paso en reposo");
    assert!(ReachedPose::capture(&idle).is_err(), "reposo sin brazo");
    // Enlace ajeno: adaptador de un reposo distinto no reproduce la postura publicada.
    let foreign: Vec<V> = arm_rest(&sc.world, &sc.p)
        .iter()
        .map(|q| [q[0] + 1e-6, q[1], q[2]])
        .collect();
    let other = ok(
        ContactAdapter::new(sc.p.debug().package(), &foreign),
        "adaptador ajeno",
    );
    let r = GraspHandoff::start(&other, &sc.reached, &sc.ship, &sc.body, DURATION);
    assert!(r.is_err(), "GT-B08: enlace ajeno aceptado");
    println!("GT_B enlace ajeno: {}", r.err().unwrap());
}

#[test]
#[ignore = "assets privados (ver PRIVATE)"]
fn gt_b09_contact_failure_mid_handoff_publishes_nothing_and_keeps_the_clock() {
    let sc = scene(Case::ObliqueNear);
    let mut h = begin(&sc);
    assert!(
        h.vertices() == sc.reached.vertices(),
        "primer cuadro exacto"
    );
    let before = snapshot(&h);
    let r = h.advance(&sc.ship, &sc.body, STEP);
    println!("GT_B fallo A a mitad: {:?}", r.as_ref().err());
    assert!(r.is_err(), "GT-B09: A sin solucion publicada");
    assert_eq!(snapshot(&h), before, "nada cambia, reloj quieto");
}
