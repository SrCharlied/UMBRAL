//! GT1-CARRY: captura efectiva y arrastre tras el Gripped REAL de C aprobada, sobre la ruta del lanzador
//! (World, superficie lucida, ojo del vacio, Sol con ojo, percepcion) y el cuadro COMPARTIDO
//! `advance_capture_frame`. Assets privados opt-in (`#[ignore]`, claves de BASELINE.json). Oraculos
//! independientes: contacto en la MALLA final contra el ancla con la matriz de render REAL, marco de
//! marcadores distales propio, replica del filtro distal de la topologia, inversa afin propia.
use umbral::{
    assets::{obj::load_model, ship_model_path},
    math::{Mat4, Vec3},
    renderer::framebuffer::Framebuffer,
    scene::{
        ship::ShipInput,
        world::{SHIP_MODEL_YAW, SPACE_COLOR, World},
    },
    visual::{
        lucid_game::set_lucid_surface,
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
        static_creature::StaticCreature,
        tentacle_grasp_carry::*,
        tentacle_grasp_closure::ClosureClip,
        tentacle_grasp_preview::*,
        tentacle_grasp_transition::CONTACT_TOLERANCE_OBJ,
        tentacle_growth_cpu::{GrowthVertex, Side},
        tentacle_solar_game::TentacleCaptureRequest,
        tentacle_spatial_pursuit::SpatialPursuit,
        void_game::set_void_eye,
        watching_stars_game::render_game_frame_with,
    },
};

type V = [f64; 3];
const DT: f64 = 1.0 / 60.0;
const PRIVATE: &str = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE";
/// Relacion rigida y forma distal (round-trip de Euler ~1e-15; margen declarado en el contrato).
const RIGID_TOL: f64 = 1e-9;

fn ok<T>(r: Result<T, String>, what: &str) -> T {
    assert!(r.is_ok(), "{what}: {:?}", r.as_ref().err());
    r.unwrap()
}
fn env(k: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var(k).unwrap_or_else(|_| panic!("falta {k} ({PRIVATE})")))
}
fn request() -> TentacleCaptureRequest {
    TentacleCaptureRequest {
        capture_dir: env("UMBRAL_CAPTURE_PACKAGE_DIR"),
        growth_package: env("UMBRAL_GROWTH_PACKAGE"),
        correspondence: env("UMBRAL_ARM_CORRESPONDENCE"),
        start_time: None,
    }
}
fn bits(a: &Mat4, b: &Mat4) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}
fn ship_m(w: &World) -> Mat4 {
    w.ship.transform(SHIP_MODEL_YAW).matrix()
}
fn body_m(w: &World) -> Mat4 {
    w.creature_render_transform().unwrap().matrix()
}
fn vbits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

struct Game {
    world: World,
    game: SolarGame,
    oxygen: OxygenTransition,
}

fn game() -> Game {
    let mut world = World::new(load_model(&ship_model_path()).unwrap(), 1280.0 / 720.0).unwrap();
    world.debug_oxygen01 = 1.0;
    ok(set_lucid_surface(&mut world, true).map(|_| ()), "lucida");
    ok(set_void_eye(&mut world, true).map(|_| ()), "vacio");
    let placement = ok(solar_creature_placement(&world), "placement");
    let creature = ok(
        StaticCreature::load(
            &env("UMBRAL_CAPTURE_CREATURE_OBJ"),
            &env("UMBRAL_CREATURE_TEXTURE"),
            placement,
        ),
        "criatura real",
    );
    let game = ok(SolarGame::start(&mut world, creature), "solar");
    Game {
        world,
        game,
        oxygen: OxygenTransition::new(1.0).unwrap(),
    }
}

fn bound(
    world: &World,
    debug: umbral::visual::tentacle_solar_game::SolarCaptureDebug,
) -> SpatialPursuit {
    let mut p = SpatialPursuit::new(debug);
    ok(p.bind(world).map(|_| ()), "bind real");
    ok(p.step(world, 0.0).map(|_| ()), "paso 0");
    p
}

/// Escena + paquete + clip + preview C + arrastre (habilitado o ruta legacy).
macro_rules! fixture {
    ($g:ident, $carry:ident, $enabled:expr) => {
        let mut $g = game();
        let PreviewInputs { debug, package } = ok(load_preview_inputs(&request()), "entradas");
        let pursuit = bound(&$g.world, debug);
        let rest = ok(arm_rest(&$g.world, &pursuit), "reposo");
        let clip = ok(ClosureClip::new(&package, &rest), "clip");
        let preview = ok(GraspPreview::new(&clip, pursuit), "preview C");
        #[allow(unused_mut)]
        let mut $carry = ok(CaptureCarry::new(preview, $enabled), "arrastre");
    };
}

fn controls(
    ship: ShipInput,
    oxygen: OxygenKeys,
    solar: SolarKeys,
    keys: PreviewKeys,
) -> FrameControls {
    FrameControls {
        ship,
        oxygen,
        solar,
        preview: keys,
    }
}
fn neutral() -> FrameControls {
    FrameControls::default()
}
fn with_keys(request: bool, reset: bool) -> FrameControls {
    FrameControls {
        preview: PreviewKeys { request, reset },
        ..FrameControls::default()
    }
}
fn preset(o2: f64) -> FrameControls {
    FrameControls {
        oxygen: OxygenKeys {
            preset: Some(o2),
            ..OxygenKeys::default()
        },
        ..FrameControls::default()
    }
}
fn all_flight() -> ShipInput {
    ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        pitch: -1.0,
        roll: 1.0,
        boost: true,
    }
}

/// Un cuadro del helper COMPARTIDO; el cuadro de la preview debe ser Ok.
fn step(
    g: &mut Game,
    carry: &mut CaptureCarry,
    c: FrameControls,
    dt: f64,
) -> (FrameOutcome, CarryStatus) {
    let out = ok(
        advance_capture_frame(&mut g.world, &mut g.game, &mut g.oxygen, carry, c, dt),
        "cuadro compartido",
    );
    let st = ok(out.status.clone(), "cuadro del arrastre");
    (out, st)
}

