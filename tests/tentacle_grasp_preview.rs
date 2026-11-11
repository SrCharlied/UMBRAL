//! GT1-C: preview manual aislada (`GraspPreview`) sobre la ruta REAL del lanzador: World, superficie lucida,
//! ojo del vacio, Sol con ojo y percepcion -> `advance_perception_frame` -> `SpatialPursuit::step` ->
//! llegada -> B1 -> P1. Target PROPIO; ningun test heredado se edita. Assets privados opt-in (`#[ignore]`,
//! claves de BASELINE.json). El oraculo de la capa es una REPLICA independiente con las APIs del nucleo.
use umbral::{
    app::advance_perception_frame,
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
        tentacle_grasp_closure::*,
        tentacle_grasp_handoff::*,
        tentacle_grasp_preview::*,
        tentacle_grasp_transition::CONTACT_TOLERANCE_OBJ,
        tentacle_live_pursuit::{MAX_PURSUIT_DT, PursuitMode},
        tentacle_solar_game::TentacleCaptureRequest,
        tentacle_spatial_pursuit::SpatialPursuit,
        void_game::set_void_eye,
        watching_stars_game::render_game_frame_with,
    },
};

type V = [f64; 3];
const DT: f64 = 1.0 / 60.0;
const PRIVATE: &str = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE";

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
fn mats(w: &World) -> (Mat4, Mat4) {
    (
        w.ship.transform(SHIP_MODEL_YAW).matrix(),
        w.creature_render_transform().unwrap().matrix(),
    )
}
fn key4() -> OxygenKeys {
    OxygenKeys {
        preset: Some(0.05),
        ..OxygenKeys::default()
    }
}
const T: PreviewKeys = PreviewKeys {
    request: true,
    reset: false,
};
const X: PreviewKeys = PreviewKeys {
    request: false,
    reset: true,
};
const NONE: PreviewKeys = PreviewKeys {
    request: false,
    reset: false,
};

/// La escena del lanzador (lucida + vacio + Sol con ojo + percepcion), O2 inicial 100 %.
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
    inputs_debug: umbral::visual::tentacle_solar_game::SolarCaptureDebug,
) -> SpatialPursuit {
    let mut p = SpatialPursuit::new(inputs_debug);
    ok(p.bind(world).map(|_| ()), "bind real");
    ok(p.step(world, 0.0).map(|_| ()), "paso 0");
    p
}

/// Un cuadro de la ruta: World PRIMERO, despues la preview con el mismo dt.
fn step(
    g: &mut Game,
    pv: &mut GraspPreview,
    o2: OxygenKeys,
    input: ShipInput,
    keys: PreviewKeys,
) -> PreviewStatus {
    ok(
        advance_perception_frame(&mut g.world, &mut g.oxygen, o2, &input, DT),
        "cuadro del World",
    );
    ok(pv.frame(&g.world, DT, keys), "cuadro de la preview")
}

fn centroid(points: &[V], idx: &[usize]) -> Vec3 {
    let mut s = Vec3::ZERO;
    for &i in idx {
        s += Vec3::from_array(points[i]);
    }
    s * (1.0 / idx.len() as f64)
}

fn arm_of(pv: &GraspPreview) -> (Vec<V>, Vec<V>) {
    let (v, n) = pv.arm().expect("capa dibujada");
    (v.to_vec(), n.to_vec())
}

