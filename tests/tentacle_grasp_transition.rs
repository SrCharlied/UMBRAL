//! GT1 fase A: contacto anatomico (adaptador puro `ContactAdapter`). Target PROPIO; ningun test heredado
//! se edita. Assets reales privados opt-in (`#[ignore]`, mismas claves que el preflight). Los contactos se
//! miden en la malla PUBLICADA por la prueba (centroide de los 105 vertices grip), no en lo pedido.
use umbral::{
    app::advance_frame,
    assets::{obj::load_model, ship_model_path},
    math::{Mat4, Vec3},
    scene::{
        ship::ShipInput,
        world::{SHIP_MODEL_YAW, World},
    },
    visual::{
        solar_game::{SolarGame, solar_creature_placement},
        static_creature::StaticCreature,
        tentacle_grasp_transition::*,
        tentacle_growth_cpu::{GrowthPackage, GrowthVertex, Part, Side},
        tentacle_live_pursuit::{LivePursuitRequest, MAX_REACH_OBJ, PursuitMode},
        tentacle_spatial_pursuit::{SpatialMemory, SpatialPursuit, SpatialRig},
    },
};

type V = [f64; 3];
const STEP: f64 = 1.0 / 30.0;
/// Oraculo del padre (CONTACT02, preflight nativo): gap del solver raw en la nave de la escena.
const RAW_GAP_CONTACT02: f64 = 0.6087753648552583;
/// Oraculo del padre: ancla ship-local derivada del cuadro 168 (solo para comparar la derivacion).
const PARENT_ANCHOR: V = [-2.1572096132740435, 0.18263683544683468, -1.22987011253];

fn ok<T>(r: Result<T, String>, what: &str) -> T {
    assert!(r.is_ok(), "{what}: {:?}", r.as_ref().err());
    r.unwrap()
}
fn v3(a: V) -> Vec3 {
    Vec3::from_array(a)
}
fn real_env(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k}")))
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

fn centroid(points: &[V], indices: &[usize]) -> Vec3 {
    assert!(!indices.is_empty());
    let mut sum = Vec3::ZERO;
    for &i in indices {
        assert!(points[i].iter().all(|x| x.is_finite()));
        sum += v3(points[i]);
    }
    sum * (1.0 / indices.len() as f64)
}
/// Mundo -> OBJ para un transform de semejanza (independiente del producto).
fn to_obj(c: &Mat4, w: Vec3) -> Vec3 {
    let rel = w - c.column(3);
    let s2 = c.column(0).dot(c.column(0));
    Vec3::new(
        rel.dot(c.column(0)) / s2,
        rel.dot(c.column(1)) / s2,
        rel.dot(c.column(2)) / s2,
    )
}
fn ship_matrix(world: &World) -> Mat4 {
    world.ship.transform(SHIP_MODEL_YAW).matrix()
}
fn body(world: &World) -> Mat4 {
    world.creature_render_transform().unwrap().matrix()
}
/// Inversa EXACTA de la parte afin (Cramer), independiente del producto. `to_obj` (la del preflight)
/// supone columnas ortogonales; la matriz horneada de la nave no lo es del todo (d01: error de
/// reconstruccion 8.3e-10 OBJ del ancla del padre).
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
/// Centroide grip aprobado (evaluate del cuadro en una malla temporal, como el preflight) y matriz de
/// la nave del mismo cuadro.
fn approved_contact(package: &GrowthPackage) -> (Vec3, Mat4) {
    let data = package.data();
    let manifest = package.capture().manifest();
    let frame = manifest.grip_frame();
    let mut model = umbral::assets::obj::Model {
        vertices: data.template().vertices.clone(),
        uvs: data.template().uvs.clone(),
        normals: data.template().normals.clone(),
        faces: data
            .template()
            .faces
            .iter()
            .map(|f| umbral::assets::obj::Face {
                vertex_indices: f.vertex_indices,
                uv_indices: f.uv_indices,
                normal_indices: f.normal_indices,
                material: f.material.clone(),
            })
            .collect(),
        material_libraries: data.template().material_libraries.clone(),
        materials: data.template().materials.clone(),
        bounds_center: data.template().bounds_center,
        bounds_radius: data.template().bounds_radius,
    };
    ok(data.evaluate(frame, [0.0; 3], &mut model), "evaluate grip");
    let contact = centroid(&model.vertices, data.topology().grip());
    (contact, manifest.ship_obj_matrix(frame).unwrap())
}
/// Ancla ship-local exacta derivada por la prueba.
fn test_anchor(package: &GrowthPackage) -> Vec3 {
    let (contact, ship) = approved_contact(package);
    exact_local(&ship, contact)
}
/// Contacto deseado calculado por la prueba (cadena del preflight: nave dibujada -> mundo -> OBJ).
fn desired_obj(world_ship: &Mat4, body: &Mat4, anchor: Vec3) -> Vec3 {
    to_obj(body, world_ship.transform_point(anchor))
}
fn template_buffers(p: &SpatialPursuit) -> (Vec<V>, Vec<V>) {
    let t = p.debug().package().data().template();
    (t.vertices.clone(), t.normals.clone())
}
fn sentinel(p: &SpatialPursuit) -> (Vec<V>, Vec<V>) {
    let t = p.debug().package().data().template();
    (
        vec![[7.0; 3]; t.vertices.len()],
        vec![[7.0; 3]; t.normals.len()],
    )
}

