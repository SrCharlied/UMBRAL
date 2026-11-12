//! GT3-AUTO: captura AUTOMATICA por contacto sobre la ruta del juego principal (World, superficie lucida,
//! ojo del vacio, Sol con ojo, percepcion) y el cuadro COMPARTIDO `advance_auto_capture_frame` que usa el
//! bucle de `app`. Vuelo REAL con controles MANTENIDOS y sin `T`. Assets privados opt-in (`#[ignore]`,
//! claves de BASELINE.json). Oraculos independientes: gemelo de juego sin captura (pose libre exacta hasta el
//! contacto), contacto en la MALLA contra el ancla con la matriz de render REAL, marco de marcadores distales,
//! inversa afin propia.
use umbral::{
    app::advance_perception_frame,
    assets::{obj::load_model, ship_model_path},
    math::{Mat4, Vec3},
    scene::{
        ship::ShipInput,
        world::{SHIP_MODEL_YAW, World},
    },
    visual::{
        lucid_game::set_lucid_surface,
        perception_game::{OxygenKeys, OxygenTransition},
        solar_game::{SolarGame, SolarKeys, solar_creature_placement},
        static_creature::StaticCreature,
        tentacle_auto_capture::*,
        tentacle_grasp_carry::{CaptureCarry, CarryPhase},
        tentacle_grasp_closure::ClosureClip,
        tentacle_grasp_preview::*,
        tentacle_grasp_transition::CONTACT_TOLERANCE_OBJ,
        tentacle_growth_cpu::{GrowthVertex, Side},
        tentacle_live_pursuit::PursuitMode,
        tentacle_solar_game::TentacleCaptureRequest,
        tentacle_spatial_pursuit::SpatialPursuit,
        void_game::set_void_eye,
    },
};

type V = [f64; 3];
const DT: f64 = 1.0 / 60.0;
const PRIVATE: &str = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE";
/// Relacion rigida y forma distal (round-trip de Euler ~1e-15; el mismo margen del contrato GT1-CARRY).
const RIGID_TOL: f64 = 1e-9;
/// Cuadros maximos hasta el contacto en los escenarios (el dominio existente, no una promesa universal).
const CONTACT_BUDGET: usize = 3000;

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
fn vbits(v: Vec3) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
fn ship_m(w: &World) -> Mat4 {
    w.ship.transform(SHIP_MODEL_YAW).matrix()
}
fn body_m(w: &World) -> Mat4 {
    w.creature_render_transform().unwrap().matrix()
}
fn angular_zero(w: &World) -> bool {
    format!("{:?}", w.ship).contains("angular: Vec3 { x: 0.0, y: 0.0, z: 0.0 }")
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

/// Escena + paquete + clip + preview C + captura automatica (la misma construccion que `app`).
macro_rules! fixture {
    ($g:ident, $auto:ident) => {
        let mut $g = game();
        let PreviewInputs { debug, package } = ok(load_preview_inputs(&request()), "entradas");
        let mut pursuit = SpatialPursuit::new(debug);
        ok(pursuit.bind(&$g.world).map(|_| ()), "bind real");
        ok(pursuit.step(&$g.world, 0.0).map(|_| ()), "paso 0");
        let rest = ok(arm_rest(&$g.world, &pursuit), "reposo");
        let clip = ok(ClosureClip::new(&package, &rest), "clip");
        let preview = ok(GraspPreview::new(&clip, pursuit), "preview C");
        #[allow(unused_mut)]
        let mut $auto = ok(AutoCapture::new(preview), "captura automatica");
    };
}

/// W + A + Arriba + Q + Shift mantenidos (teclado real: +-1): intento de escape a fondo.
fn escape() -> ShipInput {
    ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        pitch: 1.0,
        roll: 1.0,
        boost: true,
    }
}
/// A + Arriba + Q + Shift mantenidos: la nave gira sin parar (sus matrices cambian CADA cuadro).
fn spin() -> ShipInput {
    ShipInput {
        thrust: 0.0,
        yaw: 1.0,
        pitch: 1.0,
        roll: 1.0,
        boost: true,
    }
}
/// Vuelo de juego hasta el alcance (medido en d02 con la persecucion existente: con empuje SOSTENIDO la ley
/// de llegada deja la punta a v^2/2a detras y no hay alcance): `first` durante `switch` cuadros y despues
/// `then` mantenido. El alcance llega con la nave girando y aun desacelerando.
#[derive(Clone, Copy)]
struct Script {
    first: ShipInput,
    switch: usize,
    then: ShipInput,
}
impl Script {
    fn at(&self, i: usize) -> ShipInput {
        if i < self.switch {
            self.first
        } else {
            self.then
        }
    }
}
/// S5 de d02: W + A durante 5 s y despues los tres giros con impulso.
fn coast_spin() -> Script {
    Script {
        first: ShipInput {
            thrust: 1.0,
            yaw: 1.0,
            ..ShipInput::default()
        },
        switch: 300,
        then: spin(),
    }
}
/// S4 de d02: W + tres giros + impulso durante 1 s y despues los tres giros con impulso.
fn boost_spin() -> Script {
    Script {
        first: escape(),
        switch: 60,
        then: spin(),
    }
}
fn controls(ship: ShipInput, preset: Option<f64>) -> AutoControls {
    AutoControls {
        ship,
        oxygen: OxygenKeys {
            preset,
            ..OxygenKeys::default()
        },
        ..AutoControls::default()
    }
}