/// La esfera publicada contiene TODO el tramo publicado y el prefijo de la capa es el de la persecucion.
fn check_layer(pv: &GraspPreview, what: &str) {
    let o = pv.body_override().expect("capa");
    let (c, r) = o.extra_bounds_obj;
    let (nv, nn) = pv.pursuit().body_prefix().unwrap();
    let pc = pv.pursuit().composite().unwrap();
    assert_eq!(
        o.model.vertices.len(),
        pc.vertices.len(),
        "{what}: tamano de capa"
    );
    assert_eq!(o.model.faces.len(), pc.faces.len(), "{what}: caras de capa");
    assert!(
        o.model.vertices[..nv] == pc.vertices[..nv],
        "{what}: prefijo del cuerpo"
    );
    assert!(
        o.model.normals[..nn] == pc.normals[..nn],
        "{what}: prefijo de normales"
    );
    for p in &o.model.vertices[nv..] {
        let d = (Vec3::from_array(*p) - c).length();
        assert!(
            d <= r * (1.0 + 1e-12) + 1e-12,
            "{what}: vertice fuera de la esfera {d} > {r}"
        );
    }
    let (av, an) = pv.arm().unwrap();
    assert!(
        av == &o.model.vertices[nv..] && an == &o.model.normals[nn..],
        "{what}: arm() != capa"
    );
}

/// Llega hasta la captura con la solicitud armada en el cuadro 30 (antes de la llegada).
fn to_capture(g: &mut Game, pv: &mut GraspPreview) -> PreviewStatus {
    let s = step(g, pv, key4(), ShipInput::default(), NONE);
    assert_eq!(s.phase, PreviewPhase::Pursuit);
    for _ in 0..29 {
        step(g, pv, OxygenKeys::default(), ShipInput::default(), NONE);
    }
    let s = step(g, pv, OxygenKeys::default(), ShipInput::default(), T);
    assert!(s.armed, "T arma la solicitud");
    assert_ne!(
        s.pursuit.unwrap().step.mode,
        PursuitMode::Pursuing,
        "solicitud armada ANTES de la llegada (O2 aun alto)"
    );
    for _ in 0..3000 {
        let s = step(g, pv, OxygenKeys::default(), ShipInput::default(), NONE);
        if s.phase != PreviewPhase::Pursuit {
            return s;
        }
        assert!(s.armed, "la solicitud sigue armada hasta cumplirse");
    }
    panic!("la solicitud no se cumplio");
}

/// Replica independiente de B1/P1 con las APIs del nucleo (oraculo de la capa).
struct Replica<'c, 'p> {
    clip: &'c ClosureClip<'p>,
    ship: Mat4,
    body: Mat4,
    b1: Option<GraspHandoff<'c>>,
    p1: Option<GraspClosure<'c, 'p>>,
}

impl<'c, 'p> Replica<'c, 'p> {
    fn new(clip: &'c ClosureClip<'p>, reached: &ReachedPose, ship: Mat4, body: Mat4) -> Self {
        let b1 = ok(
            GraspHandoff::start(
                clip.adapter(),
                reached,
                &ship,
                &body,
                HANDOFF_CANDIDATE_DURATION_S,
            ),
            "replica B1",
        );
        Self {
            clip,
            ship,
            body,
            b1: Some(b1),
            p1: None,
        }
    }
    fn advance(&mut self, dt: f64) {
        if let Some(c) = &mut self.p1 {
            ok(
                c.advance(&self.ship, &self.body, dt).map(|_| ()),
                "replica P1",
            );
            return;
        }
        let h = self.b1.as_mut().unwrap();
        ok(
            h.advance(&self.ship, &self.body, dt).map(|_| ()),
            "replica B1",
        );
        if h.status().phase == HandoffPhase::Contact {
            self.p1 = Some(ok(GraspClosure::start(self.clip, h), "replica P1 start"));
        }
    }
    fn arm(&self) -> (Vec<V>, Vec<V>) {
        match (&self.p1, &self.b1) {
            (Some(c), _) => (c.vertices().to_vec(), c.normals().to_vec()),
            (None, Some(h)) => (h.vertices().to_vec(), h.normals().to_vec()),
            _ => unreachable!(),
        }
    }
}