fn has(st: &CarryStatus, e: &CarryEvent) -> bool {
    st.events.contains(e)
}
fn gripped(st: &CarryStatus) -> bool {
    has(st, &CarryEvent::Preview(PreviewEvent::Gripped))
}

/// O2 al 5 % en el cuadro 0, T en el 30 (antes de la llegada) y cuadros neutros hasta `Locked`.
/// Devuelve (cuadros, matriz de nave del cuadro ANTERIOR al bloqueo, estado del bloqueo).
fn to_lock(g: &mut Game, carry: &mut CaptureCarry) -> (u64, Mat4, CarryStatus) {
    for i in 0..9000u64 {
        let c = match i {
            0 => preset(0.05),
            30 => with_keys(true, false),
            _ => neutral(),
        };
        let before = ship_m(&g.world);
        let (_, st) = step(g, carry, c, DT);
        if has(&st, &CarryEvent::Locked) {
            assert!(
                gripped(&st),
                "el bloqueo debe nacer en el cuadro del Gripped real"
            );
            return (i, before, st);
        }
        assert!(i < 8999, "sin bloqueo tras 9000 cuadros: {:?}", st.phase);
    }
    unreachable!()
}

/// Pide otra captura desde la pose ACTUAL (tras soltar con X) y espera el siguiente `Locked`. Con
/// `frontal`, R antes (vista FRONTAL, permitida sin bloqueo): tras un arrastre la nave queda girada y
/// camara/cuerpo pueden entrar en el ciclo-2 documentado en C, que impide la quietud que exige C.
fn relock_with(g: &mut Game, carry: &mut CaptureCarry, frontal: bool) -> CarryStatus {
    if frontal {
        let r = SolarKeys {
            reset: true,
            ..SolarKeys::default()
        };
        let (out, _) = step(
            g,
            carry,
            controls(
                ShipInput::default(),
                OxygenKeys::default(),
                r,
                PreviewKeys::default(),
            ),
            DT,
        );
        assert!(
            out.solar_applied.reset && !out.solar_blocked,
            "R sin bloqueo debe aplicarse"
        );
    }
    let (_, st) = step(g, carry, with_keys(true, false), DT);
    assert_eq!(st.phase, CarryPhase::Free(PreviewPhase::Pursuit));
    for _ in 0..9000 {
        let (_, st) = step(g, carry, neutral(), DT);
        if has(&st, &CarryEvent::Locked) {
            return st;
        }
        let rejected = st
            .events
            .iter()
            .find(|e| matches!(e, CarryEvent::CarryRejected(_)));
        assert!(rejected.is_none(), "bloqueo rechazado: {rejected:?}");
    }
    panic!("sin segundo bloqueo: {:?}", carry.status());
}
fn relock(g: &mut Game, carry: &mut CaptureCarry) -> CarryStatus {
    relock_with(g, carry, false)
}

fn centroid(points: &[V], idx: &[usize]) -> Vec3 {
    let mut s = Vec3::ZERO;
    for &i in idx {
        s += Vec3::from_array(points[i]);
    }
    s * (1.0 / idx.len() as f64)
}

/// Inversa afin propia (cofactores 3x3 + traslacion).
fn inv(m: &Mat4) -> Mat4 {
    let a = m.m;
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    assert!(det.is_finite() && det.abs() > 1e-300);
    let mut r = Mat4::IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            let (i1, i2) = ((j + 1) % 3, (j + 2) % 3);
            let (j1, j2) = ((i + 1) % 3, (i + 2) % 3);
            r.m[i][j] = (a[i1][j1] * a[i2][j2] - a[i1][j2] * a[i2][j1]) / det;
        }
    }
    for i in 0..3 {
        r.m[i][3] = -(0..3).map(|k| r.m[i][k] * a[k][3]).sum::<f64>();
    }
    r
}

/// Oraculo independiente: marcadores distales, conjunto distal (replica del filtro D sin lazo), raiz,
/// grip y ancla del clip; mide la malla publicada y la nave de render del World.
struct Oracle {
    markers: [usize; 3],
    distal: Vec<usize>,
    root: Vec<usize>,
    grip: Vec<usize>,
    rest: Vec<V>,
    rest_normals: Vec<V>,
    faces: Vec<[usize; 3]>,
    degenerate_at_lock: Vec<bool>,
    local_at_lock: Vec<Vec3>,
    rel_at_lock: Mat4,
    prefix_vertices: Vec<V>,
}

fn marker_frame(v: &[V], m: [usize; 3]) -> Mat4 {
    let [p0, p1, p2] = m.map(|i| Vec3::from_array(v[i]));
    let x = (p1 - p0).normalized();
    let z = x.cross(p2 - p0).normalized();
    let y = z.cross(x);
    let mut f = Mat4::IDENTITY;
    for (c, a) in [x, y, z].into_iter().enumerate() {
        f.m[0][c] = a.x;
        f.m[1][c] = a.y;
        f.m[2][c] = a.z;
    }
    f.m[0][3] = p0.x;
    f.m[1][3] = p0.y;
    f.m[2][3] = p0.z;
    f
}
fn area(v: &[V], f: [usize; 3]) -> f64 {
    let [a, b, c] = f.map(|i| Vec3::from_array(v[i]));
    (b - a).cross(c - a).length()
}