fn step(g: &mut Game, auto: &mut AutoCapture, c: AutoControls, elapsed: f64) -> AutoFrame {
    ok(
        advance_auto_capture_frame(&mut g.world, &mut g.game, &mut g.oxygen, auto, c, elapsed),
        "cuadro compartido",
    )
}
fn st(out: &AutoFrame) -> AutoStatus {
    out.status.clone().expect("cuadro sin error")
}
fn has(s: &AutoStatus, e: &AutoEvent) -> bool {
    s.events.contains(e)
}

/// Gemelo SIN captura: el juego de siempre (teclas solares + `advance_perception_frame`).
fn twin_step(t: &mut Game, c: AutoControls, elapsed: f64) {
    ok(t.game.apply(&mut t.world, c.solar), "teclas solares gemelo");
    ok(
        advance_perception_frame(&mut t.world, &mut t.oxygen, c.oxygen, &c.ship, elapsed),
        "gemelo",
    );
}

/// Contacto REAL de este cuadro segun el paso publicado de la persecucion (precondicion del escenario).
fn fresh(s: &AutoStatus, w: &World) -> bool {
    s.carry
        .preview
        .pursuit
        .is_some_and(|p| fresh_contact(&p, w))
}

/// Vuela con `ship` mantenido (O2 al 5 % en el cuadro 0) hasta el primer contacto real; devuelve
/// (cuadro, estado). Comprueba en cada cuadro previo que la nave NO esta capturada.
fn fly_to_contact(g: &mut Game, auto: &mut AutoCapture, script: Script) -> (usize, AutoFrame) {
    for i in 0..CONTACT_BUDGET {
        let out = step(
            g,
            auto,
            controls(script.at(i), (i == 0).then_some(0.05)),
            DT,
        );
        let s = st(&out);
        if fresh(&s, &g.world) || has(&s, &AutoEvent::Contact) {
            return (i, out);
        }
        assert!(
            !auto.flight_locked() && s.phase == AutoPhase::Pursuit,
            "cuadro {i}: capturada sin contacto ({:?})",
            s.phase
        );
    }
    panic!("el escenario no alcanzo la nave en {CONTACT_BUDGET} cuadros (precondicion)");
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

/// Oraculo independiente del bloqueo (mismo diseno que GT1-CARRY): marcadores grip, conjunto distal (filtro
/// D sin lazo), raiz exacta, esfera, prefijo Body, contacto contra el ancla con la nave de render REAL.
struct Oracle {
    markers: [usize; 3],
    distal: Vec<usize>,
    root: Vec<usize>,
    grip: Vec<usize>,
    rest: Vec<V>,
    faces: Vec<[usize; 3]>,
    degenerate_at_lock: Vec<bool>,
    local_at_lock: Vec<Vec3>,
    rel_at_lock: Mat4,
    prefix_vertices: Vec<V>,
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
    /// (contacto, relacion rigida, forma distal) de lo publicado ahora mismo.
    fn check(&self, carry: &CaptureCarry, world: &World, what: &str) -> (f64, f64, f64) {
        let (v, n) = carry.arm().expect("capa");
        assert!(
            v.iter().chain(n).all(|p| p.iter().all(|x| x.is_finite())),
            "{what}: no finito"
        );
        for &i in &self.root {
            assert_eq!(v[i], self.rest[i], "{what}: raiz no exacta en {i}");
        }
        for (k, f) in self.faces.iter().enumerate() {
            assert!(
                self.degenerate_at_lock[k] || area(v, *f) > 0.0,
                "{what}: cara {k} degenerada nueva"
            );
        }
        let o = carry.body_override().expect("capa dibujada");
        let nv = self.prefix_vertices.len();
        assert_eq!(o.model.vertices.len(), nv + v.len(), "{what}: topologia");
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

/// Tras el contacto: controles MANTENIDOS (y R/7/8 pulsadas) durante TODO el cierre; devuelve el estado del
/// cuadro del bloqueo (Gripped). Comprueba en cada cuadro: vuelo bloqueado, nave en la pose de contacto
/// (bits), cuerpo inmovil (bits), sin cancelacion ni espera de quietud.
fn hold_through_closure(
    g: &mut Game,
    auto: &mut AutoCapture,
    ship: ShipInput,
    contact: (Mat4, Mat4, Vec3),
) -> (usize, AutoStatus) {
    let (s0, b0, p0) = contact;
    for i in 0..1200usize {
        let mut c = controls(ship, None);
        c.solar = SolarKeys {
            reset: i % 97 == 5,
            frontal: i % 89 == 7,
            oblique: i % 83 == 11,
            ..SolarKeys::default()
        };
        let out = step(g, auto, c, DT);
        let s = st(&out);
        let what = format!("cierre cuadro {i} {:?}", s.phase);
        assert!(auto.flight_locked(), "{what}: vuelo libre");
        assert!(
            out.effective_ship.thrust == 0.0 && !out.effective_ship.boost,
            "{what}: entrada efectiva no neutra"
        );
        if c.solar.reset || c.solar.frontal || c.solar.oblique {
            assert!(out.solar_blocked, "{what}: R/7/8 no bloqueadas");
        }
        assert_eq!(g.world.ship.speed, 0.0, "{what}: velocidad");
        assert!(angular_zero(&g.world), "{what}: inercia angular");
        assert_eq!(
            vbits(g.world.ship.position),
            vbits(p0),
            "{what}: la nave se movio"
        );
        assert!(bits(&ship_m(&g.world), &s0), "{what}: matriz de nave");
        assert!(
            !matches!(
                s.carry.preview.phase,
                PreviewPhase::Cancelled | PreviewPhase::Failed
            ),
            "{what}: el gesto se cancelo ({:?})",
            s.carry.preview
        );
        if has(&s, &AutoEvent::Locked) {
            assert!(bits(&body_m(&g.world), &b0), "{what}: cuerpo en el Gripped");
            return (i, s);
        }
        assert_eq!(s.phase, AutoPhase::Closing, "{what}");
        assert!(out.body_held, "{what}: cuerpo no inmovilizado");
        assert!(bits(&body_m(&g.world), &b0), "{what}: el cuerpo se movio");
    }
    panic!("el cierre no llego al Gripped con controles mantenidos");
}

#[test]
#[ignore = "assets privados"]
fn a01_held_flight_without_t_locks_exactly_at_fresh_contact_and_completes_gesture_drag_and_hold() {
    fixture!(g, auto);
    let mut twin = game();
    // Gemelo: el juego de siempre con los MISMOS controles hasta el contacto (pose libre exacta).
    let script = coast_spin();
    let ship = escape();
    let mut contact = None;
    for i in 0..CONTACT_BUDGET {
        let c = controls(script.at(i), (i == 0).then_some(0.05));
        twin_step(&mut twin, c, DT);
        let out = step(&mut g, &mut auto, c, DT);
        let s = st(&out);
        if fresh(&s, &g.world) || has(&s, &AutoEvent::Contact) {
            contact = Some((i, s));
            break;
        }
        assert!(!auto.flight_locked() && s.phase == AutoPhase::Pursuit);
        assert_eq!(
            vbits(g.world.ship.position),
            vbits(twin.world.ship.position)
        );
        assert!(bits(&body_m(&g.world), &body_m(&twin.world)));
    }
    let (i, s) = contact.expect("precondicion: el escenario no alcanzo la nave");
    println!(
        "contacto real en el cuadro {i}: nave {:?} v_gemelo {:.3} u/s | O2 {:.3}",
        g.world.ship.position, twin.world.ship.speed, g.world.debug_oxygen01
    );
    // La nave del gemelo sigue girando (y desacelerando): la quietud de C nunca se cumpliria.
    assert!(!angular_zero(&twin.world), "la nave no giraba al contacto");
    assert!(
        twin.world.ship.speed != 0.0,
        "la nave no se movia al contacto"
    );
    assert!(
        has(&s, &AutoEvent::Contact),
        "contacto real en el cuadro {i} SIN captura en ese cuadro: eventos {:?} | preview {:?}",
        s.events,
        s.carry.preview.phase
    );
    // Captura donde ESTA (pose del gemelo libre en ese mismo cuadro) y sin velocidad ni inercia.
    assert_eq!(s.contact_frame, Some(i as u64));
    assert_eq!(
        vbits(g.world.ship.position),
        vbits(twin.world.ship.position)
    );
    assert!(bits(&ship_m(&g.world), &ship_m(&twin.world)));
    assert!(bits(&body_m(&g.world), &body_m(&twin.world)));
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(angular_zero(&g.world));
    assert!(auto.flight_locked());
    assert_eq!(s.phase, AutoPhase::Closing);
    let contact_pose = (ship_m(&g.world), body_m(&g.world), g.world.ship.position);
    let d0 = (g.world.ship.position - g.world.creature().unwrap().placement().center()).length();
    let (k, locked) = hold_through_closure(&mut g, &mut auto, ship, contact_pose);
    println!("Gripped/bloqueo {k} cuadros despues del contacto");
    assert_eq!(locked.phase, AutoPhase::Captured);
    // Arrastre y retencion aprobados con controles mantenidos; oraculo en cada cuadro.
    let o = Oracle::new(auto.carry(), &g.world);
    let (mut held_at, mut max_contact, mut max_rigid) = (None, 0.0f64, 0.0f64);
    let mut yaw_at_held = None;
    for j in 0..1200usize {
        let out = step(&mut g, &mut auto, controls(ship, None), DT);
        let s = st(&out);
        assert!(auto.flight_locked() && out.effective_ship.thrust == 0.0);
        let (c, r, _) = o.check(
            auto.carry(),
            &g.world,
            &format!("arrastre {j} {:?}", s.phase),
        );
        (max_contact, max_rigid) = (max_contact.max(c), max_rigid.max(r));
        if held_at.is_none() && s.phase == AutoPhase::Held {
            held_at = Some(j);
            yaw_at_held = g.world.creature_horizontal_follow().map(|f| f.yaw());
        }
        if held_at.is_some_and(|h| j >= h + 120) {
            break;
        }
    }
    let h = held_at.expect("no llego a RETENIDA");
    let d1 = (g.world.ship.position - g.world.creature().unwrap().placement().center()).length();
    let drift = g
        .world
        .creature_horizontal_follow()
        .map(|f| f.yaw())
        .zip(yaw_at_held)
        .map(|(a, b)| (a - b).to_degrees() / 2.0);
    println!(
        "retenida {h} cuadros tras el bloqueo | distancia {d0:.1} -> {d1:.1} u | contacto max {max_contact:.2e} | rigida max {max_rigid:.2e} | deriva H {drift:?} grados/s"
    );
    assert!(d0 - d1 > 150.0, "la nave no fue arrastrada: {d0} -> {d1}");
}

#[test]
#[ignore = "assets privados"]
fn a02_first_published_layer_after_contact_is_exact_copy_of_pursuit_step() {
    fixture!(g, auto);
    let (i, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
    let s = st(&out);
    assert!(
        has(&s, &AutoEvent::Contact),
        "contacto real en el cuadro {i} sin captura: {:?}",
        s.events
    );
    let pursuit = auto.carry().preview().pursuit();
    let composite = pursuit.composite().expect("capa de la persecucion");
    let layer = auto.body_override().expect("capa del episodio");
    assert!(
        layer.model.vertices == composite.vertices && layer.model.normals == composite.normals,
        "la primera capa tras el contacto no es la copia exacta del paso de persecucion"
    );
    assert_eq!(layer.model.faces.len(), composite.faces.len());
    let reached = auto.carry().preview().reached().expect("pose alcanzada");
    let (nv, _) = pursuit.body_prefix().unwrap();
    assert!(reached.vertices() == &composite.vertices[nv..]);
    assert_eq!(s.carry.preview.phase, PreviewPhase::Handoff);
}

#[test]
#[ignore = "assets privados"]
fn a03_born_green_no_capture_while_pursuit_inactive_and_free_flight_equals_game() {
    fixture!(g, auto);
    let mut twin = game();
    // O2 al 100 %: la persecucion existente esta inactiva; vuelo agresivo mantenido sin captura.
    let ship = ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        pitch: -1.0,
        roll: 1.0,
        boost: true,
    };
    for i in 0..600 {
        let c = controls(ship, None);
        twin_step(&mut twin, c, DT);
        let out = step(&mut g, &mut auto, c, DT);
        let s = st(&out);
        assert_eq!(s.phase, AutoPhase::Pursuit, "cuadro {i}");
        assert!(s.events.is_empty() && !auto.flight_locked());
        assert_eq!(
            s.carry.preview.pursuit.map(|p| p.step.mode),
            Some(PursuitMode::Idle)
        );
        assert!(auto.body_override().is_none(), "sin capa en reposo");
        assert_eq!(
            vbits(g.world.ship.position),
            vbits(twin.world.ship.position)
        );
        assert!(bits(&ship_m(&g.world), &ship_m(&twin.world)));
        assert!(bits(&body_m(&g.world), &body_m(&twin.world)));
        assert_eq!(
            vbits(g.world.camera.position),
            vbits(twin.world.camera.position)
        );
    }
}

/// Lleva al contacto y avanza `frames` cuadros del cierre con controles mantenidos.
fn into_closure(g: &mut Game, auto: &mut AutoCapture, frames: usize) -> AutoStatus {
    let (i, out) = fly_to_contact(g, auto, coast_spin());
    let mut s = st(&out);
    assert!(has(&s, &AutoEvent::Contact), "contacto {i} sin captura");
    for _ in 0..frames {
        s = st(&step(g, auto, controls(escape(), None), DT));
        assert_eq!(s.phase, AutoPhase::Closing);
    }
    s
}

/// Sube el O2 (preset 1) con controles mantenidos hasta soltar; comprueba suelta sin inercia ni bloqueo.
fn release_by_oxygen(g: &mut Game, auto: &mut AutoCapture, what: &str) {
    let mut released = None;
    for i in 0..400 {
        let p = g.world.ship.position;
        let out = step(g, auto, controls(escape(), (i == 0).then_some(1.0)), DT);
        let s = st(&out);
        if s.events
            .iter()
            .any(|e| matches!(e, AutoEvent::Released(ReleaseCause::OxygenRecovered)))
        {
            assert!(
                g.world.debug_oxygen01 > 0.15,
                "{what}: suelta sin O2 > 15 %"
            );
            assert_eq!(
                vbits(g.world.ship.position),
                vbits(p),
                "{what}: teleport al soltar"
            );
            assert_eq!(g.world.ship.speed, 0.0, "{what}: velocidad al soltar");
            assert!(angular_zero(&g.world), "{what}: inercia al soltar");
            assert!(!auto.flight_locked(), "{what}: sigue bloqueada");
            assert_eq!(s.phase, AutoPhase::Observation, "{what}");
            released = Some(i);
            break;
        }
        assert!(auto.flight_locked(), "{what}: libre antes de soltar");
    }
    released.unwrap_or_else(|| panic!("{what}: el O2 no solto la nave"));
    // Sin impulso viejo: el empuje mantenido arranca desde 0 con la respuesta de siempre (un paso).
    let out = step(g, auto, controls(escape(), None), DT);
    assert!(
        out.effective_ship.thrust == 1.0,
        "{what}: entrada no devuelta"
    );
    let v = g.world.ship.speed;
    let one_step = 300.0 * (1.0 - (-1.5 * DT).exp());
    assert!(
        v > 0.0 && v <= one_step * (1.0 + 1e-9),
        "{what}: velocidad tras soltar {v} (un paso desde 0: {one_step})"
    );
    // Re-armado: al ocultarse la criatura (O2 alto) la persecucion vuelve al reposo.
    let mut rearmed = false;
    for _ in 0..600 {
        let s = st(&step(g, auto, controls(escape(), None), DT));
        assert!(!auto.flight_locked());
        if has(&s, &AutoEvent::Rearmed) {
            assert_eq!(s.phase, AutoPhase::Pursuit);
            rearmed = true;
            break;
        }
    }
    assert!(rearmed, "{what}: sin re-armado al ocultarse la criatura");
}

#[test]
#[ignore = "assets privados"]
fn a04_oxygen_recovery_releases_during_handoff_closure_and_drag_without_permalock() {
    {
        fixture!(g, auto);
        into_closure(&mut g, &mut auto, 10);
        release_by_oxygen(&mut g, &mut auto, "traspaso B1");
    }
    {
        fixture!(g, auto);
        let s = into_closure(&mut g, &mut auto, 200);
        assert!(matches!(
            s.carry.preview.phase,
            PreviewPhase::Preparing | PreviewPhase::Closing
        ));
        release_by_oxygen(&mut g, &mut auto, "cierre P1");
    }
    {
        fixture!(g, auto);
        let (i, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
        assert!(has(&st(&out), &AutoEvent::Contact), "contacto {i}");
        let contact = (ship_m(&g.world), body_m(&g.world), g.world.ship.position);
        hold_through_closure(&mut g, &mut auto, escape(), contact);
        for _ in 0..90 {
            step(&mut g, &mut auto, controls(escape(), None), DT);
        }
        assert_eq!(auto.status().phase, AutoPhase::Dragging);
        release_by_oxygen(&mut g, &mut auto, "arrastre");
    }
}

#[test]
#[ignore = "assets privados"]
fn a05_stale_clamped_and_shifted_steps_are_not_contact() {
    fixture!(g, auto);
    let (_, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
    let s = st(&out);
    let step0 = s.carry.preview.pursuit.expect("paso del contacto");
    assert!(fresh_contact(&step0, &g.world));
    // El mismo paso con la nave desplazada 1 ulp ya no es contacto (ni proximidad ni estado viejo).
    let saved = g.world.ship.position;
    g.world.ship.position.x = f64::from_bits(saved.x.to_bits() + 1);
    assert!(!fresh_contact(&step0, &g.world), "paso viejo aceptado");
    g.world.ship.position = saved;
    let mut clamped = step0;
    clamped.step.target_clamped = true;
    assert!(
        !fresh_contact(&clamped, &g.world),
        "objetivo recortado aceptado"
    );
    let mut idle = step0;
    idle.step.mode = PursuitMode::Retracting;
    assert!(!fresh_contact(&idle, &g.world), "recogida aceptada");
    let mut no_shaft = step0;
    no_shaft.shaft = None;
    assert!(!fresh_contact(&no_shaft, &g.world), "sin tramo aceptado");
}

#[test]
#[ignore = "assets privados"]
fn a06_scene_identity_loss_during_closure_is_atomic_then_aborts_without_permalock() {
    fixture!(g, auto);
    into_closure(&mut g, &mut auto, 30);
    let p = g.world.ship.position;
    let before = auto.status();
    let creature = g.world.clear_creature().expect("criatura");
    assert!(
        auto.frame(&mut g.world, DT, false).is_err(),
        "sin criatura debe fallar"
    );
    assert_eq!(auto.status(), before, "Err no atomico");
    assert!(auto.flight_locked());
    let out = ok(
        advance_auto_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut auto,
            controls(escape(), None),
            DT,
        ),
        "helper",
    );
    assert!(out.status.is_err() && out.aborted, "el helper no aborto");
    assert_eq!(auto.status().phase, AutoPhase::Aborted);
    assert!(!auto.flight_locked(), "permalock tras perder la escena");
    assert!(auto.body_override().is_none(), "abortada sin capa");
    assert_eq!(vbits(g.world.ship.position), vbits(p));
    assert_eq!(g.world.ship.speed, 0.0);
    assert!(angular_zero(&g.world));
    for _ in 0..5 {
        let _ = advance_auto_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut auto,
            controls(escape(), None),
            DT,
        );
    }
    assert!(
        (g.world.ship.position - p).length() > 0.0,
        "la nave no es libre"
    );
    let _ = g.world.set_creature(creature);
}

#[test]
#[ignore = "assets privados"]
fn a07_dt_invalid_atomic_zero_holds_and_varied_steps_are_deterministic() {
    let run = |pattern: &[f64]| {
        fixture!(g, auto);
        let mut trace = Vec::new();
        for i in 0..4000usize {
            let e = pattern[i % pattern.len()];
            let input = if auto.flight_locked() {
                escape()
            } else {
                coast_spin().at(i)
            };
            let s = st(&step(
                &mut g,
                &mut auto,
                controls(input, (i == 0).then_some(0.05)),
                e,
            ));
            if !s.events.is_empty() {
                trace.push((i, format!("{:?}", s.events)));
            }
            if s.phase == AutoPhase::Held {
                return (trace, vbits(g.world.ship.position), g.world.time.to_bits());
            }
        }
        panic!("sin Held con pasos {pattern:?}");
    };
    let varied = [DT, 1.0 / 30.0, 0.005, 1.0 / 45.0];
    let a = run(&varied);
    let b = run(&varied);
    assert_eq!(a, b, "pasos variados no deterministas");
    assert!(a.0.iter().any(|(_, e)| e.contains("Contact")));
    // Invalidos y cero sobre un cierre vivo.
    fixture!(g, auto);
    into_closure(&mut g, &mut auto, 20);
    let (s0, w0) = (auto.status(), format!("{:?}", g.world.ship));
    for e in [f64::NAN, -DT, f64::INFINITY] {
        assert!(
            advance_auto_capture_frame(
                &mut g.world,
                &mut g.game,
                &mut g.oxygen,
                &mut auto,
                controls(escape(), None),
                e,
            )
            .is_err(),
            "elapsed {e} aceptado"
        );
        assert_eq!(auto.status(), s0, "elapsed {e}: estado cambiado");
        assert_eq!(
            format!("{:?}", g.world.ship),
            w0,
            "elapsed {e}: nave cambiada"
        );
    }
    let h0 = s0.carry.preview.handoff.map(|h| h.elapsed_s);
    let s = st(&step(&mut g, &mut auto, controls(escape(), None), 0.0));
    assert_eq!(s.phase, AutoPhase::Closing);
    assert_eq!(
        s.carry.preview.handoff.map(|h| h.elapsed_s),
        h0,
        "dt 0 avanzo el gesto"
    );
}

#[test]
#[ignore = "assets privados"]
fn a08_emergency_x_is_diagnostic_release_during_closure_and_drag() {
    {
        fixture!(g, auto);
        into_closure(&mut g, &mut auto, 40);
        let p = g.world.ship.position;
        let mut c = controls(escape(), None);
        c.emergency = true;
        let s = st(&step(&mut g, &mut auto, c, DT));
        assert!(
            has(&s, &AutoEvent::Released(ReleaseCause::Emergency)),
            "{:?}",
            s.events
        );
        assert!(!auto.flight_locked());
        assert_eq!(vbits(g.world.ship.position), vbits(p));
        assert_eq!(g.world.ship.speed, 0.0);
    }
    {
        fixture!(g, auto);
        let (i, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
        assert!(has(&st(&out), &AutoEvent::Contact), "contacto {i}");
        let contact = (ship_m(&g.world), body_m(&g.world), g.world.ship.position);
        hold_through_closure(&mut g, &mut auto, escape(), contact);
        for _ in 0..60 {
            step(&mut g, &mut auto, controls(escape(), None), DT);
        }
        let mut c = controls(escape(), None);
        c.emergency = true;
        let s = st(&step(&mut g, &mut auto, c, DT));
        assert!(
            has(&s, &AutoEvent::Released(ReleaseCause::Emergency)),
            "{:?}",
            s.events
        );
        assert!(!auto.flight_locked());
        assert_eq!(g.world.ship.speed, 0.0);
    }
}

#[test]
#[ignore = "assets privados"]
fn a09_forward_turning_contact_keeps_rigid_grip_with_current_world_and_body() {
    fixture!(g, auto);
    // Impulso con los tres giros y despues giro mantenido: contacto con otra orientacion y traslacion.
    let ship = escape();
    let (i, out) = fly_to_contact(&mut g, &mut auto, boost_spin());
    let s = st(&out);
    assert!(has(&s, &AutoEvent::Contact), "contacto {i} sin captura");
    let contact = (ship_m(&g.world), body_m(&g.world), g.world.ship.position);
    hold_through_closure(&mut g, &mut auto, ship, contact);
    let o = Oracle::new(auto.carry(), &g.world);
    let mut held = false;
    for j in 0..600 {
        let s = st(&step(&mut g, &mut auto, controls(ship, None), DT));
        o.check(auto.carry(), &g.world, &format!("adelante {j}"));
        if s.phase == AutoPhase::Held {
            held = true;
            break;
        }
    }
    assert!(held, "sin RETENIDA");
    assert_eq!(auto.carry().status().phase, CarryPhase::Held);
}

/// Forma local del conjunto distal en el marco de marcadores grip (independiente de la pose rigida).
fn distal_local(carry: &CaptureCarry) -> Vec<Vec3> {
    let clip = carry.preview().clip();
    let topo = clip.package().data().topology();
    let loop_d = topo.loop_d().to_vec();
    let (v, _) = carry.arm().expect("capa");
    let grip = clip.adapter().grip().to_vec();
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
    let fi = inv(&marker_frame(v, [i0, i1, i2]));
    topo.vertices()
        .iter()
        .enumerate()
        .filter(|(i, x)| {
            matches!(x, GrowthVertex::Retained { side: Side::D, .. }) && !loop_d.contains(i)
        })
        .map(|(i, _)| fi.transform_point(p(i)))
        .collect()
}

#[test]
#[ignore = "assets privados"]
fn a10_characterization_gesture_phases_match_manual_route_with_declared_time_alignment() {
    use umbral::visual::tentacle_grasp_carry::{CarryEvent, FrameControls, advance_capture_frame};
    // Ruta MANUAL aprobada (T con la nave quieta, cuadro compartido de GT1-CARRY).
    let mut gm = game();
    let PreviewInputs { debug, package } = ok(load_preview_inputs(&request()), "entradas");
    let mut pursuit = SpatialPursuit::new(debug);
    ok(pursuit.bind(&gm.world).map(|_| ()), "bind");
    ok(pursuit.step(&gm.world, 0.0).map(|_| ()), "paso 0");
    let rest = ok(arm_rest(&gm.world, &pursuit), "reposo");
    let clip = ok(ClosureClip::new(&package, &rest), "clip");
    let mut manual = ok(
        CaptureCarry::new(ok(GraspPreview::new(&clip, pursuit), "C"), true),
        "arrastre",
    );
    let (mut m_captured, mut m_locked, mut m_held, mut m_shape) = (None, None, None, None);
    for i in 0..4000usize {
        let c = FrameControls {
            oxygen: OxygenKeys {
                preset: (i == 0).then_some(0.05),
                ..OxygenKeys::default()
            },
            preview: PreviewKeys {
                request: i == 0,
                reset: false,
            },
            ..FrameControls::default()
        };
        let out = ok(
            advance_capture_frame(
                &mut gm.world,
                &mut gm.game,
                &mut gm.oxygen,
                &mut manual,
                c,
                DT,
            ),
            "manual",
        );
        let s = out.status.expect("manual sin error");
        if s.events
            .contains(&CarryEvent::Preview(PreviewEvent::Captured))
        {
            m_captured = Some(i);
        }
        if s.events.contains(&CarryEvent::Locked) {
            m_locked = Some(i);
            m_shape = Some(distal_local(&manual));
        }
        if s.events.contains(&CarryEvent::HeldReached) {
            m_held = Some(i);
            break;
        }
    }
    let (mc, ml, mh) = (
        m_captured.expect("manual sin captura"),
        m_locked.expect("manual sin bloqueo"),
        m_held.expect("manual sin retencion"),
    );
    // Ruta AUTOMATICA (contacto con la nave girando y controles mantenidos).
    fixture!(g, auto);
    let (ac, out) = fly_to_contact(&mut g, &mut auto, coast_spin());
    assert!(has(&st(&out), &AutoEvent::Contact));
    let (mut a_locked, mut a_held, mut a_shape) = (None, None, None);
    for i in ac + 1..ac + 2000 {
        let s = st(&step(&mut g, &mut auto, controls(escape(), None), DT));
        if has(&s, &AutoEvent::Locked) {
            a_locked = Some(i);
            a_shape = Some(distal_local(auto.carry()));
        }
        if has(&s, &AutoEvent::HeldReached) {
            a_held = Some(i);
            break;
        }
    }
    let (al, ah) = (
        a_locked.expect("auto sin bloqueo"),
        a_held.expect("auto sin retencion"),
    );
    // Alineamiento temporal DECLARADO: el contacto cambia CUANDO empieza (cuadro de captura), no la duracion
    // de cada fase (B1 + P1 hasta el Gripped, agarre + arrastre hasta la retencion).
    println!(
        "manual: captura {mc} bloqueo {ml} retencion {mh} | auto: contacto {ac} bloqueo {al} retencion {ah}"
    );
    assert_eq!(
        al - ac,
        ml - mc,
        "B1+P1 no dura lo mismo que en la ruta manual"
    );
    assert_eq!(
        ah - al,
        mh - ml,
        "agarre+arrastre no dura lo mismo que en la ruta manual"
    );
    let (ms, as_) = (m_shape.unwrap(), a_shape.unwrap());
    assert_eq!(ms.len(), as_.len());
    let diff = ms
        .iter()
        .zip(&as_)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0, f64::max);
    let size = ms.iter().map(|a| a.length()).fold(0.0, f64::max);
    println!("forma distal local en el bloqueo: dif max {diff:.3e} OBJ (tamano {size:.3} OBJ)");
    assert!(diff.is_finite());
}

#[path = "support/parent_mech_atomic.rs"]
mod parent_mech_atomic;

/// CORRECTION05 (API defensiva, extension autorizada): el MISMO cuadro compartido no debe consumir el tick que
/// alcanza el terminal y devolver `frozen` con una criatura PRESENTE ajena (misma geometria, otra identidad).
/// No afirma que un jugador cambie la criatura en partida: la fuente es `World::set_creature` publico.
mod correction05_auto_terminal_tick_identity {
    use super::*;

    fn snapshot(g: &Game, auto: &AutoCapture) -> String {
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            g.world.ship,
            g.world.camera,
            (g.oxygen.shown(), g.oxygen.target()),
            g.game.view(),
            (
                g.world.time,
                g.world.debug_oxygen01,
                g.world.forced_oxygen,
                g.world.gameplay.oxygen01
            ),
            (
                g.world.gameplay.game_over,
                g.world.gameplay.objective_complete
            ),
            (
                g.world.creature_horizontal_follow(),
                g.world.creature_eye_target()
            ),
            auto.status(),
            auto.body_override().map(|b| b.model.vertices.clone())
        )
    }

    /// `captured`: cierre con la nave capturada (retorno tras `update_with_pose_owner`); si no, persecucion
    /// libre (retorno tras `advance_perception_frame_owned`). Reserva real de O2 menor que el consumo del tick.
    fn check(captured: bool) {
        fixture!(g, auto);
        if captured {
            into_closure(&mut g, &mut auto, 30);
            assert!(auto.flight_locked(), "precondicion: nave capturada");
        } else {
            for _ in 0..3 {
                step(&mut g, &mut auto, controls(spin(), None), DT);
            }
            assert!(
                !auto.flight_locked() && auto.status().phase == AutoPhase::Pursuit,
                "precondicion: persecucion libre"
            );
        }
        let placement = g.world.creature().unwrap().placement();
        let mut model = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
        model
            .material_libraries
            .push("correction05-auto-foreign-identity.mtl".into());
        let texture =
            umbral::renderer::texture::Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap();
        let alien = StaticCreature::new(model, texture, placement).unwrap();
        ok(g.world.set_creature(alien), "reemplazo publico de escena");
        g.world.gameplay.game_over = false;
        g.world.gameplay.objective_complete = false;
        g.world.gameplay.oxygen01 = g.world.gameplay.settings.drain_per_second * DT / 2.0;
        let before = snapshot(&g, &auto);
        let result = advance_auto_capture_frame(
            &mut g.world,
            &mut g.game,
            &mut g.oxygen,
            &mut auto,
            controls(escape(), None),
            DT,
        );
        let unchanged = snapshot(&g, &auto) == before;
        let ended = g.world.gameplay.game_over;
        println!(
            "CORRECTION05_AUTO captured={captured} rejected={} game_over={ended} unchanged={unchanged}",
            result.is_err()
        );
        assert!(
            result.is_err() && unchanged,
            "AUTO captured={captured}: identidad ajena aceptada o tick consumido antes de rechazarla: ok={} game_over_tras_cuadro={ended} sin_efectos={unchanged}",
            result.is_ok()
        );
    }

    #[test]
    #[ignore = "assets privados; correction05 AUTO foreign identity before the terminal tick"]
    fn correction05_auto_captured_foreign_identity_is_rejected_before_the_terminal_tick() {
        check(true);
    }

    #[test]
    #[ignore = "assets privados; correction05 AUTO foreign identity before the terminal tick"]
    fn correction05_auto_pursuit_foreign_identity_is_rejected_before_the_terminal_tick() {
        check(false);
    }
}