/// Fixture comun: World del lanzador, persecucion enlazada (duena) y paquete de los mismos bytes.
macro_rules! fixture {
    ($g:ident, $pv:ident, $clip:ident, $inputs_package:ident) => {
        let mut $g = game();
        let inputs = ok(load_preview_inputs(&request()), "entradas");
        let $inputs_package = inputs.package;
        let pursuit = bound(&$g.world, inputs.debug);
        let rest = ok(arm_rest(&$g.world, &pursuit), "reposo");
        let $clip = ok(ClosureClip::new(&$inputs_package, &rest), "clip");
        #[allow(unused_mut)]
        let mut $pv = ok(GraspPreview::new(&$clip, pursuit), "preview");
    };
}

#[test]
#[ignore = "assets privados"]
fn g01_armed_before_arrival_progresses_to_grip_with_core_buffers() {
    fixture!(g, pv, clip, package);
    let s = to_capture(&mut g, &mut pv);
    // Captura en ESTE cuadro: llegada actual, nave quieta, matrices actuales.
    assert_eq!(s.phase, PreviewPhase::Handoff);
    assert!(s.events.contains(&PreviewEvent::Captured), "{:?}", s.events);
    assert!(!s.armed, "la solicitud se consume");
    let ps = s.pursuit.unwrap();
    assert!(ps.step.reached && ps.step.mode == PursuitMode::Pursuing && ps.shaft.is_some());
    assert_eq!(
        pv.pursuit().last(),
        Some(ps),
        "estado de persecucion de este cuadro"
    );
    assert!(s.matrices_stable);
    let (ship, body) = pv.captured_transforms().expect("matrices capturadas");
    let (cs, cb) = mats(&g.world);
    assert!(bits(&ship, &cs) && bits(&body, &cb), "matrices ACTUALES");
    let reached = pv.reached().expect("pose alcanzada").clone();
    let (nv, nn) = pv.pursuit().body_prefix().unwrap();
    let pc = pv.pursuit().composite().unwrap();
    assert!(reached.vertices() == &pc.vertices[nv..] && reached.normals() == &pc.normals[nn..]);
    let (av, an) = arm_of(&pv);
    assert!(
        av == reached.vertices() && an == reached.normals(),
        "primer cuadro B1 = brazo publicado (bits)"
    );
    check_layer(&pv, "captura");
    let mut rep = Replica::new(&clip, &reached, ship, body);
    let mut order = vec![PreviewPhase::Handoff];
    let mut time = g.world.time;
    let mut gripped_at = None;
    for i in 0..1200 {
        let s = step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        assert!(g.world.time > time, "el World avanza en CADA cuadro");
        time = g.world.time;
        rep.advance(DT);
        let (rv, rn) = rep.arm();
        let (av, an) = arm_of(&pv);
        assert!(
            av == rv && an == rn,
            "cuadro {i}: capa != buffers del nucleo"
        );
        check_layer(&pv, "episodio");
        if *order.last().unwrap() != s.phase {
            order.push(s.phase);
        }
        if s.phase == PreviewPhase::Gripped && gripped_at.is_none() {
            gripped_at = Some(i);
            assert!(s.events.contains(&PreviewEvent::Gripped));
            assert_eq!(s.closure.unwrap().frame, clip.grip_frame());
        }
        if gripped_at.is_some_and(|k| i > k + 60) {
            break;
        }
    }
    assert_eq!(
        order,
        vec![
            PreviewPhase::Handoff,
            PreviewPhase::Preparing,
            PreviewPhase::Closing,
            PreviewPhase::Gripped
        ]
    );
    // Hold y contacto medido sobre la MALLA FINAL publicada (independiente del status).
    let (av, _) = arm_of(&pv);
    let desired = Vec3::from_array(ok(
        clip.adapter().anchor().desired_obj(&ship, &body),
        "ancla",
    ));
    let err = (centroid(&av, clip.adapter().grip()) - desired).length();
    assert!(err <= CONTACT_TOLERANCE_OBJ, "contacto final {err}");
    let reported = pv
        .final_mesh_contact_error()
        .expect("error de contacto de la malla final");
    assert!(
        (reported - err).abs() <= 1e-12,
        "producto {reported} vs prueba {err}"
    );
    let before = arm_of(&pv);
    for _ in 0..30 {
        let s = step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        assert_eq!(s.phase, PreviewPhase::Gripped);
    }
    assert!(arm_of(&pv) == before, "hold determinista");
}