impl Oracle {
    fn new(carry: &CaptureCarry, world: &World) -> Self {
        let clip = carry.preview().clip();
        let topo = clip.package().data().topology();
        let loop_d = topo.loop_d().to_vec();
        let distal: Vec<usize> = topo
            .vertices()
            .iter()
            .enumerate()
            .filter(|(i, x)| {
                matches!(x, GrowthVertex::Retained { side: Side::D, .. }) && !loop_d.contains(i)
            })
            .map(|(i, _)| i)
            .collect();
        let grip = clip.adapter().grip().to_vec();
        let (v, _) = carry.arm().expect("capa del bloqueo");
        // Marcadores: grip[0], el grip mas lejano y el que maximiza el area.
        let p = |i: usize| Vec3::from_array(v[i]);
        let i0 = grip[0];
        let i1 = *grip
            .iter()
            .max_by(|a, b| {
                (p(**a) - p(i0))
                    .length()
                    .total_cmp(&(p(**b) - p(i0)).length())
            })
            .unwrap();
        let i2 = *grip
            .iter()
            .max_by(|a, b| area(v, [i0, i1, **a]).total_cmp(&area(v, [i0, i1, **b])))
            .unwrap();
        let markers = [i0, i1, i2];
        let f = marker_frame(v, markers);
        let fi = inv(&f);
        let faces: Vec<[usize; 3]> = topo.faces().iter().map(|f| f.v).collect();
        let o = carry.body_override().expect("capa");
        let nv = o.model.vertices.len() - v.len();
        Oracle {
            markers,
            local_at_lock: distal.iter().map(|&i| fi.transform_point(p(i))).collect(),
            distal,
            root: clip.adapter().root().to_vec(),
            grip,
            rest: clip.adapter().rig().rest_vertices().to_vec(),
            rest_normals: clip.adapter().rig().rest_normals().to_vec(),
            degenerate_at_lock: faces
                .iter()
                .map(|f| {
                    let a = area(v, *f);
                    a.is_nan() || a <= 0.0
                })
                .collect(),
            faces,
            rel_at_lock: inv(&(body_m(world) * f)) * ship_m(world),
            prefix_vertices: o.model.vertices[..nv].to_vec(),
        }
    }
    /// Contacto (OBJ) del centroide grip de la MALLA publicada al ancla transportada con la nave de render.
    fn contact(&self, carry: &CaptureCarry, world: &World) -> f64 {
        let (v, _) = carry.arm().unwrap();
        let anchor = carry
            .preview()
            .clip()
            .adapter()
            .anchor()
            .desired_obj(&ship_m(world), &body_m(world))
            .unwrap();
        (centroid(v, &self.grip) - Vec3::from_array(anchor)).length()
    }
    /// Comprueba TODO lo publicado; devuelve (contacto, error de relacion rigida, error de forma distal).
    fn check(&self, carry: &CaptureCarry, world: &World, what: &str) -> (f64, f64, f64) {
        let (v, n) = carry.arm().expect("capa");
        assert!(
            v.iter().chain(n).all(|p| p.iter().all(|x| x.is_finite())),
            "{what}: no finito"
        );
        for &i in &self.root {
            assert_eq!(v[i], self.rest[i], "{what}: raiz no exacta en {i}");
        }
        for (a, b) in n.iter().zip(&self.rest_normals) {
            let d = (Vec3::from_array(*a).length() - Vec3::from_array(*b).length()).abs();
            assert!(d <= 1e-9, "{what}: normal sin la norma de reposo");
        }
        for (k, f) in self.faces.iter().enumerate() {
            assert!(
                self.degenerate_at_lock[k] || area(v, *f) > 0.0,
                "{what}: cara {k} degenerada nueva"
            );
        }
        let o = carry.body_override().expect("capa dibujada");
        let nv = self.prefix_vertices.len();
        assert_eq!(
            o.model.vertices.len(),
            nv + v.len(),
            "{what}: topologia de la capa"
        );
        assert!(
            o.model.vertices[..nv] == self.prefix_vertices[..],
            "{what}: prefijo Body cambiado"
        );
        let (c, r) = o.extra_bounds_obj;
        assert!(
            v.iter()
                .all(|p| (Vec3::from_array(*p) - c).length() <= r * (1.0 + 1e-12) + 1e-12),
            "{what}: la esfera no contiene el tramo"
        );
        let contact = self.contact(carry, world);
        assert!(
            contact <= CONTACT_TOLERANCE_OBJ,
            "{what}: contacto {contact}"
        );
        let f = marker_frame(v, self.markers);
        let fi = inv(&f);
        let shape = self
            .distal
            .iter()
            .zip(&self.local_at_lock)
            .map(|(&i, l)| (fi.transform_point(Vec3::from_array(v[i])) - *l).length())
            .fold(0.0, f64::max);
        assert!(shape <= RIGID_TOL, "{what}: forma distal {shape}");
        let rel = inv(&(body_m(world) * f)) * ship_m(world);
        let rigid = rel
            .m
            .iter()
            .flatten()
            .zip(self.rel_at_lock.m.iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(rigid <= RIGID_TOL, "{what}: relacion rigida {rigid}");
        (contact, rigid, shape)
    }
}

fn angular_zero(w: &World) -> bool {
    format!("{:?}", w.ship).contains("angular: Vec3 { x: 0.0, y: 0.0, z: 0.0 }")
}

/// Corre cuadros neutros hasta `Held` (+`extra`); comprueba el oraculo en cada uno.
fn to_held(g: &mut Game, carry: &mut CaptureCarry, o: &Oracle, extra: usize) -> Vec<CarryStatus> {
    let mut out = Vec::new();
    let mut held_at = None;
    for i in 0..3000usize {
        let (_, st) = step(g, carry, neutral(), DT);
        o.check(carry, &g.world, &format!("cuadro {i} {:?}", st.phase));
        if held_at.is_none() && st.phase == CarryPhase::Held {
            held_at = Some(i);
        }
        out.push(st);
        if held_at.is_some_and(|h| i >= h + extra) {
            return out;
        }
    }
    panic!("no llego a Held");
}

#[test]
#[ignore = "assets privados"]
fn k01_lock_on_real_gripped_is_exact_copy_with_cleared_inertia_and_data_plan() {
    fixture!(g, carry, true);
    // Plan derivado de datos: recalculado aqui desde el manifest y el rig.
    let clip = carry.preview().clip();
    let m = clip.package().capture().manifest();
    let st_frame = |name: &str| m.stages().iter().find(|(_, s)| s == name).unwrap().0;
    let (agarre, drag, fin) = (st_frame("agarre"), st_frame("arrastre"), st_frame("final"));
    let fps = m.fps() as f64;
    let plan = carry.plan().clone();
    assert_eq!(plan.stage_frames, [agarre, drag, fin]);
    assert_eq!(plan.hold_s, (drag - clip.grip_frame()) as f64 / fps);
    assert_eq!(plan.drag_s, (fin - drag) as f64 / fps);
    let c = Vec3::from_array(clip.adapter().rig().rest_loop().center);
    let pos = clip.package().data().base_positions(fin).unwrap();
    let authored = (centroid(pos, m.grip()) - c).length();
    assert!((plan.r_authored_obj - authored).abs() <= 1e-12);
    assert_eq!(
        plan.r_end_obj,
        authored.max(clip.adapter().rig().min_radius())
    );
    assert_eq!(plan.progress(0.0), 0.0);
    assert_eq!(plan.progress(plan.hold_s), 0.0);
    assert_eq!(plan.progress(plan.hold_s + plan.drag_s), 1.0);
    let mut last = 0.0;
    for k in 0..=400 {
        let p = plan.progress(k as f64 * (plan.hold_s + plan.drag_s + 1.0) / 400.0);
        assert!(
            p.is_finite() && p >= last && p <= 1.0,
            "progreso monotono acotado"
        );
        last = p;
    }

    let (_, before, st) = to_lock(&mut g, &mut carry);
    assert_eq!(st.phase, CarryPhase::Captured);
    assert!(carry.locks_flight());
    // Copia EXACTA del P1 final publicado por C en ESTE cuadro.
    let (cv, cn) = carry.preview().arm().unwrap();
    let (v, n) = carry.arm().unwrap();
    assert!(
        v == cv && n == cn,
        "primer cuadro: buffers distintos del P1 final"
    );
    let closure = carry.preview().closure().expect("cierre vivo");
    assert_eq!(closure.status().frame, clip.grip_frame());
    assert!(
        bits(&ship_m(&g.world), &before),
        "la nave salto al bloquear"
    );
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(
        angular_zero(&g.world),
        "inercia angular no limpia: {:?}",
        g.world.ship
    );
    assert_eq!(st.progress, 0.0);
    let r0 = st.r0_obj.unwrap();
    let t0 = Vec3::from_array(closure.pose().tip_obj);
    assert_eq!(r0, (t0 - c).length());
    let o = Oracle::new(&carry, &g.world);
    let (contact, _, _) = o.check(&carry, &g.world, "bloqueo");
    assert!(contact <= CONTACT_TOLERANCE_OBJ);
    assert!(carry.title().contains("CAPTURADA"), "{}", carry.title());
}

#[test]
#[ignore = "assets privados"]
fn k02_drag_retracts_and_moves_real_ship_with_rigid_grip_until_finite_held_end() {
    fixture!(g, carry, true);
    let (_, _, lock) = to_lock(&mut g, &mut carry);
    let o = Oracle::new(&carry, &g.world);
    let creature = g.world.creature_render_transform().unwrap().position;
    let d0 = (g.world.ship.position - creature).length();
    let p0 = g.world.ship.position;
    let r_end = carry.plan().r_end_obj;
    let sts = to_held(&mut g, &mut carry, &o, 90);
    let mut phases = vec![lock.phase];
    let mut last_r = lock.r0_obj.unwrap();
    let mut max_contact: f64 = 0.0;
    for st in &sts {
        if phases.last() != Some(&st.phase) {
            phases.push(st.phase);
        }
        let r = st.tip_radius_obj.unwrap();
        assert!(r <= last_r + 1e-12, "la punta volvio a alejarse");
        assert!(r >= r_end - 1e-9, "sobrepaso del extremo: {r} < {r_end}");
        last_r = r;
        max_contact = max_contact.max(o.contact(&carry, &g.world));
    }
    assert_eq!(
        phases,
        [CarryPhase::Captured, CarryPhase::Dragging, CarryPhase::Held]
    );
    let count = |e: &CarryEvent| sts.iter().filter(|s| has(s, e)).count();
    assert_eq!(count(&CarryEvent::DragStarted), 1);
    assert_eq!(count(&CarryEvent::HeldReached), 1);
    let end = sts.last().unwrap();
    assert!((end.tip_radius_obj.unwrap() - r_end).abs() <= 1e-9);
    assert_eq!(end.progress, 1.0);
    let d1 = (g.world.ship.position - creature).length();
    assert!(
        d0 - d1 > 150.0,
        "la nave no se acerco a la criatura: {d0} -> {d1}"
    );
    assert!(
        (g.world.ship.position - p0).length() > 150.0,
        "World.ship no se movio"
    );
    assert!(max_contact <= CONTACT_TOLERANCE_OBJ);
    // Retenida: la punta queda en el extremo finito y nada salta (contacto sigue exacto).
    assert!(carry.locks_flight());
    assert!(carry.title().contains("RETENIDA"), "{}", carry.title());
}

#[test]
#[ignore = "assets privados"]
fn k03_flight_and_view_keys_do_not_change_locked_trajectory_and_flight_returns_after_release() {
    fixture!(ga, a, true);
    fixture!(gb, b, true);
    let (fa, _, _) = to_lock(&mut ga, &mut a);
    let (fb, _, _) = to_lock(&mut gb, &mut b);
    assert_eq!(fa, fb, "misma ruta antes del bloqueo");
    let views = [
        SolarKeys {
            reset: true,
            ..SolarKeys::default()
        },
        SolarKeys {
            frontal: true,
            ..SolarKeys::default()
        },
        SolarKeys {
            oblique: true,
            ..SolarKeys::default()
        },
    ];
    for i in 0..420usize {
        let (oa, sa) = step(&mut ga, &mut a, neutral(), DT);
        let solar = if i % 40 == 7 {
            views[(i / 40) % 3]
        } else {
            SolarKeys::default()
        };
        let (ob, sb) = step(
            &mut gb,
            &mut b,
            controls(
                all_flight(),
                OxygenKeys::default(),
                solar,
                PreviewKeys::default(),
            ),
            DT,
        );
        assert_eq!(sa.phase, sb.phase, "cuadro {i}");
        assert!(
            bits(&ship_m(&ga.world), &ship_m(&gb.world)),
            "cuadro {i}: nave distinta"
        );
        assert_eq!(
            vbits(ga.world.camera.position),
            vbits(gb.world.camera.position)
        );
        let (va, na) = a.arm().unwrap();
        let (vb, nb) = b.arm().unwrap();
        assert!(va == vb && na == nb, "cuadro {i}: tramo distinto");
        assert_eq!(
            format!("{:?}", oa.effective_ship),
            format!("{:?}", ShipInput::default())
        );
        assert_eq!(
            format!("{:?}", ob.effective_ship),
            format!("{:?}", ShipInput::default())
        );
        assert_eq!(ob.solar_blocked, solar != SolarKeys::default());
        assert!(!ob.solar_applied.reset && !ob.solar_applied.frontal && !ob.solar_applied.oblique);
    }
    // Soltar con X y volar: la entrada vuelve a mover la nave real.
    let (_, st) = step(&mut gb, &mut b, with_keys(false, true), DT);
    assert!(has(&st, &CarryEvent::Released(ReleaseReason::ResetKey)));
    assert!(!b.locks_flight());
    let p = gb.world.ship.position;
    let (ob, _) = step(
        &mut gb,
        &mut b,
        controls(
            all_flight(),
            OxygenKeys::default(),
            SolarKeys::default(),
            PreviewKeys::default(),
        ),
        DT,
    );
    assert!(ob.effective_ship.thrust == 1.0 && ob.effective_ship.boost);
    assert!(
        (gb.world.ship.position - p).length() > 0.0,
        "sin vuelo tras soltar"
    );
}

/// Soltar con X y comprobar: sin teleport, inercia a 0, sin impulso, vuelo de vuelta, C reiniciada.
fn release_with_x(g: &mut Game, carry: &mut CaptureCarry, what: &str) {
    let before = g.world.ship.position;
    let (_, st) = step(g, carry, with_keys(false, true), DT);
    assert!(
        has(&st, &CarryEvent::Released(ReleaseReason::ResetKey)),
        "{what}: {:?}",
        st.events
    );
    assert!(
        has(&st, &CarryEvent::Preview(PreviewEvent::Reset)),
        "{what}: C sin reiniciar"
    );
    assert_eq!(st.phase, CarryPhase::Free(PreviewPhase::Pursuit), "{what}");
    assert!(!carry.locks_flight());
    assert_eq!(
        vbits(g.world.ship.position),
        vbits(before),
        "{what}: teleport al soltar"
    );
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(angular_zero(&g.world), "{what}: inercia angular");
    let p = g.world.ship.position;
    step(g, carry, neutral(), DT);
    assert_eq!(
        vbits(g.world.ship.position),
        vbits(p),
        "{what}: impulso tras soltar"
    );
}

#[test]
#[ignore = "assets privados"]
fn k04_x_releases_from_current_pose_while_waiting_and_at_start_middle_end() {
    fixture!(g, carry, true);
    // En espera: solicitud armada antes de la llegada; X es el reinicio de C (sin bloqueo).
    let (_, st) = step(&mut g, &mut carry, preset(0.05), DT);
    assert_eq!(st.phase, CarryPhase::Free(PreviewPhase::Pursuit));
    let (_, st) = step(&mut g, &mut carry, with_keys(true, false), DT);
    assert!(st.preview.armed);
    let (_, st) = step(&mut g, &mut carry, with_keys(false, true), DT);
    assert!(has(&st, &CarryEvent::Preview(PreviewEvent::Reset)) && !st.preview.armed);
    assert!(!carry.locks_flight());
    // Inicio (Captured).
    let (_, _, st) = to_lock(&mut g, &mut carry);
    assert_eq!(st.phase, CarryPhase::Captured);
    release_with_x(&mut g, &mut carry, "inicio");
    // Medio (Dragging, progreso >= 0,4).
    relock(&mut g, &mut carry);
    loop {
        let (_, st) = step(&mut g, &mut carry, neutral(), DT);
        if st.phase == CarryPhase::Dragging && st.progress >= 0.4 {
            break;
        }
    }
    release_with_x(&mut g, &mut carry, "medio");
    // Fin (Held). Tras soltar a mitad la nave quedo girada: R (FRONTAL) antes de pedir otra vez.
    let st = relock_with(&mut g, &mut carry, true);
    assert_eq!(st.phase, CarryPhase::Captured);
    loop {
        let (_, st) = step(&mut g, &mut carry, neutral(), DT);
        if st.phase == CarryPhase::Held {
            break;
        }
    }
    release_with_x(&mut g, &mut carry, "fin");
}

/// Sube el O2 (1) hasta que el arrastre suelta; comprueba la observacion y el vuelo libre.
fn release_with_oxygen(g: &mut Game, carry: &mut CaptureCarry, what: &str) {
    step(g, carry, preset(1.0), DT);
    let mut released = None;
    for i in 0..600 {
        let before = g.world.ship.position;
        let (_, st) = step(g, carry, neutral(), DT);
        if has(&st, &CarryEvent::Released(ReleaseReason::OxygenAboveEngage)) {
            assert!(
                g.world.debug_oxygen01 > 0.15,
                "{what}: solto con O2 {}",
                g.world.debug_oxygen01
            );
            assert_eq!(
                vbits(g.world.ship.position),
                vbits(before),
                "{what}: teleport"
            );
            released = Some(i);
            break;
        }
        assert!(
            g.world.debug_oxygen01 <= 0.15 || i == 0,
            "{what}: O2 alto sin soltar"
        );
    }
    assert!(released.is_some(), "{what}: el O2 no solto");
    assert_eq!(carry.status().phase, CarryPhase::Released);
    assert!(!carry.locks_flight());
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(angular_zero(&g.world));
    let frozen: Vec<V> = carry.arm().expect("observacion").0.to_vec();
    assert!(carry.title().contains("SUELTA"), "{}", carry.title());
    let p = g.world.ship.position;
    for _ in 0..30 {
        step(
            g,
            carry,
            controls(
                ShipInput {
                    thrust: -1.0,
                    ..ShipInput::default()
                },
                OxygenKeys::default(),
                SolarKeys::default(),
                PreviewKeys::default(),
            ),
            DT,
        );
    }
    assert!(
        (g.world.ship.position - p).length() > 1.0,
        "{what}: sin vuelo tras soltar"
    );
    assert!(
        carry.arm().unwrap().0 == frozen.as_slice(),
        "{what}: la observacion cambio"
    );
    let (_, st) = step(g, carry, with_keys(false, true), DT);
    assert!(has(&st, &CarryEvent::Preview(PreviewEvent::Reset)));
    assert_eq!(st.phase, CarryPhase::Free(PreviewPhase::Pursuit));
    step(g, carry, preset(0.05), DT);
}

#[test]
#[ignore = "assets privados"]
fn k05_oxygen_recovery_releases_at_start_middle_end_and_not_while_waiting() {
    fixture!(g, carry, true);
    // En espera (armada, sin llegada): subir el O2 no bloquea nada (C intacta).
    step(&mut g, &mut carry, preset(0.05), DT);
    step(&mut g, &mut carry, with_keys(true, false), DT);
    step(&mut g, &mut carry, preset(1.0), DT);
    for _ in 0..120 {
        let (_, st) = step(&mut g, &mut carry, neutral(), DT);
        assert!(matches!(st.phase, CarryPhase::Free(_)));
    }
    step(&mut g, &mut carry, with_keys(false, true), DT);
    let (_, _, _) = to_lock(&mut g, &mut carry);
    release_with_oxygen(&mut g, &mut carry, "inicio");
    // Esperar a que el O2 vuelva a bajar antes de pedir otra captura.
    for _ in 0..600 {
        step(&mut g, &mut carry, neutral(), DT);
    }
    relock(&mut g, &mut carry);
    loop {
        let (_, st) = step(&mut g, &mut carry, neutral(), DT);
        if st.phase == CarryPhase::Dragging && st.progress >= 0.4 {
            break;
        }
    }
    release_with_oxygen(&mut g, &mut carry, "medio");
    for _ in 0..600 {
        step(&mut g, &mut carry, neutral(), DT);
    }
    relock(&mut g, &mut carry);
    loop {
        let (_, st) = step(&mut g, &mut carry, neutral(), DT);
        if st.phase == CarryPhase::Held {
            break;
        }
    }
    release_with_oxygen(&mut g, &mut carry, "fin");
}

#[test]
#[ignore = "assets privados"]
fn k06_dt_zero_invalid_atomic_varied_steps_and_determinism() {
    fixture!(g, carry, true);
    fixture!(h, twin, true);
    to_lock(&mut g, &mut carry);
    to_lock(&mut h, &mut twin);
    for _ in 0..70 {
        step(&mut g, &mut carry, neutral(), DT);
        step(&mut h, &mut twin, neutral(), DT);
    }
    let snap = |g: &Game, c: &CaptureCarry| {
        (
            c.status(),
            c.arm().map(|(v, n)| (v.to_vec(), n.to_vec())),
            ship_m(&g.world),
            vbits(g.world.camera.position),
            c.body_override().map(|o| o.extra_bounds_obj),
        )
    };
    let s0 = snap(&g, &carry);
    assert_eq!(s0.0.phase, CarryPhase::Dragging);
    // dt invalido: Err y NADA cambia.
    for bad in [f64::NAN, f64::INFINITY, -1.0, 2.0] {
        assert!(
            carry
                .frame(&mut g.world, bad, PreviewKeys::default())
                .is_err()
        );
        let s = snap(&g, &carry);
        assert!(s.1 == s0.1 && bits(&s.2, &s0.2) && s.3 == s0.3 && s.4 == s0.4);
        assert_eq!(s.0.progress, s0.0.progress);
        assert_eq!(s.0.elapsed_s, s0.0.elapsed_s);
    }
    // dt 0: el reloj no avanza y la pose no cambia.
    let st = ok(
        carry.frame(&mut g.world, 0.0, PreviewKeys::default()),
        "dt 0",
    );
    assert_eq!(st.elapsed_s, s0.0.elapsed_s);
    assert!(carry.arm().map(|(v, _)| v.to_vec()) == s0.1.as_ref().map(|x| x.0.clone()));
    assert!(bits(&ship_m(&g.world), &s0.2));
    // Pasos variados (1/30, 1/144, 1/60, 0,1) hasta Held: contacto y extremo exactos.
    let o = Oracle::new(&carry, &g.world);
    let pattern = [1.0 / 30.0, 1.0 / 144.0, 1.0 / 60.0, 0.1];
    let mut k = 0;
    loop {
        let (_, st) = step(&mut g, &mut carry, neutral(), pattern[k % 4]);
        o.check(&carry, &g.world, &format!("paso variado {k}"));
        k += 1;
        if st.phase == CarryPhase::Held {
            assert!((st.tip_radius_obj.unwrap() - carry.plan().r_end_obj).abs() <= 1e-9);
            break;
        }
        assert!(k < 2000);
    }
    // Determinismo: el gemelo con los MISMOS pasos (sin los cuadros dt 0 / invalidos) da los mismos bits.
    let mut k = 0;
    loop {
        let (_, st) = step(&mut h, &mut twin, neutral(), pattern[k % 4]);
        k += 1;
        if st.phase == CarryPhase::Held {
            break;
        }
    }
    // Ambos recorrieron 70 cuadros de 1/60 y el mismo patron: comparar nave y tramo.
    let a = snap(&g, &carry);
    let b = snap(&h, &twin);
    assert!(a.1 == b.1, "tramo no determinista");
    assert!(bits(&a.2, &b.2), "nave no determinista");
}

#[test]
#[ignore = "assets privados"]
fn k07_scene_loss_is_atomic_err_then_explicit_abort_frees_ship_without_layer() {
    fixture!(g, carry, true);
    to_lock(&mut g, &mut carry);
    for _ in 0..90 {
        step(&mut g, &mut carry, neutral(), DT);
    }
    let st0 = carry.status();
    assert_eq!(st0.phase, CarryPhase::Dragging);
    let arm0 = carry.arm().map(|(v, _)| v.to_vec());
    let ship0 = ship_m(&g.world);
    // Sol invalido (cuerpo editado): Err atomico y nada cambia.
    let saved = g.world.system.bodies.clone();
    for b in g.world.system.bodies.iter_mut() {
        b.body_radius = -1.0;
    }
    assert!(
        carry
            .frame(&mut g.world, DT, PreviewKeys::default())
            .is_err()
    );
    assert!(carry.arm().map(|(v, _)| v.to_vec()) == arm0 && bits(&ship_m(&g.world), &ship0));
    assert_eq!(carry.status().elapsed_s, st0.elapsed_s);
    assert!(carry.locks_flight());
    g.world.system.bodies = saved;
    let (_, st) = step(&mut g, &mut carry, neutral(), DT);
    assert_eq!(
        st.phase,
        CarryPhase::Dragging,
        "se recupera con el Sol restaurado"
    );
    // Perdida de la criatura: Err atomico; el helper compartido ABORTA (escape explicito).
    let creature = g.world.clear_creature().expect("criatura");
    let arm1 = carry.arm().map(|(v, _)| v.to_vec());
    assert!(
        carry
            .frame(&mut g.world, DT, PreviewKeys::default())
            .is_err()
    );
    assert!(
        carry.arm().map(|(v, _)| v.to_vec()) == arm1,
        "Err no atomico"
    );
    let out = ok(
        advance_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut carry,
            neutral(),
            DT,
        ),
        "helper",
    );
    assert!(out.status.is_err() && out.aborted);
    let st = carry.status();
    assert_eq!(st.phase, CarryPhase::Aborted);
    assert!(
        st.events
            .iter()
            .any(|e| matches!(e, CarryEvent::Aborted(_)))
    );
    assert!(!carry.locks_flight());
    assert!(
        carry.body_override().is_none(),
        "abortada sin capa (sin render viejo)"
    );
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(angular_zero(&g.world));
    // La nave es libre aun sin escena.
    let p = g.world.ship.position;
    for _ in 0..10 {
        let _ = advance_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut carry,
            controls(
                all_flight(),
                OxygenKeys::default(),
                SolarKeys::default(),
                PreviewKeys::default(),
            ),
            DT,
        );
    }
    assert!((g.world.ship.position - p).length() > 0.0);
    // X sin escena: Err sin panico; con la criatura de vuelta, sin panico.
    let _ = carry.frame(
        &mut g.world,
        DT,
        PreviewKeys {
            request: false,
            reset: true,
        },
    );
    let _ = g.world.set_creature(creature);
    let _ = carry.frame(
        &mut g.world,
        DT,
        PreviewKeys {
            request: false,
            reset: true,
        },
    );
    assert!(!carry.locks_flight());
}