/// Comprueba la malla PUBLICADA: contacto real, mundo, raiz27 y proximal exactos, punta axial en el
/// dominio, normales con la norma de reposo y caras G sin invertir respecto al tubo recto.
#[allow(clippy::too_many_arguments)]
fn check_published(
    what: &str,
    p: &SpatialPursuit,
    straight: &(Vec<V>, Vec<V>),
    world_ship: &Mat4,
    body: &Mat4,
    vertices: &[V],
    normals: &[V],
    s: &ContactSolution,
) {
    let rig = p.rig().unwrap();
    let data = p.debug().package().data();
    let topo = data.topology();
    assert_eq!(topo.grip().len(), 105);
    assert_eq!(topo.root().len(), 27);
    let anchor = test_anchor(p.debug().package());
    let desired = desired_obj(world_ship, body, anchor);
    let contact = centroid(vertices, topo.grip());
    let gap = (contact - desired).length();
    let world_gap = (body.transform_point(contact) - world_ship.transform_point(anchor)).length();
    let scale = body.column(0).length();
    let cp = v3(rig.rest_loop().center);
    let r = (v3(s.axial_target_obj) - cp).length();
    println!(
        "GT_A {{\"case\":\"{what}\",\"gap_obj\":{gap:e},\"gap_world\":{world_gap:e},\"raw_gap_obj\":{},\"iterations\":{},\"evaluations\":{},\"axial_r_obj\":{r},\"tolerance_obj\":{CONTACT_TOLERANCE_OBJ}}}",
        s.raw_contact_error_obj, s.iterations, s.evaluations
    );
    assert!(
        gap.is_finite() && gap <= CONTACT_TOLERANCE_OBJ,
        "GT-A {what}: el contacto anatomico publicado no coincide con el ancla aprobada: gap={gap} OBJ"
    );
    assert!(
        world_gap <= CONTACT_TOLERANCE_OBJ * scale,
        "{what}: mundo {world_gap}"
    );
    assert!(
        (v3(s.contact_obj) - contact).length() < 1e-12,
        "{what}: contacto declarado = real"
    );
    assert!(
        (v3(s.desired_contact_obj) - desired).length() < 1e-9,
        "{what}: deseado"
    );
    assert!(
        (s.contact_error_obj - gap).abs() < 1e-12,
        "{what}: error declarado = real"
    );
    for &i in topo.root() {
        assert_eq!(vertices[i], rig.rest_vertices()[i], "{what}: raiz {i}");
    }
    let mut proximal = 0;
    for (i, tv) in topo.vertices().iter().enumerate() {
        if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
            assert_eq!(vertices[i], rig.rest_vertices()[i], "{what}: proximal {i}");
            proximal += 1;
        }
    }
    assert!(proximal > 0);
    assert!(
        r >= rig.min_radius() * (1.0 - 1e-12) && r <= MAX_REACH_OBJ,
        "{what}: punta axial r {r}"
    );
    assert!(
        (v3(s.pose.axial_tip) - v3(s.axial_target_obj)).length() < 1e-9,
        "{what}: punta axial exacta"
    );
    assert!(normals.iter().zip(rig.rest_normals()).all(|(n, rn)| {
        n.iter().all(|x| x.is_finite()) && (v3(*n).length() - v3(*rn).length()).abs() <= 1e-9
    }));
    let faces = topo.faces();
    let orientation = |v: &[V], n: &[V], fi: usize| {
        let [p0, p1, p2] = faces[fi].v.map(|i| v3(v[i]));
        let g = (p1 - p0).cross(p2 - p0);
        let c = v3(n[3 * fi]) + v3(n[3 * fi + 1]) + v3(n[3 * fi + 2]);
        (g.length(), g.dot(c) > 0.0)
    };
    for (fi, f) in faces.iter().enumerate() {
        if f.part != Part::G {
            continue;
        }
        let (area, sign) = orientation(vertices, normals, fi);
        assert!(area > 0.0, "{what}: cara G {fi} degenerada");
        assert_eq!(
            sign,
            orientation(&straight.0, &straight.1, fi).1,
            "{what}: cara G {fi} invertida"
        );
    }
    let (c, radius) = s.bounds_obj;
    assert!(v3(c).is_finite() && radius.is_finite() && radius > 0.0);
    assert!(
        vertices
            .iter()
            .all(|q| (v3(*q) - v3(c)).length() <= radius * (1.0 + 1e-12))
    );
}