fn cancel_in(target: PreviewPhase) {
    fixture!(g, pv, clip, package);
    to_capture(&mut g, &mut pv);
    let mut guard = 0;
    while pv.status().phase != target {
        step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        guard += 1;
        assert!(guard < 1000, "no llego a {target:?}");
    }
    let last = arm_of(&pv);
    let last_status = pv.status();
    let before = g.world.ship;
    let cam = g.world.camera.position;
    let input = ShipInput {
        thrust: 1.0,
        yaw: 1.0,
        ..ShipInput::default()
    };
    let s = step(&mut g, &mut pv, OxygenKeys::default(), input, NONE);
    let moved = (g.world.ship.position - before.position).length() > 0.0
        || g.world.ship.orientation.m != before.orientation.m;
    assert!(moved, "{target:?}: el World aplico la entrada de la nave");
    assert_eq!(s.phase, PreviewPhase::Cancelled, "{target:?}");
    assert_eq!(s.cancel, Some(CancelReason::ShipOrBodyMoved));
    assert_eq!(s.stopped_in, Some(target));
    assert!(
        s.events
            .contains(&PreviewEvent::Cancelled(CancelReason::ShipOrBodyMoved))
    );
    assert!(
        arm_of(&pv) == last,
        "{target:?}: queda la ULTIMA malla publicada"
    );
    assert_eq!(s.handoff, last_status.handoff, "sin advance tras cancelar");
    assert_eq!(s.closure, last_status.closure, "sin advance tras cancelar");
    check_layer(&pv, "cancelada");
    // La nave sigue libre: mas cuadros con entrada la siguen moviendo; la observacion no cambia.
    for _ in 0..20 {
        let p0 = g.world.ship.position;
        let s = step(&mut g, &mut pv, OxygenKeys::default(), input, T);
        assert_eq!(s.phase, PreviewPhase::Cancelled);
        assert!(
            s.events.contains(&PreviewEvent::RequestIgnored),
            "T no reanuda"
        );
        assert!((g.world.ship.position - p0).length() > 0.0, "nave libre");
    }
    assert!(
        (g.world.camera.position - cam).length() > 0.0,
        "camara libre"
    );
    assert!(arm_of(&pv) == last);
}

#[test]
#[ignore = "assets privados"]
fn g02a_ship_input_during_handoff_cancels() {
    cancel_in(PreviewPhase::Handoff);
}
#[test]
#[ignore = "assets privados"]
fn g02b_ship_input_during_preparation_cancels() {
    cancel_in(PreviewPhase::Preparing);
}
#[test]
#[ignore = "assets privados"]
fn g02c_ship_input_during_closing_cancels() {
    cancel_in(PreviewPhase::Closing);
}