#[test]
#[ignore = "assets privados"]
fn k08_born_green_inactive_carry_equals_approved_c_route_frames_and_render() {
    // Ruta C pura (GraspPreview) frente al arrastre HABILITADO sin T y a la ruta legacy con el guion de C.
    fixture!(ga, carry, true);
    fixture!(gl, legacy, false);
    let mut gc = game();
    let PreviewInputs { debug, package } = ok(load_preview_inputs(&request()), "entradas");
    let pursuit = bound(&gc.world, debug);
    let rest = ok(arm_rest(&gc.world, &pursuit), "reposo");
    let clip = ok(ClosureClip::new(&package, &rest), "clip");
    let mut pv = ok(GraspPreview::new(&clip, pursuit), "preview C");
    let mut gripped_at = None;
    for i in 0..2400u64 {
        let o2 = OxygenKeys {
            preset: (i == 0).then_some(0.05),
            ..OxygenKeys::default()
        };
        let keys_c = PreviewKeys {
            request: i == 30,
            reset: false,
        };
        // C pura: el cuadro de su ejemplo (World primero, despues la preview).
        ok(
            umbral::app::advance_perception_frame(
                &mut gc.world,
                &mut gc.oxygen,
                o2,
                &ShipInput::default(),
                DT,
            ),
            "World C",
        );
        let sc = ok(pv.frame(&gc.world, DT, keys_c), "C");
        // Habilitado SIN T.
        let (_, sa) = step(
            &mut ga,
            &mut carry,
            controls(
                ShipInput::default(),
                o2,
                SolarKeys::default(),
                PreviewKeys::default(),
            ),
            DT,
        );
        // Legacy con el guion de C (T en 30).
        let (_, sl) = step(
            &mut gl,
            &mut legacy,
            controls(ShipInput::default(), o2, SolarKeys::default(), keys_c),
            DT,
        );
        assert_eq!(sl.phase, CarryPhase::Free(sc.phase), "legacy {i}");
        assert!(!sl.events.iter().any(|e| matches!(e, CarryEvent::Locked)));
        assert!(matches!(sa.phase, CarryPhase::Free(PreviewPhase::Pursuit)));
        assert!(
            bits(&ship_m(&gl.world), &ship_m(&gc.world))
                && bits(&body_m(&gl.world), &body_m(&gc.world))
        );
        assert!(bits(&ship_m(&ga.world), &ship_m(&gc.world)));
        assert_eq!(
            vbits(gl.world.camera.position),
            vbits(gc.world.camera.position)
        );
        assert_eq!(gl.world.time.to_bits(), gc.world.time.to_bits());
        assert!(legacy.arm() == pv.arm(), "legacy {i}: capa distinta de C");
        if sc.events.contains(&PreviewEvent::Gripped) {
            gripped_at = Some(i);
        }
        if gripped_at.is_some_and(|g| i == g + 120) || i % 600 == 599 {
            let mut fc = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
            let mut fl = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
            let rc = render_game_frame_with(
                &gc.world,
                &mut fc,
                Some(&gc.game),
                true,
                pv.body_override().as_ref(),
            )
            .unwrap();
            let rl = render_game_frame_with(
                &gl.world,
                &mut fl,
                Some(&gl.game),
                true,
                legacy.body_override().as_ref(),
            )
            .unwrap();
            assert!(fc.pixels == fl.pixels, "{i}: imagen legacy distinta de C");
            assert_eq!(format!("{:?}", rc.stats), format!("{:?}", rl.stats));
        }
        if gripped_at.is_some_and(|g| i >= g + 120) {
            break;
        }
    }
    assert!(
        gripped_at.is_some(),
        "la ruta legacy debe llegar al Gripped de C sin bloquear"
    );
}

