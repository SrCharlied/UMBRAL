//! UMB-311 PERSECUCION VIVA del tentaculo (prototipo opt-in). Pruebas NUEVAS: el controlador puro, la
//! deformacion sobre una topologia SINTETICA publica (copiada del fixture de
//! `tests/tentacle_solar_game.rs`, sin editarlo), la cadena real nave -> World -> persecucion -> cuadro
//! y la linea de comandos. Los assets reales son opt-in (`#[ignore]`, rutas por entorno).
// Bucles por indice sobre componentes xyz: estilo, no logica.
#![allow(clippy::needless_range_loop)]

use umbral::{
    app::{GameOptions, PerceptionRoute, advance_frame, run_game_with_tentacle_live_pursuit},
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

// ---------------- S1: progresion de rapidez por O2 ----------------

#[test]
fn s1_speed_progression_is_ordered_gradual_and_holds_below_five_percent() {
    // Por encima de 15 %: sin persecucion.
    for o2 in [1.0, 0.5, 0.25, 0.150001] {
        assert_eq!(ok(pursuit_speed(o2), "o2"), None, "o2 {o2}");
    }
    let v = |o2: f64| ok(pursuit_speed(o2), "o2").expect("persigue");
    let (v15, v10, v5, v0) = (v(0.15), v(0.10), v(0.05), v(0.0));
    assert!(
        0.0 < v15 && v15 < v10 && v10 < v5,
        "15 < 10 < 5: {v15} {v10} {v5}"
    );
    assert_eq!(v0, v5, "0 % sostiene la de 5 % (sin accion nueva)");
    assert_eq!(v(0.02), v5);
    assert_eq!(
        [v15, v10, v5],
        PURSUIT_SPEEDS.map(|(_, s)| s),
        "valores declarados"
    );
    // Gradual: lineal entre bandas, monotona al bajar el O2.
    assert!((v(0.125) - 0.5 * (v15 + v10)).abs() < 1e-12);
    assert!((v(0.075) - 0.5 * (v10 + v5)).abs() < 1e-12);
    let mut last = 0.0;
    for i in 0..=150 {
        let s = v(0.15 - i as f64 * 0.001);
        assert!(s >= last, "monotona");
        last = s;
    }
    // Entradas invalidas: error explicito.
    for bad in [f64::NAN, f64::INFINITY, -0.01, 1.01] {
        assert!(pursuit_speed(bad).is_err(), "{bad}");
    }
    let _ = Vec3::ZERO;
}

// ---------------- S2: controlador puro (punta en mundo, rapidez limitada, rodea la raiz) ----------------

const DT: f64 = 0.1;
const A: f64 = 10.0;

fn input(ship: Vec3, oxygen01: f64, dt: f64) -> PursuitInput {
    PursuitInput {
        ship,
        anchor: Vec3::ZERO,
        rest_tip: Vec3::new(A, 0.0, 0.0),
        oxygen01,
        dt,
        min_radius: A,
        max_radius: 1000.0,
    }
}

fn rest() -> PursuitController {
    ok(PursuitController::at_rest(Vec3::new(A, 0.0, 0.0)), "rest")
}

#[test]
fn s2_tip_chases_the_moving_ship_with_bounded_speed_and_without_overshoot() {
    let mut c = rest();
    let mut ship = Vec3::new(300.0, 50.0, 0.0);
    let (mut last_speed, mut first_far) = (0.0, None);
    for i in 0..400 {
        if i < 200 {
            ship += Vec3::new(0.0, 0.0, 3.0); // 30 u/s, mas lenta que la punta a 5 %
        }
        let before = c.tip();
        let r = ok(c.step(&input(ship, 0.05, DT)), "step");
        assert_eq!(r.mode, PursuitMode::Pursuing);
        assert_eq!(r.target, ship, "objetivo = posicion ACTUAL de la nave");
        let moved = (c.tip() - before).length();
        assert!((moved - r.moved).abs() < 1e-9);
        assert!(moved <= 48.0 * DT + 1e-9, "rapidez limitada: {moved}");
        assert!(
            c.speed() - last_speed <= PURSUIT_ACCELERATION * DT + 1e-9,
            "aceleracion"
        );
        assert!(c.tip().length() >= A - 1e-9, "fuera de la bola minima");
        last_speed = c.speed();
        if i == 10 {
            first_far = Some((ship - c.tip()).length());
        }
    }
    assert!(first_far.unwrap() > 200.0);
    assert_eq!(
        c.tip(),
        ship,
        "alcanza la nave quieta exactamente, sin sobrepasarla"
    );
    let before = c.tip();
    let r = ok(c.step(&input(ship, 0.05, DT)), "step");
    assert!(
        r.reached && r.moved == 0.0 && c.tip() == before,
        "sin oscilar"
    );
}

#[test]
fn s2_dt_zero_and_invalid_inputs_change_nothing() {
    let mut c = rest();
    let ship = Vec3::new(0.0, 200.0, 0.0);
    for _ in 0..5 {
        ok(c.step(&input(ship, 0.05, DT)), "step");
    }
    let snapshot = c;
    let r = ok(c.step(&input(ship, 0.05, 0.0)), "dt 0");
    assert_eq!(r.moved, 0.0);
    assert_eq!(c, snapshot, "dt 0 no mueve");
    let nan = f64::NAN;
    let big = 2.0e9;
    let mut bad = Vec::new();
    for (name, f) in [
        (
            "ship nan",
            Box::new(|i: &mut PursuitInput| i.ship.x = nan) as Box<dyn Fn(&mut PursuitInput)>,
        ),
        ("ship extremo", Box::new(move |i| i.ship.y = big)),
        ("anchor inf", Box::new(|i| i.anchor.z = f64::INFINITY)),
        ("rest nan", Box::new(|i| i.rest_tip.x = nan)),
        ("o2 nan", Box::new(|i| i.oxygen01 = nan)),
        ("o2 > 1", Box::new(|i| i.oxygen01 = 1.5)),
        ("o2 < 0", Box::new(|i| i.oxygen01 = -0.1)),
        ("dt negativo", Box::new(|i| i.dt = -0.01)),
        ("dt nan", Box::new(|i| i.dt = nan)),
        ("dt enorme", Box::new(|i| i.dt = MAX_PURSUIT_DT * 2.0)),
        ("min 0", Box::new(|i| i.min_radius = 0.0)),
        ("max < min", Box::new(|i| i.max_radius = A * 0.5)),
        ("max inf", Box::new(|i| i.max_radius = f64::INFINITY)),
    ] {
        let mut i = input(ship, 0.05, DT);
        f(&mut i);
        if c.step(&i).is_ok() {
            bad.push(name);
        }
        assert_eq!(c, snapshot, "{name}: el estado no cambia con Err");
    }
    assert!(bad.is_empty(), "aceptados: {bad:?}");
    assert!(PursuitController::at_rest(Vec3::new(nan, 0.0, 0.0)).is_err());
}

#[test]
fn s2_rear_target_goes_around_the_root_ball_deterministically() {
    let run = || {
        let mut c = rest();
        let ship = Vec3::new(-200.0, 0.0, 0.0); // exactamente detras
        let mut path = Vec::new();
        for _ in 0..200 {
            ok(c.step(&input(ship, 0.05, DT)), "step");
            path.push(c.tip());
        }
        (c, path)
    };
    let (c, path) = run();
    let (_, again) = run();
    assert_eq!(path, again, "determinista");
    assert_eq!(c.tip(), Vec3::new(-200.0, 0.0, 0.0), "llega detras");
    assert!(
        path.iter().all(|p| p.length() >= A - 1e-9),
        "nunca atraviesa la bola"
    );
    let lateral = path
        .iter()
        .map(|p| (p.y * p.y + p.z * p.z).sqrt())
        .fold(0.0, f64::max);
    assert!(lateral >= A - 1e-9, "rodea por un lado: {lateral}");
    for w in path.windows(2) {
        assert!((w[1] - w[0]).length() <= 48.0 * DT + 1e-9);
    }
}

#[test]
fn s2_recovery_retracts_without_jumps_and_rest_becomes_idle() {
    let mut c = rest();
    let ship = Vec3::new(0.0, 0.0, 150.0);
    for _ in 0..60 {
        ok(c.step(&input(ship, 0.10, DT)), "step");
    }
    assert!(c.tip().length() > 50.0);
    let mut steps = 0;
    loop {
        let before = c.tip();
        let r = ok(c.step(&input(ship, 0.5, DT)), "recogida");
        assert!((c.tip() - before).length() <= PURSUIT_RETRACT_SPEED * DT + 1e-9);
        steps += 1;
        if r.mode == PursuitMode::Idle {
            break;
        }
        assert_eq!(r.mode, PursuitMode::Retracting);
        assert!(steps < 1000);
    }
    assert!(steps > 10, "sin reset brusco: {steps} pasos");
    assert_eq!(c.tip(), Vec3::new(A, 0.0, 0.0));
    assert_eq!(c.speed(), 0.0);
    // En reposo la punta acompana al cuerpo (punta de reposo actual) y no persigue.
    let mut i = input(ship, 0.30, DT);
    i.anchor = Vec3::new(5.0, 0.0, 0.0);
    i.rest_tip = Vec3::new(15.0, 1.0, 0.0);
    let r = ok(c.step(&i), "idle");
    assert_eq!(r.mode, PursuitMode::Idle);
    assert_eq!(c.tip(), i.rest_tip);
}

#[test]
fn s2_bands_change_speed_gradually_and_far_targets_are_clamped() {
    let mut c = rest();
    let ship = Vec3::new(0.0, 900.0, 0.0);
    let mut last = 0.0;
    for (o2, n) in [(0.15, 40), (0.10, 40), (0.05, 40), (0.0, 20)] {
        for _ in 0..n {
            ok(c.step(&input(ship, o2, DT)), "step");
            assert!((c.speed() - last).abs() <= PURSUIT_ACCELERATION * DT + 1e-9);
            last = c.speed();
        }
        let cap = ok(pursuit_speed(o2), "v").unwrap();
        assert!(
            (c.speed() - cap).abs() < 1e-9,
            "o2 {o2}: {} vs {cap}",
            c.speed()
        );
    }
    let mut far = rest();
    let r = ok(
        far.step(&input(Vec3::new(5000.0, 0.0, 0.0), 0.05, DT)),
        "far",
    );
    assert!(r.target_clamped);
    assert_eq!(
        r.target,
        Vec3::new(1000.0, 0.0, 0.0),
        "objetivo recortado al alcance"
    );
}

// ---------------- S6 (hallazgo del bucle real): el paso usa la rapidez completa ----------------

#[test]
fn s6_full_speed_is_used_while_turning_and_extending() {
    // Hallazgo con los assets reales: la cota max(r, r_obj) * theta + |dr| subestimaba el avance
    // cuando el radio crece mucho (de la bola a la nave). El recorrido del paso debe ser la
    // longitud del camino polar real: a rapidez maxima se recorre ~ v * dt (sin pasarse).
    let mut c = rest();
    let ship = Vec3::new(0.0, 0.0, 300.0);
    let mut checked = 0;
    for i in 0..120 {
        let before = c.tip();
        let r = ok(c.step(&input(ship, 0.05, DT)), "step");
        let moved = (c.tip() - before).length();
        assert!(moved <= c.speed() * DT + 1e-9, "nunca mas que la rapidez");
        if i >= 15 && !r.reached && (ship - c.tip()).length() > 30.0 {
            assert!((c.speed() - 48.0).abs() < 1e-9);
            assert!(
                moved >= 0.97 * 48.0 * DT,
                "paso {i}: {moved} de {}",
                48.0 * DT
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked}");
}

// ---------------- S7 (hallazgo de las secuencias reales): embestida recta si no hay obstaculo ----------------

#[test]
fn s7_unobstructed_targets_are_approached_in_a_straight_line() {
    // Hallazgo: el paso polar giraba y extendia a la vez (espiral hasta ~30 % mas larga y con barrido
    // lateral: la distancia punta-nave crecia al empezar). Si el segmento punta -> objetivo no entra en
    // la bola minima, la punta va RECTA: la distancia al objetivo baja exactamente lo recorrido.
    let mut c = rest();
    let ship = Vec3::new(60.0, 300.0, -40.0);
    let mut steps = 0;
    loop {
        let before = (ship - c.tip()).length();
        let r = ok(c.step(&input(ship, 0.05, DT)), "step");
        let after = (ship - c.tip()).length();
        assert!(
            (before - after - r.moved).abs() < 1e-9,
            "paso {steps}: se acerca {} y recorre {}",
            before - after,
            r.moved
        );
        steps += 1;
        if r.reached {
            break;
        }
        assert!(steps < 400);
    }
    assert_eq!(c.tip(), ship);
    // Con el objetivo detras la punta sigue rodeando la bola (nunca entra).
    let mut c = rest();
    let behind = Vec3::new(-120.0, 5.0, 0.0);
    for _ in 0..200 {
        ok(c.step(&input(behind, 0.05, DT)), "step");
        assert!(c.tip().length() >= A - 1e-9);
    }
    assert_eq!(c.tip(), behind);
}

// ---------------- S8 (regresion de S7 en S4): reposo DENTRO de la bola de exactitud ----------------

#[test]
fn s8_rest_inside_the_ball_leaves_and_returns_along_the_rest_axis() {
    // Cerca del lazo, con angulo, el distal rigido no puede poner su punta axial en el objetivo (hasta
    // ~18 u con el asset real). La bola del controlador es entonces la de EXACTITUD (mayor que la punta de
    // reposo): se sale y se vuelve por el eje de reposo, sin saltos, y fuera se embiste o se rodea.
    let rest_tip = Vec3::new(A, 0.0, 0.0);
    let ball = 1.5 * A;
    let on_axis = |p: Vec3| p.y.abs() < 1e-9 && p.z.abs() < 1e-9 && p.x > 0.0;
    let mut c = rest();
    let ship = Vec3::new(20.0, 250.0, 30.0);
    let mut seen_outside = false;
    for step in 0..400 {
        let before = c.tip();
        let mut i = input(ship, 0.05, DT);
        i.min_radius = ball;
        let r = ok(c.step(&i), "persigue");
        let p = c.tip();
        assert!((p - before).length() <= 48.0 * DT + 1e-9, "sin saltos");
        assert!(
            p.length() >= ball - 1e-9 || on_axis(p),
            "paso {step}: dentro de la bola solo por el eje: {p:?}"
        );
        seen_outside |= p.length() >= ball - 1e-9;
        if r.reached {
            break;
        }
    }
    assert!(seen_outside);
    assert_eq!(c.tip(), ship, "alcanza la nave");
    // Recuperacion: vuelve por la puerta del eje y termina EXACTO en el reposo.
    let mut steps = 0;
    loop {
        let before = c.tip();
        let mut i = input(ship, 0.5, DT);
        i.min_radius = ball;
        let r = ok(c.step(&i), "recoge");
        let p = c.tip();
        assert!((p - before).length() <= PURSUIT_RETRACT_SPEED * DT + 1e-9);
        assert!(
            p.length() >= ball - 1e-9 || on_axis(p),
            "recogida {steps}: {p:?}"
        );
        steps += 1;
        if r.mode == PursuitMode::Idle {
            break;
        }
        assert!(steps < 2000);
    }
    assert_eq!(c.tip(), rest_tip);
}

// ---------------- S9 (hallazgo real: OBLIQUA con el cuerpo girando) ----------------

#[test]
fn s9_engagement_starts_from_the_current_rest_tip() {
    // Hallazgo real: en reposo el cuerpo gira (seguimiento 02); al empezar a perseguir se partia de la
    // punta de reposo del cuadro ANTERIOR, que respecto al anclaje nuevo podia quedar dentro de la bola
    // de la malla (la deformacion la rechazaba en cada cuadro: persecucion congelada).
    let mut c = rest();
    let mut i = input(Vec3::new(0.0, 300.0, 0.0), 0.5, DT);
    i.rest_tip = Vec3::new(0.0, A, 0.0);
    ok(c.step(&i), "reposo");
    assert_eq!(c.tip(), i.rest_tip);
    // El cuerpo giro otra vez antes del primer paso de persecucion.
    let mut j = input(Vec3::new(0.0, 300.0, 0.0), 0.05, DT);
    j.rest_tip = Vec3::new(-A * 0.6, A * 0.8, 0.0);
    j.min_radius = 1.5 * A;
    ok(c.step(&j), "persigue");
    let p = c.tip();
    let rest_dir = j.rest_tip * (1.0 / A);
    assert!(
        (p - rest_dir * p.length()).length() < 1e-9,
        "sale por el eje de reposo ACTUAL: {p:?}"
    );
    assert!(p.length() >= A, "nunca dentro de la punta de reposo");
}

#[test]
fn s9_a_turning_body_never_freezes_the_pursuit() {
    let (mut w, mut g) = game(0.05);
    // OBLIQUA con la tecla real: el seguimiento 02 hace girar el cuerpo mientras el brazo arranca.
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
    let mut p = pursuit(&w);
    let start = w.creature_render_transform().unwrap().matrix();
    for i in 0..240 {
        advance_frame(&mut w, &ShipInput::default(), STEP);
        let s = p.step(&w, STEP);
        assert!(s.is_ok(), "cuadro {i}: {:?}", s.err());
    }
    assert_ne!(
        start,
        w.creature_render_transform().unwrap().matrix(),
        "el cuerpo giro"
    );
    let s = p.last().unwrap();
    assert!(s.tip_ship_distance < 0.5 * (s.ship_world - s.tip_world).length() + 1e9);
    assert!(s.step.mode == PursuitMode::Pursuing && s.shaft.is_some());
}

// ---------------- S10 (hallazgo real: progresion en OBLIQUA estancada ~13 s) ----------------

#[test]
fn s10_a_drifting_anchor_does_not_stall_the_exit() {
    // Hallazgo: con el cuerpo girando, el anclaje deriva y deja la punta un pelo DENTRO de la bola; la
    // salida radial llegaba al borde con un camino diminuto y la rapidez caia a camino / dt (~0): la
    // persecucion quedaba quieta. A rapidez maxima cada paso sin llegar recorre ~ v * dt y la punta
    // alcanza la nave.
    let mut c = rest();
    let ship = Vec3::new(0.0, 0.0, 400.0);
    let ball = 1.5 * A;
    let mut anchor = Vec3::ZERO;
    let (mut checked, mut reached) = (0, false);
    for k in 0..400 {
        anchor += Vec3::new(1e-4, 0.0, 0.0); // el cuerpo arrastra el anclaje hacia la punta
        let mut i = input(ship, 0.05, DT);
        i.anchor = anchor;
        i.rest_tip = anchor + Vec3::new(A, 0.0, 0.0);
        i.min_radius = ball;
        let r = ok(c.step(&i), "paso");
        assert!(r.moved <= 48.0 * DT + 1e-9);
        if r.reached {
            reached = true;
            break;
        }
        if k >= 40 && (ship - c.tip()).length() > 30.0 {
            assert!(
                r.moved >= 0.9 * 48.0 * DT,
                "paso {k}: recorre {} de {}: la punta no debe estancarse",
                r.moved,
                48.0 * DT
            );
            checked += 1;
        }
    }
    assert!(
        reached && checked > 40,
        "alcanza la nave ({reached}) tras {checked} pasos medidos"
    );
}

// ---------------- S3: deformacion (topologia sintetica aprobada como dato) ----------------

type V = [f64; 3];

fn v3(a: V) -> Vec3 {
    Vec3::from_array(a)
}

fn rig() -> (SolarCaptureDebug, ShaftRig) {
    let d = debug();
    let r = ok(ShaftRig::new(d.package().data(), &tube(1).vertices), "rig");
    (d, r)
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
    // R x = sum_k b_k (a_k . x)
    let col = |x: Vec3| b[0] * a[0].dot(x) + b[1] * a[1].dot(x) + b[2] * a[2].dot(x);
    [col(Vec3::X), col(Vec3::Y), col(Vec3::Z)]
}

fn apply(r: &[Vec3; 3], x: Vec3) -> Vec3 {
    r[0] * x.x + r[1] * x.y + r[2] * x.z
}

#[test]
fn s3_rest_tip_reproduces_the_exact_rest_arm() {
    let (d, rig) = rig();
    let data = d.package().data();
    let topo = data.topology();
    let exact = exact_rest(&d);
    let meshy = creature_model();
    for (i, &m) in d.correspondence().arm_vertex().iter().enumerate() {
        assert_eq!(
            meshy.vertices[m],
            tube(1).vertices[i],
            "brazo = Meshy (sintetico)"
        );
    }
    let (mut v, mut n) = buffers(&d);
    let pose = ok(rig.deform(rig.rest_tip(), None, &mut v, &mut n), "reposo");
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
    let mut reference = copy_model(data.template());
    ok(
        data.evaluate(1, [0.0; 3], &mut reference).map(|_| ()),
        "ley o = 0",
    );
    assert_eq!(n, reference.normals, "normales de reposo de la ley");
    assert_eq!(pose.shaft_length, 0.0);
    assert_eq!(pose.bend, 0.0);
    let rest = rig.rest_loop();
    assert!(
        (v3(rest.tangent) - Vec3::X).length() < 1e-9,
        "tangente del lazo (+X sintetico)"
    );
    assert!(rig.tip_offset() >= 2.0 * rig.bend_radius() && rig.bend_radius() > rest.contour_radius);
    assert!((rig.bend_radius() - BEND_RADIUS_CONTOURS * rest.contour_radius).abs() < 1e-15);
}

#[test]
fn s3_tip_is_reached_in_every_direction_with_rigid_rings_and_distal() {
    let (d, rig) = rig();
    let topo = d.package().data().topology();
    let rest = rig.rest_loop();
    let (cp, t, a) = (v3(rest.center), v3(rest.tangent), rig.tip_offset());
    let (mut rv, mut rn) = buffers(&d);
    ok(rig.deform(rig.rest_tip(), None, &mut rv, &mut rn), "reposo");
    let dirs = [
        Vec3::X,  // radial, hacia afuera
        -Vec3::X, // detras (opuesto)
        Vec3::Y,  // vertical (arriba del mundo)
        -Vec3::Y,
        Vec3::Z, // lateral
        -Vec3::Z,
        Vec3::new(1.0, 1.0, 1.0).normalized(),
        Vec3::new(-1.0, 0.3, -0.5).normalized(),
    ];
    let faces = topo.faces();
    // Orientacion de referencia por cara G: el tubo RECTO de la misma plantilla (el fixture sintetico
    // enrolla G hacia dentro respecto a sus normales; lo que se exige es no invertir respecto al recto).
    let straight = cp + t * 1.5;
    let (mut sv, mut sn) = buffers(&d);
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
    for dir in dirs {
        for dist in [0.5, 1.5, 4.0] {
            let tip = cp + dir * dist;
            let (mut v, mut n) = buffers(&d);
            let pose = ok(
                rig.deform([tip.x, tip.y, tip.z], None, &mut v, &mut n),
                "deform",
            );
            let what = format!("dir {dir:?} dist {dist}");
            assert!(
                (v3(pose.axial_tip) - tip).length() < 1e-9,
                "{what}: punta alcanzada"
            );
            assert!(pose.shaft_length > 0.0 && pose.line_length >= 0.0, "{what}");
            // Proximal y raiz inmoviles (bit a bit).
            for (i, tv) in topo.vertices().iter().enumerate() {
                if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
                    assert_eq!(v[i], rv[i], "{what}: proximal {i}");
                }
            }
            // Distal rigido: R ortonormal; cada vertice D = centro distal + R (reposo - cp).
            let r = distal_rotation(&d, &rv, &v);
            let cd = v3(pose.distal_center);
            for (i, tv) in topo.vertices().iter().enumerate() {
                if matches!(tv, GrowthVertex::Retained { side: Side::D, .. }) {
                    let expect = cd + apply(&r, v3(rv[i]) - cp);
                    assert!(
                        (v3(v[i]) - expect).length() < 1e-9,
                        "{what}: distal {i} rigido"
                    );
                }
            }
            let end_dir = apply(&r, t);
            assert!(
                (cd + end_dir * a - tip).length() < 1e-9,
                "{what}: la tangente final apunta a la punta"
            );
            // Anillos rigidos (copias del lazo) y caras G sanas: sin degenerados ni winding invertido.
            let lp = topo.loop_p();
            for ring in &topo.rings()[1..topo.rings().len() - 1] {
                for k in 0..ring.len() {
                    let k1 = (k + 1) % ring.len();
                    let l = (v3(v[ring[k]]) - v3(v[ring[k1]])).length();
                    let l0 = (v3(rv[lp[k]]) - v3(rv[lp[k1]])).length();
                    assert!((l - l0).abs() < 1e-9, "{what}: anillo rigido");
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
        }
    }
}

#[test]
fn s3_motion_is_continuous_and_the_bend_side_has_hysteresis() {
    let (d, rig) = rig();
    let rest = rig.rest_loop();
    let cp = v3(rest.center);
    let run = || {
        let (mut prev_v, mut prev_n) = buffers(&d);
        let mut side = None;
        let mut log = Vec::new();
        for deg in 60..=300 {
            let a = (deg as f64).to_radians();
            let tip = cp + (Vec3::X * a.cos() + Vec3::Y * a.sin()) * 1.0;
            let (mut v, mut n) = buffers(&d);
            let pose = ok(
                rig.deform([tip.x, tip.y, tip.z], side, &mut v, &mut n),
                "deform",
            );
            let jump = if deg == 60 {
                0.0
            } else {
                v.iter()
                    .zip(&prev_v)
                    .map(|(a, b)| (v3(*a) - v3(*b)).length())
                    .fold(0.0, f64::max)
            };
            log.push((deg, pose.side_switched, jump, pose.bend));
            side = Some(pose.bend_side);
            (prev_v, prev_n) = (v, n);
        }
        let _ = prev_n;
        log
    };
    let log = run();
    assert_eq!(log, run(), "determinista");
    let switches: Vec<_> = log.iter().filter(|e| e.1).collect();
    assert_eq!(switches.len(), 1, "un solo cambio de lado: {switches:?}");
    let at = switches[0].0;
    assert!(
        (195..=215).contains(&at),
        "el lado se conserva hasta ~180 + 30 grados: {at}"
    );
    let before = log.iter().find(|e| e.0 == at - 1).unwrap();
    assert!(
        before.3 > std::f64::consts::PI + 0.4 * SIDE_HYSTERESIS,
        "antes del cambio gira por el MISMO lado mas alla de 180: {before:?}"
    );
    for &(deg, switched, jump, bend) in &log {
        if !switched && deg > 60 {
            assert!(jump < 0.1, "continuo a {deg}: {jump}");
        }
        assert!(
            bend <= std::f64::consts::PI + SIDE_HYSTERESIS + 0.02,
            "{deg}: {bend}"
        );
    }
}

#[test]
fn s3_invalid_tips_change_nothing() {
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
        ShaftRig::new(d.package().data(), &bad_arm).is_err(),
        "reposo no finito"
    );
    assert!(
        ShaftRig::new(d.package().data(), &bad_arm[..4]).is_err(),
        "brazo corto"
    );
}

// ---------------- S4: cadena real nave -> World -> persecucion -> cuadro (sintetico) ----------------

const STEP: f64 = 1.0 / 30.0;

fn pursuit(world: &World) -> LivePursuit {
    let mut p = LivePursuit::new(debug());
    ok(p.bind(world), "bind");
    p
}

/// Un cuadro del juego con la nave volada por el controlador REAL (`advance_frame`) y la persecucion.
fn fly(world: &mut World, p: &mut LivePursuit, input: ShipInput) -> LiveFrameStatus {
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

#[test]
fn s4_idle_and_above_fifteen_percent_frames_are_exactly_the_original_frames() {
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
            assert!(p.body_override().is_none(), "o2 {o2}: sin capa propia");
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
fn s4_root_and_proximal_coincide_with_the_body_every_frame_while_the_body_turns() {
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

fn d_correspondence() -> Vec<usize> {
    debug().correspondence().arm_vertex().to_vec()
}

#[test]
fn s4_tip_follows_the_current_ship_position_from_the_real_controller() {
    let (mut w, g) = game(0.05);
    let mut p = pursuit(&w);
    let mut last_ship = w.ship.position;
    let mut max_far: f64 = 0.0;
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
    let (with, _) = frame(&w, &g, false, p.body_override().as_ref());
    let (without, _) = frame(&w, &g, false, None);
    assert_ne!(
        with.pixels, without.pixels,
        "el brazo extendido se dibuja en la capa Body"
    );
}

#[test]
fn s4_errors_are_atomic_and_keep_the_last_frame() {
    let (mut w, g) = game(0.05);
    let mut p = pursuit(&w);
    for _ in 0..40 {
        fly(&mut w, &mut p, thrust(1.0, 0.4));
    }
    let snapshot = |p: &LivePursuit| {
        (
            format!("{:?} {:?}", p.last(), p.controller()),
            p.composite()
                .map(|m| (m.vertices.clone(), m.normals.clone())),
        )
    };
    let before = snapshot(&p);
    let (fb_before, st_before) = frame(&w, &g, true, p.body_override().as_ref());
    for (name, dt) in [
        ("dt nan", f64::NAN),
        ("dt negativo", -STEP),
        ("dt enorme", 5.0),
    ] {
        assert!(p.step(&w, dt).is_err(), "{name}");
        assert_eq!(snapshot(&p), before, "{name}");
    }
    let o2 = w.debug_oxygen01;
    w.debug_oxygen01 = f64::NAN;
    assert!(p.step(&w, STEP).is_err(), "O2 NaN");
    w.debug_oxygen01 = o2;
    let ship = w.ship.position;
    w.ship.position = Vec3::new(2.0e9, 0.0, 0.0);
    assert!(p.step(&w, STEP).is_err(), "nave extrema");
    w.ship.position = ship;
    assert_eq!(snapshot(&p), before, "nada cambio");
    let (fb_after, st_after) = frame(&w, &g, true, p.body_override().as_ref());
    assert_eq!(
        pixels_and_stats(&fb_before, &st_before),
        pixels_and_stats(&fb_after, &st_after)
    );
    let mut unbound = LivePursuit::new(debug());
    assert!(unbound.step(&w, STEP).is_err(), "sin bind");
    assert!(unbound.body_override().is_none());
}

#[test]
fn s4_bind_reuses_the_identity_guards() {
    let mut model = creature_model();
    model.vertices[DOME_VERTICES + 3][1] += 1e-3;
    let (w, _g) = game_with(model, 0.05);
    let mut p = LivePursuit::new(debug());
    assert!(p.bind(&w).is_err(), "otra criatura");
    assert!(p.rig().is_none() && p.composite().is_none() && p.body_prefix().is_none());
    let (w, _g) = game(0.05);
    let report = ok(p.bind(&w), "bind");
    assert_eq!(report.capture.arm_faces_removed, debug_faces());
    assert!(report.exact_rest_vertices > 0);
    assert!(report.tip_offset_obj >= 2.0 * report.bend_radius_obj);
}

fn debug_faces() -> usize {
    debug().correspondence().arm_face().len()
}

// ---------------- S5: linea de comandos y entrada de la app (antes de cualquier E/S) ----------------

fn temp_inputs(tag: &str) -> (std::path::PathBuf, LivePursuitRequest) {
    let dir = std::env::temp_dir().join(format!(
        "umbral_live_pursuit_{tag}_{}_{}",
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
fn s5_live_flags_are_validated_before_any_io() {
    let (dir, r) = temp_inputs("flags");
    // Sin el flag: no decide nada (las reglas de siempre siguen en validate_tentacle_capture_flags).
    assert_eq!(
        ok(validate_live_pursuit_flags(false, &flags(&r)), "sin flag"),
        None
    );
    let good = ok(validate_live_pursuit_flags(true, &flags(&r)), "valido").expect("pedido");
    assert_eq!(good, r);
    assert_eq!(
        good.capture_request().start_time,
        None,
        "sin reloj de la composicion"
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
        let e = validate_live_pursuit_flags(true, &f).expect_err(name);
        assert!(e.contains(needle), "{name}: {e}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn s5_the_app_entry_rejects_bad_routes_before_any_io() {
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
        let e = run_game_with_tentacle_live_pursuit(
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
fn s5_the_binary_rejects_live_combinations_before_any_io() {
    let out = std::env::temp_dir().join(format!("umbral_live_guard_{}.png", std::process::id()));
    for (args, needle) in [
        (
            vec!["--tentacle-live-pursuit"],
            "requiere --creature-solar-reveal",
        ),
        (
            vec!["--tentacle-live-pursuit", "--tentacle-capture-debug"],
            "incompatibles",
        ),
        (
            vec!["--tentacle-live-pursuit", "--tentacle-capture-time", "1"],
            "--tentacle-capture-time",
        ),
        (vec!["--tentacle-live-pursuit", "--legacy"], "--legacy"),
        // Sin el flag nuevo, las reglas viejas no cambian.
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
    assert!(help.contains("--tentacle-live-pursuit (requiere --creature-solar-reveal)"));
    assert!(help.contains("--tentacle-capture-debug (requiere --creature-solar-reveal)"));
}

// ============================ assets REALES (opt-in, #[ignore]) ============================
// Entorno: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE,
// UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE (claves fixture_environment de BASELINE.json).

fn real_env(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k}")))
}

fn real_game(oxygen: f64) -> (World, SolarGame, LivePursuit) {
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
        LivePursuit::load(&LivePursuitRequest {
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
fn real_live_rest_is_the_exact_meshy_arm_and_deforms_in_every_direction() {
    let (world, _g, p) = real_game(0.05);
    let rig = p.rig().unwrap();
    let data = p.debug().package().data();
    let topo = data.topology();
    let meshy = world.creature().unwrap().model();
    let corr = p.debug().correspondence().arm_vertex();
    // Reposo EXACTO del Meshy (no el export de 6 decimales de la captura) en P y D originales.
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
        "vertices originales exactos: {exact}; codo {} OBJ, punta axial {} OBJ, contorno {}",
        rig.bend_radius(),
        rig.tip_offset(),
        rig.rest_loop().contour_radius
    );
    let rest = rig.rest_loop();
    let (cp, t) = (
        Vec3::from_array(rest.center),
        Vec3::from_array(rest.tangent),
    );
    let template = data.template();
    let buffers = || (template.vertices.clone(), template.normals.clone());
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
    let mut worst_flip = 0;
    for (name, dir) in dirs {
        for dist in [0.5, 2.0, 6.0, 11.0] {
            let tip = cp + dir * dist;
            let (mut v, mut n) = buffers();
            let pose = ok(
                rig.deform([tip.x, tip.y, tip.z], None, &mut v, &mut n),
                name,
            );
            assert!(
                (Vec3::from_array(pose.axial_tip) - tip).length() < 1e-9,
                "{name} {dist}: punta"
            );
            for (i, tv) in topo.vertices().iter().enumerate() {
                if matches!(tv, GrowthVertex::Retained { side: Side::P, .. }) {
                    assert_eq!(v[i], rig.rest_vertices()[i], "{name} {dist}: proximal {i}");
                }
            }
            let mut flips = 0;
            for (fi, f) in faces.iter().enumerate() {
                if f.part != Part::G {
                    continue;
                }
                let (area, sign) = orientation(&v, &n, fi);
                assert!(area > 0.0, "{name} {dist}: cara G {fi} degenerada");
                if sign != orientation(&sv, &sn, fi).1 {
                    flips += 1;
                }
            }
            worst_flip = worst_flip.max(flips);
            assert_eq!(
                flips, 0,
                "{name} {dist}: caras G invertidas respecto al tubo recto"
            );
            println!(
                "{name} {dist}: codo {:.2} grados, tramo {:.4} OBJ",
                pose.bend.to_degrees(),
                pose.shaft_length
            );
        }
    }
    assert_eq!(worst_flip, 0);
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn real_live_pursuit_follows_the_ship_and_keeps_the_root_exact() {
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
fn real_live_oblique_turning_body_never_freezes_the_pursuit() {
    // Regresion del hallazgo real (secuencia e09): OBLIQUA + seguimiento 02 girando el cuerpo.
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


// Parent review regressions: inherited bytes above remain an exact immutable prefix.
#[path = "support/live_pursuit_review_regressions.rs"]
mod parent_review_regressions;