#[test]
#[ignore = "assets privados"]
fn g03_camera_only_does_not_cancel_and_reset_uses_current_world() {
    fixture!(g, pv, clip, package);
    to_capture(&mut g, &mut pv);
    while pv.status().phase != PreviewPhase::Preparing {
        step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
    }
    // Solo la camara (observacion): camara de diagnostico aparte y render; no cancela.
    for _ in 0..10 {
        ok(
            advance_perception_frame(
                &mut g.world,
                &mut g.oxygen,
                OxygenKeys::default(),
                &ShipInput::default(),
                DT,
            ),
            "World",
        );
        let saved = g.world.camera;
        g.world.camera.position += Vec3::new(30.0, 12.0, -20.0);
        let mut fb = Framebuffer::new(320, 180, SPACE_COLOR).unwrap();
        ok(
            render_game_frame_with(
                &g.world,
                &mut fb,
                Some(&g.game),
                false,
                pv.body_override().as_ref(),
            )
            .map(|_| ()),
            "render diagnostico",
        );
        let before = pv.status().closure.unwrap().elapsed_s;
        let s = ok(pv.frame(&g.world, DT, NONE), "preview");
        g.world.camera = saved;
        assert_eq!(
            s.phase,
            PreviewPhase::Preparing,
            "la camara sola no invalida"
        );
        assert!(s.closure.unwrap().elapsed_s > before, "P1 sigue avanzando");
    }
    // Mover la nave cancela; X reinicia con el World ACTUAL sin tocar nave ni usar la captura vieja.
    let turn = ShipInput {
        yaw: 1.0,
        pitch: 0.5,
        ..ShipInput::default()
    };
    for _ in 0..40 {
        step(&mut g, &mut pv, OxygenKeys::default(), turn, NONE);
    }
    assert_eq!(pv.status().phase, PreviewPhase::Cancelled);
    ok(
        advance_perception_frame(
            &mut g.world,
            &mut g.oxygen,
            OxygenKeys::default(),
            &ShipInput::default(),
            DT,
        ),
        "World",
    );
    let ship_before = g.world.ship;
    let s = ok(pv.frame(&g.world, DT, X), "X");
    assert!(s.events.contains(&PreviewEvent::Reset), "{:?}", s.events);
    assert_eq!(s.phase, PreviewPhase::Pursuit);
    assert!(!s.armed);
    assert!(
        g.world.ship.position == ship_before.position
            && g.world.ship.orientation.m == ship_before.orientation.m,
        "X no toca la nave"
    );
    assert!(
        pv.reached().is_none() && pv.captured_transforms().is_none(),
        "sin captura vieja"
    );
    // Diagnostico d03/d04: tras ESTE giro la camara y el cuerpo (seguimiento hacia la camara) quedan en un
    // ciclo de 2 matrices (1 ulp). La solicitud ESPERA (no capta con matrices que cambian) ...
    let s = step(
        &mut g,
        &mut pv,
        OxygenKeys::default(),
        ShipInput::default(),
        T,
    );
    assert!(s.armed);
    for _ in 0..1500 {
        let s = step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        assert_eq!(s.phase, PreviewPhase::Pursuit, "sin quietud no hay captura");
        assert!(s.armed);
    }
    assert!(
        pv.status().quiet_frames < QUIET_FRAMES,
        "cuerpo en ciclo: no quieto"
    );
    // ... hasta que el jugador usa un control REAL (R: vista FRONTAL, SolarKeys) y espera la quietud.
    ok(
        g.game.apply(
            &mut g.world,
            SolarKeys {
                reset: true,
                ..SolarKeys::default()
            },
        ),
        "R frontal",
    );
    let mut captured = false;
    for _ in 0..4000 {
        let s = step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        if s.phase == PreviewPhase::Handoff {
            assert!(s.matrices_stable && s.quiet_frames >= QUIET_FRAMES);
            assert!(s.events.contains(&PreviewEvent::Captured));
            captured = true;
            break;
        }
        assert_eq!(s.phase, PreviewPhase::Pursuit);
    }
    assert!(captured, "nueva captura tras quietud");
    // Captura FRESCA: matrices del World actual y brazo publicado por la persecucion en ESTE cuadro.
    let (ship, body) = pv.captured_transforms().unwrap();
    let (cs, cb) = mats(&g.world);
    assert!(bits(&ship, &cs) && bits(&body, &cb), "matrices actuales");
    let (nv, nn) = pv.pursuit().body_prefix().unwrap();
    let pc = pv.pursuit().composite().unwrap();
    let r = pv.reached().unwrap();
    assert!(
        r.vertices() == &pc.vertices[nv..] && r.normals() == &pc.normals[nn..],
        "llegada actual"
    );
    assert_eq!(pv.status().pursuit, pv.pursuit().last());
    check_layer(&pv, "segunda captura");
}