#[test]
#[ignore = "assets privados"]
fn k09_added_after_green_arbitrary_initial_ship_orientation_keeps_rigid_grip_and_contact() {
    // Anadida tras el GREEN (generalidad del criterio 3, no RED): orientacion de partida con alabeo,
    // cabeceo y guinada; la relacion de agarre es la ACEPTADA en el bloqueo, sin alineacion nueva.
    fixture!(g, carry, true);
    let turn = Mat4::rotation_axis(Vec3::Z, 0.6)
        * Mat4::rotation_axis(Vec3::X, -0.25)
        * Mat4::rotation_axis(Vec3::Y, 0.3);
    g.world.ship.orientation = (g.world.ship.orientation * turn).orthonormalized();
    // H OFF (remedio documentado de C): con la nave girada camara/cuerpo pueden quedar en ciclo-2 y C
    // nunca veria la quietud que exige; sin seguimiento el cuerpo vuelve al frente y se detiene.
    let h = SolarKeys {
        follow: true,
        ..SolarKeys::default()
    };
    step(
        &mut g,
        &mut carry,
        controls(
            ShipInput::default(),
            OxygenKeys::default(),
            h,
            PreviewKeys::default(),
        ),
        DT,
    );
    assert!(!g.world.creature_horizontal_follow().unwrap().is_active());
    let (_, before, _) = to_lock(&mut g, &mut carry);
    assert!(
        bits(&ship_m(&g.world), &before),
        "la nave salto al bloquear"
    );
    let mut dot = 0.0;
    for c in 0..3 {
        dot += g
            .world
            .ship
            .orientation
            .column(c)
            .dot(Mat4::IDENTITY.column(c));
    }
    assert!(
        ((dot - 1.0) * 0.5).acos() > 0.3,
        "la orientacion de partida no es arbitraria"
    );
    let o = Oracle::new(&carry, &g.world);
    let sts = to_held(&mut g, &mut carry, &o, 30);
    assert_eq!(sts.last().unwrap().phase, CarryPhase::Held);
}