fn straight_reference(p: &SpatialPursuit) -> (Vec<V>, Vec<V>) {
    let rig = p.rig().unwrap();
    let rest = rig.rest_loop();
    let q = v3(rest.center) + v3(rest.tangent) * 3.0;
    let (mut v, mut n) = template_buffers(p);
    ok(
        rig.deform([q.x, q.y, q.z], None, &mut v, &mut n),
        "tubo recto",
    );
    (v, n)
}

/// Nave real con su orientacion real, reubicada en el punto `at_obj` (OBJ) de un caso del preflight.
fn ship_at(world: &World, at_obj: Vec3) -> Mat4 {
    let mut probe = world.ship;
    probe.position = body(world).transform_point(at_obj);
    probe.transform(SHIP_MODEL_YAW).matrix()
}

fn case_points(rig: &SpatialRig) -> [(&'static str, Vec3); 2] {
    let rest = rig.rest_loop();
    let (cp, t) = (v3(rest.center), v3(rest.tangent));
    let side = t.cross(Vec3::Y).normalized();
    let up = side.cross(t).normalized();
    [
        ("straight", cp + t * 3.0),
        (
            "oblique",
            cp + (-t * 0.4 + side * 0.8 + up * 0.2).normalized() * 3.0,
        ),
    ]
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a01_contact02_port_scene_ship_aligns_the_approved_anchor_on_the_published_mesh() {
    let (world, _g, p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let straight = straight_reference(&p);
    let (ship, body) = (ship_matrix(&world), body(&world));
    let (mut v, mut n) = template_buffers(&p);
    let s = ok(
        adapter.solve(adapter.rig(), &ship, &body, None, &mut v, &mut n),
        "contacto de la escena",
    );
    // Mismo caso que CONTACT02: el solver raw en el origen de la nave deja el gap del preflight.
    assert!(
        (s.raw_contact_error_obj - RAW_GAP_CONTACT02).abs() < 1e-9,
        "raw {}",
        s.raw_contact_error_obj
    );
    check_published("scene_ship", &p, &straight, &ship, &body, &v, &n, &s);
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a02_straight_and_oblique_preflight_cases_with_the_real_ship_orientation() {
    let (world, _g, p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let straight = straight_reference(&p);
    let body = body(&world);
    for (what, at) in case_points(p.rig().unwrap()) {
        let ship = ship_at(&world, at);
        let (mut v, mut n) = template_buffers(&p);
        let s = ok(
            adapter.solve(adapter.rig(), &ship, &body, None, &mut v, &mut n),
            what,
        );
        check_published(what, &p, &straight, &ship, &body, &v, &n, &s);
    }
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a03_turned_body_and_non_trivial_ship_orientations_from_the_real_game_with_memory() {
    let (mut world, _g, mut p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let straight = straight_reference(&p);
    let (body0, ship0) = (body(&world), ship_matrix(&world));
    let anchor = v3(adapter.anchor().ship_local());
    // Guinada real (cuerpo girado por el seguimiento horizontal) y despues cabeceo/alabeo reales, con la
    // persecucion publicando su memoria; nada fabricado.
    let inputs = [
        ShipInput {
            yaw: 0.7,
            ..ShipInput::default()
        },
        ShipInput {
            pitch: 0.6,
            roll: -0.8,
            ..ShipInput::default()
        },
    ];
    let mut cases = 0;
    for (stage, input) in inputs.iter().enumerate() {
        for _ in 0..90 {
            advance_frame(&mut world, input, STEP);
            ok(p.step(&world, STEP), "paso real");
        }
        assert_eq!(p.last().unwrap().step.mode, PursuitMode::Pursuing);
        let memory: Option<SpatialMemory> = p.memory();
        assert!(memory.is_some(), "memoria publicada por la persecucion");
        let (ship, body) = (ship_matrix(&world), body(&world));
        assert_ne!(body, body0, "el cuerpo giro");
        let turned = (ship.transform_dir(Vec3::X) - ship0.transform_dir(Vec3::X)).length()
            + (ship.transform_dir(Vec3::Y) - ship0.transform_dir(Vec3::Y)).length();
        assert!(turned > 0.2, "orientacion de nave no trivial: {turned}");
        // El contacto deseado NO es un desplazamiento fijo respecto al origen de la nave.
        let offset = |s: &Mat4, b: &Mat4| desired_obj(s, b, anchor) - to_obj(b, s.column(3));
        assert!((offset(&ship, &body) - offset(&ship0, &body0)).length() > 1e-3);
        let mut ships = vec![(format!("turned{stage}_ship"), ship)];
        for (what, at) in case_points(p.rig().unwrap()) {
            ships.push((format!("turned{stage}_{what}"), ship_at(&world, at)));
        }
        for (what, ship) in ships {
            let (mut v, mut n) = template_buffers(&p);
            let s = ok(
                adapter.solve(adapter.rig(), &ship, &body, memory, &mut v, &mut n),
                &what,
            );
            check_published(&what, &p, &straight, &ship, &body, &v, &n, &s);
            let side = v3(s.pose.memory.side);
            assert!(
                (side.length() - 1.0).abs() < 1e-12,
                "{what}: memoria unitaria"
            );
            assert!(side.dot(v3(p.rig().unwrap().rest_loop().tangent)).abs() < 1e-12);
            cases += 1;
        }
    }
    assert_eq!(cases, 6);
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a04_non_convergent_or_out_of_domain_contact_is_rejected_without_publishing() {
    let (world, _g, p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let rig = p.rig().unwrap();
    let rest = rig.rest_loop();
    let (cp, t) = (v3(rest.center), v3(rest.tangent));
    let side = t.cross(Vec3::Y).normalized();
    let up = side.cross(t).normalized();
    let body = body(&world);
    // Medidos en el diagnostico d02: oblicuo a 0.7 no converge; recto a 0.9 converge DENTRO de la
    // bola minima (fuera del dominio del controlador). Ninguno puede publicarse.
    for (what, at) in [
        (
            "oblique_r0.7",
            cp + (-t * 0.4 + side * 0.8 + up * 0.2).normalized() * 0.7,
        ),
        ("straight_r0.9", cp + t * 0.9),
    ] {
        let ship = ship_at(&world, at);
        let (mut v, mut n) = sentinel(&p);
        let before = (v.clone(), n.clone());
        let r = adapter.solve(adapter.rig(), &ship, &body, None, &mut v, &mut n);
        println!(
            "GT_A rechazo {what}: {:?}",
            r.as_ref().map(|s| s.contact_error_obj)
        );
        assert!(
            r.is_err(),
            "GT-A {what}: contacto sin solucion publicado: {:?}",
            r.map(|s| s.contact_error_obj)
        );
        assert_eq!((v, n), before, "{what}: buffers intactos");
    }
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a05_guard_invalid_inputs_return_err_and_leave_buffers_untouched() {
    let (world, _g, p) = real_game(0.05);
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let rig = p.rig().unwrap();
    let rest = rig.rest_loop();
    let (ship, body) = (ship_matrix(&world), body(&world));
    let far = ship_at(
        &world,
        v3(rest.center) + v3(rest.tangent) * (MAX_REACH_OBJ + 5.0),
    );
    let mut nan_ship = ship;
    nan_ship.m[0][3] = f64::NAN;
    let mut skew_body = body;
    for r in 0..3 {
        skew_body.m[r][0] *= 2.0;
    }
    let bad_memory = Some(SpatialMemory {
        side: [f64::NAN, 0.0, 0.0],
        tip_obj: [0.0; 3],
    });
    let cases: [(&str, Mat4, Mat4, Option<SpatialMemory>); 4] = [
        ("fuera de alcance", far, body, None),
        ("nave no finita", nan_ship, body, None),
        ("cuerpo no uniforme", ship, skew_body, None),
        ("memoria invalida", ship, body, bad_memory),
    ];
    for (what, s, b, m) in cases {
        let (mut v, mut n) = sentinel(&p);
        let before = (v.clone(), n.clone());
        let r = adapter.solve(adapter.rig(), &s, &b, m, &mut v, &mut n);
        assert!(r.is_err(), "{what}: Err esperado");
        println!("GT_A entrada invalida {what}: {}", r.unwrap_err());
        assert_eq!((v, n), before, "{what}: buffers intactos");
    }
    let (mut v, mut n) = sentinel(&p);
    v.pop();
    let before = (v.clone(), n.clone());
    assert!(
        adapter
            .solve(adapter.rig(), &ship, &body, None, &mut v, &mut n)
            .is_err()
    );
    assert_eq!((v, n), before, "topologia distinta: intactos");
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a06_guard_same_inputs_same_bits_and_no_world_or_pursuit_mutation() {
    let (mut world, _g, mut p) = real_game(0.05);
    for _ in 0..30 {
        advance_frame(&mut world, &ShipInput::default(), STEP);
        ok(p.step(&world, STEP), "paso");
    }
    let adapter = ok(
        ContactAdapter::new(
            p.debug().package(),
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| world.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let state = |w: &World, p: &SpatialPursuit| {
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            w.ship.position,
            w.ship.orientation,
            w.camera.view_projection(),
            p.controller(),
            p.memory(),
            p.last()
        )
    };
    let composite = p
        .composite()
        .map(|m| (m.vertices.clone(), m.normals.clone()));
    let before = state(&world, &p);
    let (ship, body) = (ship_matrix(&world), body(&world));
    let mut runs = Vec::new();
    for _ in 0..2 {
        let (mut v, mut n) = template_buffers(&p);
        let s = adapter.solve(adapter.rig(), &ship, &body, p.memory(), &mut v, &mut n);
        assert!(s.is_ok(), "A06 debe publicar contacto valido: {s:?}");
        runs.push((format!("{s:?}"), v, n));
    }
    assert_eq!(runs[0], runs[1], "mismas entradas, mismos bits");
    assert_eq!(state(&world, &p), before, "World/persecucion sin cambios");
    assert_eq!(
        p.composite()
            .map(|m| (m.vertices.clone(), m.normals.clone())),
        composite
    );
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn gt_a07_guard_anchor_is_derived_from_the_approved_grip_frame_not_hardcoded() {
    let (_w, _g, p) = real_game(0.05);
    let package = p.debug().package();
    let adapter = ok(
        ContactAdapter::new(
            package,
            &p.debug()
                .correspondence()
                .arm_vertex()
                .iter()
                .map(|&i| _w.creature().unwrap().model().vertices[i])
                .collect::<Vec<_>>(),
        ),
        "adaptador",
    );
    let anchor = adapter.anchor();
    let manifest = package.capture().manifest();
    assert_eq!(anchor.grip_frame(), manifest.grip_frame());
    assert_eq!(
        anchor.grip_frame(),
        168,
        "grip_frame del manifest (la etiqueta agarre es 169)"
    );
    assert_eq!(adapter.grip().len(), 105);
    assert_eq!(adapter.root().len(), 27);
    let derived = v3(anchor.ship_local());
    let exact = test_anchor(package);
    let (contact, ship168) = approved_contact(package);
    let recon = (ship168.transform_point(derived) - contact).length();
    let parent_recon = (ship168.transform_point(v3(PARENT_ANCHOR)) - contact).length();
    println!(
        "GT_A ancla {:?} exacta_prueba {:?} |dif| {:e} padre {:?} |dif_padre| {:e} reconstruccion_obj {recon:e} reconstruccion_padre_obj {parent_recon:e}",
        derived,
        exact,
        (derived - exact).length(),
        PARENT_ANCHOR,
        (derived - v3(PARENT_ANCHOR)).length()
    );
    // Exacta: reconstruye el centroide aprobado y coincide con la inversa independiente de la prueba.
    assert!(recon < 1e-12, "reconstruccion {recon}");
    assert!((derived - exact).length() < 1e-9 * exact.length().max(1.0));
    // El oraculo del padre usa la inversa de columnas ortogonales: coincide dentro de su propio error
    // de reconstruccion (unidades de nave = mundo; 1e-6 << tolerancia de contacto).
    assert!(
        parent_recon < 1e-9,
        "reconstruccion del padre {parent_recon}"
    );
    assert!((derived - v3(PARENT_ANCHOR)).length() < 1e-6);
    // La etiqueta de cierre (133) es otra pose: el ancla no es intercambiable con ella.
    let data = package.data();
    let ship133 = manifest.ship_obj_matrix(133).unwrap();
    let c133 = centroid(data.base_positions(133).unwrap(), manifest.grip());
    assert!((to_obj(&ship133, c133) - derived).length() > 1.0);
}