#[test]
#[ignore = "assets privados"]
fn g05_without_request_frames_equal_previous_pursuit_route() {
    fixture!(g, pv, clip, package);
    // Ruta anterior: otra persecucion enlazada al MISMO World, mismo dt.
    let mut plain = bound(
        &g.world,
        ok(load_preview_inputs(&request()), "entradas 2").debug,
    );
    let mut compared = 0;
    for i in 0..420 {
        let o2 = if i == 30 {
            key4()
        } else {
            OxygenKeys::default()
        };
        ok(
            advance_perception_frame(&mut g.world, &mut g.oxygen, o2, &ShipInput::default(), DT),
            "World",
        );
        let a = ok(plain.step(&g.world, DT), "ruta anterior");
        let s = ok(pv.frame(&g.world, DT, NONE), "preview");
        assert_eq!(s.phase, PreviewPhase::Pursuit);
        assert_eq!(
            s.pursuit,
            Some(a),
            "cuadro {i}: mismo estado de persecucion"
        );
        assert_eq!(
            pv.body_override().is_some(),
            plain.body_override().is_some()
        );
        if i % 60 == 0 || i == 419 {
            let mut fa = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
            let mut fb = Framebuffer::new(640, 360, SPACE_COLOR).unwrap();
            let ra = ok(
                render_game_frame_with(
                    &g.world,
                    &mut fa,
                    Some(&g.game),
                    true,
                    plain.body_override().as_ref(),
                ),
                "render anterior",
            );
            let rb = ok(
                render_game_frame_with(
                    &g.world,
                    &mut fb,
                    Some(&g.game),
                    true,
                    pv.body_override().as_ref(),
                ),
                "render preview",
            );
            assert_eq!(ra, rb, "cuadro {i}: estadisticas");
            assert!(fa.pixels == fb.pixels, "cuadro {i}: framebuffer");
            compared += 1;
        }
    }
    assert!(
        plain.body_override().is_some(),
        "al final la persecucion dibuja (O2 5 %)"
    );
    assert_eq!(compared, 8);
}

#[test]
#[ignore = "assets privados"]
fn g06_invalid_dt_is_atomic_and_foreign_rest_is_rejected() {
    fixture!(g, pv, clip, package);
    to_capture(&mut g, &mut pv);
    while pv.status().phase != PreviewPhase::Preparing {
        step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
    }
    for _ in 0..5 {
        step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
    }
    let (s0, a0) = (pv.status(), arm_of(&pv));
    for dt in [f64::NAN, f64::INFINITY, -DT, MAX_PURSUIT_DT * 2.0] {
        assert!(pv.frame(&g.world, dt, T).is_err(), "dt {dt}");
        assert_eq!(pv.status(), s0, "dt {dt}: estado intacto");
        assert!(arm_of(&pv) == a0, "dt {dt}: capa intacta");
    }
    // Reposo ajeno (1e-9 en un vertice: el clip lo acepta, pero no es el rig enlazado).
    let other = ok(load_preview_inputs(&request()), "entradas 2");
    let pursuit = bound(&g.world, other.debug);
    let mut rest = ok(arm_rest(&g.world, &pursuit), "reposo");
    rest[0][0] += 1e-9;
    let foreign = ok(ClosureClip::new(&package, &rest), "clip ajeno");
    assert!(
        GraspPreview::new(&foreign, pursuit).is_err(),
        "rig ajeno rechazado"
    );
}

#[test]
#[ignore = "assets privados"]
fn g07_oxygen_above_engage_during_episode_cancels() {
    fixture!(g, pv, clip, package);
    to_capture(&mut g, &mut pv);
    while pv.status().phase != PreviewPhase::Preparing {
        step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
    }
    let key1 = OxygenKeys {
        preset: Some(1.0),
        ..OxygenKeys::default()
    };
    let mut s = step(&mut g, &mut pv, key1, ShipInput::default(), NONE);
    let mut n = 0;
    while s.phase == PreviewPhase::Preparing {
        assert!(
            g.world.debug_oxygen01 <= 0.15,
            "mientras progresa el O2 sigue <= 15 %"
        );
        s = step(
            &mut g,
            &mut pv,
            OxygenKeys::default(),
            ShipInput::default(),
            NONE,
        );
        n += 1;
        assert!(n < 200);
    }
    assert_eq!(s.phase, PreviewPhase::Cancelled);
    assert_eq!(s.cancel, Some(CancelReason::OxygenAboveEngage));
    assert!(g.world.debug_oxygen01 > 0.15);
}