#[path = "support/parent_review_probes.rs"]
mod parent_review_probes;

#[test]
#[ignore = "assets privados; fix01 numeric preflight guards"]
fn fix01_shared_raw_masked_and_composed_overflow_are_atomic() {
    fixture!(g, carry, true);
    to_lock(&mut g, &mut carry);
    let snapshot = |g: &Game, c: &CaptureCarry| {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            g.world.ship,
            g.world.camera,
            g.oxygen,
            c.status(),
            g.game.view(),
            (g.world.time, g.world.debug_oxygen01),
            (
                g.world.creature_horizontal_follow(),
                g.world.creature_eye_target()
            )
        )
    };
    for field in 0..4 {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut controls = neutral();
            controls.solar = SolarKeys {
                reset: true,
                follow: true,
                target: true,
                ..SolarKeys::default()
            };
            match field {
                0 => controls.ship.thrust = invalid,
                1 => controls.ship.yaw = invalid,
                2 => controls.ship.pitch = invalid,
                _ => controls.ship.roll = invalid,
            }
            let before = snapshot(&g, &carry);
            let arm = carry.arm().map(|(v, n)| (v.to_vec(), n.to_vec()));
            assert!(
                advance_capture_frame(
                    &mut g.world,
                    &mut g.game,
                    &mut g.oxygen,
                    &mut carry,
                    controls,
                    DT
                )
                .is_err()
            );
            assert_eq!(snapshot(&g, &carry), before);
            assert!(carry.arm().map(|(v, n)| (v.to_vec(), n.to_vec())) == arm);
        }
    }
    // Finite raw angular control whose composed physics overflows must not be masked into acceptance.
    let mut controls = neutral();
    controls.ship.yaw = f64::MAX;
    controls.solar.follow = true;
    let before = snapshot(&g, &carry);
    assert!(
        advance_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut carry,
            controls,
            DT
        )
        .is_err()
    );
    assert_eq!(snapshot(&g, &carry), before);
    // Existing ordinary finite out-of-unit controls remain legal (no new clamp/restriction).
    controls.ship.yaw = 2.0;
    assert!(
        advance_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut carry,
            controls,
            DT
        )
        .is_ok()
    );
}