// Parent C-R1 regression: invalidating the render route must reject BEFORE any episode mutation.
#[test]
#[ignore = "assets privados; parent lifecycle probe"]
fn parent_c_r1_disabled_solar_route_rejects_atomically_in_every_episode_phase() {
    for target in [
        PreviewPhase::Preparing,
        PreviewPhase::Handoff,
        PreviewPhase::Closing,
        PreviewPhase::Gripped,
    ] {
        fixture!(g, pv, clip, package);
        to_capture(&mut g, &mut pv);
        for _ in 0..1500 {
            let status = pv.status();
            let prepared_enough =
                target != PreviewPhase::Preparing || status.closure.is_some_and(|s| s.frame >= 60);
            if status.phase == target && prepared_enough {
                break;
            }
            step(
                &mut g,
                &mut pv,
                OxygenKeys::default(),
                ShipInput::default(),
                NONE,
            );
        }
        assert_eq!(pv.status().phase, target, "valid matching control");
        check_layer(&pv, "valid before invalidation");
        let before_status = pv.status();
        let before_arm = arm_of(&pv);
        let before_bounds = pv.body_override().unwrap().extra_bounds_obj;
        let before_pursuit = pv.pursuit().last();
        let before_capture = pv.captured_transforms();
        let before_matrices = mats(&g.world);
        let before_oxygen = g.world.debug_oxygen01;
        g.world.disable_solar_reveal_preview();
        let after_matrices = mats(&g.world);
        assert!(
            bits(&before_matrices.0, &after_matrices.0)
                && bits(&before_matrices.1, &after_matrices.1),
            "route invalidation must NOT change captured transforms"
        );
        assert_eq!(before_oxygen.to_bits(), g.world.debug_oxygen01.to_bits());
        let result = pv.frame(&g.world, 1.0 / 24.0, NONE);
        let unchanged_status = pv.status() == before_status;
        let unchanged_arm = arm_of(&pv) == before_arm;
        let unchanged_bounds = pv.body_override().unwrap().extra_bounds_obj == before_bounds;
        let unchanged_pursuit = pv.pursuit().last() == before_pursuit;
        let unchanged_capture = pv.captured_transforms() == before_capture;
        println!(
            "PARENT_C_R1 phase {:?} rejected={} status={} arm={} bounds={} pursuit={} capture={}",
            target,
            result.is_err(),
            unchanged_status,
            unchanged_arm,
            unchanged_bounds,
            unchanged_pursuit,
            unchanged_capture
        );
        assert!(
            result.is_err(),
            "C-R1: incompatible World render route was accepted during {:?}; state_changed={}",
            target,
            !unchanged_status
        );
        assert!(unchanged_status && unchanged_arm && unchanged_bounds);
        assert!(unchanged_pursuit && unchanged_capture);
    }
}

// C-R1 additional public lifecycle cases. The entire original + parent test remains a byte prefix.
fn fix01_lifecycle_case(kind: &str) {
    use umbral::renderer::texture::Texture;
    use umbral::visual::staged_creature_reveal::{
        CREATURE_EYE_REGION, EyeRegion, StagedRevealSettings,
    };
    for target in [
        PreviewPhase::Handoff,
        PreviewPhase::Preparing,
        PreviewPhase::Closing,
        PreviewPhase::Gripped,
    ] {
        fixture!(g, pv, clip, package);
        to_capture(&mut g, &mut pv);
        for _ in 0..1500 {
            let s = pv.status();
            if s.phase == target
                && (target != PreviewPhase::Preparing || s.closure.unwrap().frame >= 60)
            {
                break;
            }
            step(
                &mut g,
                &mut pv,
                OxygenKeys::default(),
                ShipInput::default(),
                NONE,
            );
        }
        assert_eq!(pv.status().phase, target, "legal matching control");
        // A real healthy frame is accepted before invalidating; not a fixture born invalid.
        ok(pv.frame(&g.world, 0.0, NONE), "matching World accepted");
        let status = pv.status();
        let arm = arm_of(&pv);
        let bounds = pv.body_override().unwrap().extra_bounds_obj;
        let pursuit = pv.pursuit().last();
        let capture = pv.captured_transforms();
        let reached = pv.reached().unwrap().clone();
        match kind {
            "model" => {
                let placement = g.world.creature().unwrap().placement();
                let mut model = load_model(&env("UMBRAL_CAPTURE_CREATURE_OBJ")).unwrap();
                let counts = (model.vertices.len(), model.faces.len());
                let old_bounds = (model.bounds_center, model.bounds_radius);
                // Full identity, not topology, bounds, or just the arm pose: UV-only source edit.
                model.uvs[0][0] += 1e-9;
                let texture = Texture::load_png(&env("UMBRAL_CREATURE_TEXTURE")).unwrap();
                ok(
                    g.world
                        .set_creature(StaticCreature::new(model, texture, placement).unwrap()),
                    "public model replacement",
                );
                let current = g.world.creature().unwrap().model();
                assert_eq!(counts, (current.vertices.len(), current.faces.len()));
                assert_eq!(old_bounds, (current.bounds_center, current.bounds_radius));
            }
            "partition" => {
                let old = g.world.staged_creature_parts().unwrap().body().faces.len();
                let roi = EyeRegion::new(CREATURE_EYE_REGION.center(), 0.21).unwrap();
                ok(
                    g.world
                        .enable_staged_creature_reveal(StagedRevealSettings::default(), roi),
                    "public partition replacement",
                );
                assert_ne!(
                    old,
                    g.world.staged_creature_parts().unwrap().body().faces.len(),
                    "actually changed partition"
                );
            }
            "sun" => {
                // Public system lifecycle: solar route remains enabled, but cached Sun no longer exists.
                g.world.system.bodies.clear();
                assert!(g.world.solar_reveal_alphas().is_some());
            }
            _ => unreachable!(),
        }
        for keys in [
            NONE,
            T,
            X,
            PreviewKeys {
                request: true,
                reset: true,
            },
        ] {
            let result = pv.frame(&g.world, 1.0 / 24.0, keys);
            println!(
                "FIX01 {kind} {target:?} keys={keys:?} rejected={}",
                result.is_err()
            );
            assert!(
                result.is_err(),
                "C-R1 {kind}: incompatible World accepted during {target:?}"
            );
            assert_eq!(pv.status(), status, "full status atomic");
            assert!(arm_of(&pv) == arm, "arm atomic");
            assert_eq!(
                pv.body_override().unwrap().extra_bounds_obj,
                bounds,
                "bounds atomic"
            );
            assert_eq!(pv.pursuit().last(), pursuit, "pursuit atomic");
            assert_eq!(pv.captured_transforms(), capture, "capture atomic");
            let r = pv.reached().unwrap();
            assert!(
                r.vertices() == reached.vertices() && r.normals() == reached.normals(),
                "reached pose atomic"
            );
        }
    }
}

#[test]
#[ignore = "assets privados"]
fn fix01_model_lifecycle_rejects_atomically() {
    fix01_lifecycle_case("model");
}

#[test]
#[ignore = "assets privados"]
fn fix01_partition_lifecycle_rejects_atomically() {
    fix01_lifecycle_case("partition");
}

#[test]
#[ignore = "assets privados"]
fn fix01_sun_lifecycle_rejects_atomically() {
    fix01_lifecycle_case("sun");
}
