//! UMB-311 PERSECUCION VIVA del tentaculo (prototipo opt-in `--tentacle-live-pursuit`, ruta solar).
//! Ver `docs/tentacle-live-pursuit.md`. NO es captura: no hay agarre, arrastre, bloqueo de input,
//! muerte, audio, apagon, cooldown ni colisiones, y nunca mueve la nave.
//!
//! - **Controlador puro** (`PursuitController`): conserva la punta en MUNDO y su rapidez; en cada paso
//!   lee la posicion ACTUAL de la nave, el anclaje del brazo con el transform ACTUAL de la criatura, el
//!   O2 y `dt`. Embiste en recta o, si la recta entra en la bola minima (de exactitud de la malla),
//!   rodea el anclaje en polares; rapidez limitada, aceleracion limitada salvo el frenado inmediato
//!   de llegada seleccionado por Charlie; nunca sobrepasa el objetivo.
//! - **Deformacion** (`ShaftRig`): la topologia aprobada 4801/9585 como dato. Proximal y raiz
//!   INMOVILES en las posiciones exactas del Meshy; tramo procedural = codo circular + recta
//!   (camino de Dubins en el plano de la tangente del lazo) con anillos del lazo aprobado; distal y
//!   espiral RIGIDOS, girados con la tangente final (misma anatomia, UV y material).
//! - **Composicion** (`LivePursuit`): reutiliza `SolarCaptureDebug` (carga, identidad y `bind` sin
//!   debilitar) y escribe el tramo en una COPIA de su capa `Body`. En reposo devuelve `None`: el
//!   cuadro es exactamente el de siempre.
//!
//! Valores numericos (rapidez, aceleracion, codo, alcance): AJUSTES CANDIDATOS, no reglas aprobadas.
use crate::{
    assets::obj::{Face, Model},
    math::Vec3,
    scene::world::{SolarBodyOverride, World},
    visual::{
        tentacle_growth_cpu::{
            CornerSource, GrowthData, GrowthVertex, NUMERIC_WORLD_LIMIT, Part, Side, VertexSource,
        },
        tentacle_solar_game::{
            BindReport, SolarCaptureDebug, TentacleCaptureFlags, TentacleCaptureRequest,
        },
    },
};
use std::path::PathBuf;

/// O2 (fraccion) a partir del cual, hacia abajo e inclusive, empieza la persecucion.
pub const PURSUIT_ENGAGE_OXYGEN: f64 = 0.15;
/// Rapidez maxima CANDIDATA de la punta (unidades de mundo por segundo) en O2 15 %, 10 % y 5 %;
/// lineal entre ellos; por debajo de 5 % (incluido 0 %) se sostiene la de 5 %.
pub const PURSUIT_SPEEDS: [(f64, f64); 3] = [(0.15, 12.0), (0.10, 24.0), (0.05, 48.0)];
/// Aceleracion maxima CANDIDATA de la punta (u/s^2).
pub const PURSUIT_ACCELERATION: f64 = 48.0;
/// Rapidez CANDIDATA de recogida al recuperar O2 por encima de 15 % (u/s).
pub const PURSUIT_RETRACT_SPEED: f64 = 24.0;
/// `dt` maximo aceptado por paso (s); el juego pasa el `dt` limitado de la fisica (0.1 s).
pub const MAX_PURSUIT_DT: f64 = 1.0;
/// Radio CANDIDATO del codo en radios de contorno del lazo (>= 1 evita que los anillos se crucen).
pub const BEND_RADIUS_CONTOURS: f64 = 2.0;
/// Alcance maximo CANDIDATO de la punta desde el centro del lazo, en unidades OBJ de la criatura.
pub const MAX_REACH_OBJ: f64 = 12.0;
/// Histeresis CANDIDATA del lado del codo (rad): con el objetivo detras se sigue girando por el
/// mismo lado hasta 180 + 30 grados antes de cambiar.
pub const SIDE_HYSTERESIS: f64 = 30.0 * std::f64::consts::PI / 180.0;

/// Rapidez maxima de la punta para el O2 `oxygen01`: `None` por encima de 15 % (sin persecucion).
pub fn pursuit_speed(oxygen01: f64) -> Result<Option<f64>, String> {
    if !(oxygen01.is_finite() && (0.0..=1.0).contains(&oxygen01)) {
        return Err(format!("O2 invalido (finito en [0, 1]): {oxygen01}"));
    }
    if oxygen01 > PURSUIT_ENGAGE_OXYGEN {
        return Ok(None);
    }
    let [(o15, v15), (o10, v10), (o5, v5)] = PURSUIT_SPEEDS;
    Ok(Some(if oxygen01 >= o10 {
        v15 + (v10 - v15) * (o15 - oxygen01) / (o15 - o10)
    } else if oxygen01 >= o5 {
        v10 + (v5 - v10) * (o10 - oxygen01) / (o10 - o5)
    } else {
        v5
    }))
}

/// Modo del controlador.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PursuitMode {
    /// Reposo: la punta es la de reposo y el cuadro es el de siempre.
    Idle,
    /// O2 <= 15 %: persigue la posicion ACTUAL de la nave.
    Pursuing,
    /// O2 > 15 % con la punta fuera del reposo: vuelve al reposo sin saltos.
    Retracting,
}

/// Entradas de un paso (todas en MUNDO salvo `oxygen01` y `dt`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PursuitInput {
    pub ship: Vec3,
    /// Centro del lazo proximal con el transform ACTUAL de la criatura.
    pub anchor: Vec3,
    /// Punta de reposo con el transform ACTUAL de la criatura.
    pub rest_tip: Vec3,
    pub oxygen01: f64,
    pub dt: f64,
    /// Radio minimo de la punta alrededor del anclaje (bola que se rodea por fuera).
    pub min_radius: f64,
    /// Alcance maximo de la punta alrededor del anclaje.
    pub max_radius: f64,
}

/// Lo que hizo un paso (diagnostico).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PursuitStepReport {
    pub mode: PursuitMode,
    /// Objetivo efectivo (nave o reposo, recortado al alcance y a la bola minima).
    pub target: Vec3,
    /// La nave estaba fuera del alcance o dentro de la bola minima (objetivo recortado).
    pub target_clamped: bool,
    /// Rapidez maxima del paso segun O2/modo (u/s).
    pub max_speed: f64,
    /// Distancia recorrida por la punta en el paso (u).
    pub moved: f64,
    /// La punta alcanzo el objetivo efectivo en este paso.
    pub reached: bool,
    /// La punta quedo dentro de la bola minima por el movimiento del cuerpo y se proyecto.
    pub projected: bool,
}

/// Controlador puro de la punta: posicion en mundo, rapidez escalar y modo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PursuitController {
    tip: Vec3,
    speed: f64,
    mode: PursuitMode,
    axis: Option<Vec3>,
}

impl PursuitController {
    /// En reposo en `rest_tip` (finita y dentro del dominio numerico).
    pub fn at_rest(rest_tip: Vec3) -> Result<Self, String> {
        if !in_domain(rest_tip) {
            return Err(format!(
                "punta de reposo fuera del dominio numerico: {rest_tip:?}"
            ));
        }
        Ok(Self {
            tip: rest_tip,
            speed: 0.0,
            mode: PursuitMode::Idle,
            axis: None,
        })
    }
    pub fn tip(&self) -> Vec3 {
        self.tip
    }
    pub fn speed(&self) -> f64 {
        self.speed
    }
    pub fn mode(&self) -> PursuitMode {
        self.mode
    }
    /// Un paso. Valida TODO antes de cambiar nada; con `Err` el estado no cambia.
    ///
    /// - O2 <= 15 %: persigue la nave (rapidez de `pursuit_speed`); O2 > 15 %: vuelve a la punta de
    ///   reposo a `PURSUIT_RETRACT_SPEED` y, al llegar EXACTO, pasa a reposo (rapidez 0).
    /// - Embestida RECTA si el segmento punta -> objetivo no entra en la bola `min_radius`; si entra,
    ///   camino polar alrededor de `anchor` (gira por el circulo maximo y cambia el radio a la vez). El
    ///   recorrido del paso es la longitud EXACTA del camino, <= rapidez * `dt`, sin sobrepasar.
    /// - La punta de reposo puede estar DENTRO de la bola (bola de exactitud de la malla): sale y vuelve
    ///   por el eje de reposo; al empezar desde el reposo parte de la punta de reposo ACTUAL.
    /// - Rapidez escalar con aceleracion limitada y frenado previsto `sqrt(2 a d)`. Excepcion
    ///   expresa de llegada: no sobrepasar tiene prioridad sobre la cota de desaceleracion; la
    ///   rapidez informada al alcanzar el objetivo es el avance real del paso / dt.
    /// - `dt` 0: nada cambia.
    pub fn step(&mut self, input: &PursuitInput) -> Result<PursuitStepReport, String> {
        let i = *input;
        for (v, name) in [
            (i.ship, "nave"),
            (i.anchor, "anclaje"),
            (i.rest_tip, "punta de reposo"),
        ] {
            if !in_domain(v) {
                return Err(format!("{name} fuera del dominio numerico: {v:?}"));
            }
        }
        if !(i.dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&i.dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {}",
                i.dt
            ));
        }
        if !(i.min_radius.is_finite()
            && i.min_radius > 0.0
            && i.max_radius.is_finite()
            && i.max_radius >= i.min_radius
            && i.max_radius <= NUMERIC_WORLD_LIMIT)
        {
            return Err(format!(
                "radios invalidos: minimo {}, maximo {}",
                i.min_radius, i.max_radius
            ));
        }
        let cap = pursuit_speed(i.oxygen01)?;
        let (mode, goal, max_speed) = match (cap, self.mode) {
            (Some(v), _) => (PursuitMode::Pursuing, i.ship, v),
            (None, PursuitMode::Idle) => (PursuitMode::Idle, i.rest_tip, 0.0),
            (None, _) => (PursuitMode::Retracting, i.rest_tip, PURSUIT_RETRACT_SPEED),
        };
        let mut report = PursuitStepReport {
            mode: self.mode,
            target: goal,
            target_clamped: false,
            max_speed,
            moved: 0.0,
            reached: false,
            projected: false,
        };
        if i.dt == 0.0 {
            return Ok(report);
        }
        let mut next = *self;
        next.mode = mode;
        if mode == PursuitMode::Idle {
            // Reposo: la punta acompana al cuerpo (punta de reposo ACTUAL); no persigue.
            next.tip = i.rest_tip;
            next.speed = 0.0;
            next.axis = None;
            report.mode = mode;
            report.moved = (next.tip - self.tip).length();
            report.reached = true;
            *self = next;
            return Ok(report);
        }
        // Al empezar desde el reposo se parte de la punta de reposo ACTUAL (el cuerpo pudo girar desde
        // el ultimo cuadro; la del cuadro anterior podria quedar dentro de la bola de la malla).
        let current = if self.mode == PursuitMode::Idle {
            i.rest_tip
        } else {
            self.tip
        };
        // Posicion polar actual.
        let rel = current - i.anchor;
        let rest_rel = i.rest_tip - i.anchor;
        let rest_r = rest_rel.length();
        let rest_dir = rest_rel * (1.0 / rest_r);
        let r = rel.length();
        let w = if r > 0.0 { rel * (1.0 / r) } else { rest_dir };
        if !(w.is_finite() && rest_dir.is_finite() && (w.length() - 1.0).abs() < 1e-6) {
            return Err("direccion de la punta o del reposo indefinida".into());
        }
        // Bola minima (de exactitud de la malla): la punta de reposo puede estar DENTRO. Al perseguir desde
        // dentro se sale RADIALMENTE hasta el borde como tramo previo del MISMO paso y lo que sobra sigue
        // hacia la nave (sin perder rapidez); al recoger se entra por la puerta del eje de reposo (borde
        // de la bola sobre ese eje) hasta el reposo EXACTO.
        let inside = r < i.min_radius * (1.0 - 1e-12);
        report.projected = inside;
        let gate = i.anchor + rest_dir * i.min_radius;
        let rest_inside = rest_r < i.min_radius * (1.0 - 1e-12);
        let exiting = mode == PursuitMode::Pursuing && inside;
        let (origin, r0) = if exiting {
            (i.anchor + w * i.min_radius, i.min_radius)
        } else {
            (current, r)
        };
        let pre = (origin - current).length();
        let g = goal - i.anchor;
        let gl = g.length();
        let wg = if gl > 0.0 { g * (1.0 / gl) } else { w };
        let (rg, target, radial) = match mode {
            PursuitMode::Pursuing => {
                let rg = gl.clamp(i.min_radius, i.max_radius);
                if rg == gl {
                    (gl, goal, false)
                } else {
                    report.target_clamped = true;
                    (rg, i.anchor + wg * rg, false)
                }
            }
            _ if rest_inside && inside => (rest_r, i.rest_tip, true),
            _ if rest_inside => (i.min_radius, gate, false),
            _ => (gl, goal, false),
        };
        // Solo la llegada al reposo EXACTO termina la recogida (no la llegada a la puerta).
        let final_target = target == i.rest_tip;
        let wg = if radial {
            w
        } else {
            let to = target - i.anchor;
            let l = to.length();
            if l > 0.0 { to * (1.0 / l) } else { wg }
        };
        report.target = target;
        let cross = w.cross(wg);
        let sin = cross.length();
        let cos = w.dot(wg);
        let theta = sin.atan2(cos);
        let axis = if sin > 1e-12 {
            cross * (1.0 / sin)
        } else if cos < 0.0 {
            // Antipodal: el eje anterior (proyectado) o uno determinista.
            let previous = self
                .axis
                .map(|a| a - w * a.dot(w))
                .filter(|a| a.length() > 1e-6);
            match previous {
                Some(a) => a.normalized(),
                None => {
                    let e = if w.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
                    w.cross(e).normalized()
                }
            }
        } else {
            self.axis.unwrap_or(Vec3::Z)
        };
        // Embestida RECTA si el segmento origen -> objetivo no entra en la bola minima; si no, camino
        // polar (rodea la bola). Longitud EXACTA de cada camino.
        let straight = radial || segment_clear(origin - i.anchor, target - i.anchor, i.min_radius);
        let length = |f: f64| {
            if straight {
                (target - origin).length() * f
            } else {
                polar_length(r0, rg - r0, theta, f)
            }
        };
        let path = length(1.0);
        if !(path.is_finite() && pre.is_finite() && axis.is_finite()) {
            return Err("recorrido no finito".into());
        }
        // Frenado de llegada con la distancia al objetivo FINAL (la puerta es intermedia).
        let to_gate = mode == PursuitMode::Retracting && rest_inside && !inside;
        let remaining = pre
            + path
            + if to_gate {
                (i.rest_tip - gate).length()
            } else {
                0.0
            };
        let arrive = (2.0 * PURSUIT_ACCELERATION * remaining).sqrt();
        let desired = max_speed.min(arrive);
        let accel = PURSUIT_ACCELERATION * i.dt;
        let speed = (self.speed + (desired - self.speed).clamp(-accel, accel)).max(0.0);
        let full = speed * i.dt;
        if pre > 0.0 && full <= pre {
            // Todo el paso en la salida radial.
            next.tip = current + (origin - current) * (full / pre);
            next.speed = speed;
        } else {
            let budget = full - pre;
            if path <= budget {
                next.tip = target;
                next.speed = speed.min((pre + path) / i.dt);
                report.reached = mode == PursuitMode::Pursuing || final_target;
                if to_gate {
                    // Puerta alcanzada: lo que sobra del paso sigue por el EJE de reposo (la puerta se
                    // mueve con el cuerpo; asi nunca se persigue sin entrar).
                    let inward = i.rest_tip - gate;
                    let left = budget - path;
                    if left >= inward.length() {
                        next.tip = i.rest_tip;
                        report.reached = true;
                        next.mode = PursuitMode::Idle;
                        next.speed = 0.0;
                        next.axis = None;
                    } else {
                        next.tip = gate + inward * (left / inward.length());
                        next.speed = speed;
                    }
                }
                if mode == PursuitMode::Retracting && final_target {
                    next.mode = PursuitMode::Idle;
                    next.speed = 0.0;
                    next.axis = None;
                }
            } else if straight {
                next.tip = origin + (target - origin) * (budget / path);
                next.speed = speed;
            } else {
                // Fraccion con longitud de camino = presupuesto (biseccion; la longitud crece con f).
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..60 {
                    let mid = 0.5 * (lo + hi);
                    if length(mid) <= budget {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let f = lo;
                let a = f * theta;
                let (sa, ca) = a.sin_cos();
                let turned =
                    (w * ca + axis.cross(w) * sa + axis * (axis.dot(w) * (1.0 - ca))).normalized();
                next.tip = i.anchor + turned * (r0 + f * (rg - r0));
                next.speed = speed;
                if theta > 0.0 {
                    next.axis = Some(axis);
                }
            }
        }
        if !in_domain(next.tip) {
            return Err(format!("punta no finita: {:?}", next.tip));
        }
        report.mode = next.mode;
        // Recorrido propio del paso (sin el arrastre del cuerpo sobre la punta de reposo).
        report.moved = (next.tip - current).length();
        *self = next;
        Ok(report)
    }
}

/// El segmento `a -> b` (relativos al anclaje) no entra en la bola abierta de radio `radius`.
fn segment_clear(a: Vec3, b: Vec3, radius: f64) -> bool {
    let d = b - a;
    let dd = d.dot(d);
    let u = if dd > 0.0 {
        (-a.dot(d) / dd).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + d * u).length() >= radius * (1.0 - 1e-12)
}

/// Longitud del camino polar `r(u) = r0 + u dr`, `angulo(u) = u theta` para `u` en `[0, f]`:
/// integral de `sqrt(dr^2 + (r(u) theta)^2)` en forma cerrada.
fn polar_length(r0: f64, dr: f64, theta: f64, f: f64) -> f64 {
    if theta == 0.0 {
        return (dr * f).abs();
    }
    if dr == 0.0 {
        return r0 * theta * f;
    }
    let c = dr.abs();
    // Primitiva de sqrt(c^2 + theta^2 x^2) en x.
    let prim = |x: f64| {
        let q = (c * c + theta * theta * x * x).sqrt();
        0.5 * x * q + c * c / (2.0 * theta) * (theta * x / c).asinh()
    };
    ((prim(r0 + f * dr) - prim(r0)) / dr).abs()
}

/// Lazo proximal de reposo (OBJ): centro, tangente unitaria y radio de contorno.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestLoop {
    pub center: [f64; 3],
    pub tangent: [f64; 3],
    pub contour_radius: f64,
}

/// Geometria del tramo de un cuadro (OBJ).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaftPose {
    /// Punta pedida (OBJ).
    pub tip_obj: [f64; 3],
    /// Giro total del codo (rad, 0..2pi).
    pub bend: f64,
    /// Giro aplicado al distal (rad).
    pub end_rotation: f64,
    /// Longitud del tramo procedural (OBJ).
    pub shaft_length: f64,
    /// Direccion (unitaria, perpendicular a la tangente) hacia la que gira el codo.
    pub bend_side: [f64; 3],
    /// El lado del codo cambio en este cuadro (salto declarado, fuera de la histeresis).
    pub side_switched: bool,
    /// Centro del lazo distal (fin del tramo) y punta axial resultante `centro + a * tangente final`.
    pub distal_center: [f64; 3],
    pub axial_tip: [f64; 3],
    /// Longitud del tramo recto del camino (OBJ).
    pub line_length: f64,
}

/// Datos de reposo de la deformacion, cacheados una vez en `bind`.
pub struct ShaftRig {
    rest_vertices: Vec<[f64; 3]>,
    rest_normals: Vec<[f64; 3]>,
    rest: RestLoop,
    bend_radius: f64,
    tip_offset: f64,
    /// Clase de cada vertice de la plantilla.
    kinds: Vec<VertexKind>,
    /// Giro de cada esquina (3 por cara): fija (proximal), distal o del anillo `j`.
    corner_ring: Vec<CornerKind>,
    /// Desplazamiento de cada posicion k del lazo P respecto a su centro (reposo).
    loop_offsets: Vec<[f64; 3]>,
    rings: usize,
}

/// Clase de un vertice de la plantilla (expuesta solo dentro del crate a otras familias de deformacion).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VertexKind {
    Proximal,
    Distal,
    Ring { j: usize, k: usize },
}

/// Giro de una esquina de la plantilla (expuesta solo dentro del crate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CornerKind {
    Fixed,
    Distal,
    Ring(usize),
}

type V3 = [f64; 3];
/// Vista de solo lectura de la clasificacion de la plantilla (ver `ShaftRig::layout`).
pub(crate) type RigLayout<'a> = (&'a [VertexKind], &'a [CornerKind], &'a [[f64; 3]], usize);

fn vadd(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn vsub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn vmul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn vdot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn vcross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn vlen(a: V3) -> f64 {
    vdot(a, a).sqrt()
}
fn finite(a: V3) -> bool {
    a.iter().all(|c| c.is_finite())
}
/// Rodrigues: gira `v` un angulo `angle` alrededor del eje unitario `axis`.
fn rotate(v: V3, axis: V3, angle: f64) -> V3 {
    let (s, c) = angle.sin_cos();
    vadd(
        vadd(vmul(v, c), vmul(vcross(axis, v), s)),
        vmul(axis, vdot(axis, v) * (1.0 - c)),
    )
}
/// Perpendicular unitaria determinista a `t` (unitaria).
fn perpendicular(t: V3) -> V3 {
    let e = if t[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let p = vcross(t, e);
    vmul(p, 1.0 / vlen(p))
}

/// Camino codo + recta resuelto para una punta (todo en OBJ, plano `t`/`b`).
#[derive(Clone, Copy, Debug)]
struct Dubins {
    axis: V3,
    bend: f64,
    line: f64,
}

impl Dubins {
    /// `None` si la punta esta dentro del circulo de giro (no ocurre con `r >= a >= 2 rho`).
    fn solve(d: V3, t: V3, b: V3, rho: f64) -> Option<Self> {
        let x = vdot(d, t);
        let qy = vdot(d, b) - rho;
        let dc = x.hypot(qy);
        if dc.is_nan() || dc < rho {
            return None;
        }
        let line = (dc * dc - rho * rho).max(0.0).sqrt();
        let mut bend = qy.atan2(x) + rho.atan2(line);
        if bend < 0.0 {
            bend = if bend > -1e-12 {
                0.0
            } else {
                bend + std::f64::consts::TAU
            };
        }
        Some(Self {
            axis: vcross(t, b),
            bend,
            line,
        })
    }
}

impl ShaftRig {
    /// Reposo EXACTO desde las posiciones del brazo `arm_rest` (indice de vertice de la captura; en el
    /// juego, las posiciones del Meshy via la correspondencia) y la topologia aprobada de `data`.
    pub fn new(data: &GrowthData, arm_rest: &[[f64; 3]]) -> Result<Self, String> {
        let topo = data.topology();
        let n_vertices = topo.vertices().len();
        let mut rest_vertices = vec![[0.0; 3]; n_vertices];
        let mut kinds = vec![VertexKind::Proximal; n_vertices];
        let at = |i: usize| -> Result<V3, String> {
            arm_rest
                .get(i)
                .copied()
                .filter(|p| finite(*p))
                .ok_or(format!("reposo del brazo: vertice {i} ausente o no finito"))
        };
        for (i, v) in topo.vertices().iter().enumerate() {
            match *v {
                GrowthVertex::Retained { side, source } => {
                    rest_vertices[i] = match source {
                        VertexSource::Orig { orig, .. } => at(orig)?,
                        VertexSource::Cut { a, b, t } => {
                            let (pa, pb) = (at(a)?, at(b)?);
                            std::array::from_fn(|k| pa[k] + (pb[k] - pa[k]) * t)
                        }
                    };
                    kinds[i] = if side == Side::D {
                        VertexKind::Distal
                    } else {
                        VertexKind::Proximal
                    };
                }
                GrowthVertex::Ring { j, k } => kinds[i] = VertexKind::Ring { j, k },
            }
        }
        let loop_p = topo.loop_p();
        let lp: Vec<V3> = loop_p.iter().map(|&i| rest_vertices[i]).collect();
        let inv = 1.0 / lp.len() as f64;
        let center: V3 = std::array::from_fn(|a| lp.iter().map(|p| p[a]).sum::<f64>() * inv);
        let law = data.loop_frame(1)?;
        let tangent = vmul(law.tangent, 1.0 / vlen(law.tangent));
        let loop_offsets: Vec<V3> = lp.iter().map(|p| vsub(*p, center)).collect();
        let contour_radius = loop_offsets.iter().map(|o| vlen(*o)).fold(0.0, f64::max);
        let last_ring = topo.rings().len() - 1;
        for (j, ring) in topo.rings().iter().enumerate() {
            if ring.len() != lp.len() {
                return Err(format!(
                    "anillo {j} con otro numero de vertices que el lazo"
                ));
            }
            if j > 0 && j < last_ring {
                for (k, &vi) in ring.iter().enumerate() {
                    if kinds[vi] != (VertexKind::Ring { j, k }) {
                        return Err(format!("anillo {j}: vertice {vi} no es el anillo esperado"));
                    }
                    rest_vertices[vi] = lp[k];
                }
            }
        }
        // Normales de reposo: las de la ley con o = 0 (anillos sin giro = normales del lazo).
        let mut scratch = copy_model(data.template());
        data.evaluate_with(data.offset(1, [0.0; 3])?, &mut scratch)?;
        let rest_normals = scratch.normals;
        let mut corner_ring = Vec::with_capacity(3 * topo.faces().len());
        for f in topo.faces() {
            for s in &f.src {
                corner_ring.push(match (f.part, *s) {
                    (Part::G, CornerSource::Ring { ring, .. }) if ring <= last_ring => {
                        CornerKind::Ring(ring)
                    }
                    (Part::D, _) => CornerKind::Distal,
                    _ => CornerKind::Fixed,
                });
            }
        }
        let grip = topo.grip();
        let gc: V3 = std::array::from_fn(|a| {
            grip.iter().map(|&i| rest_vertices[i][a]).sum::<f64>() / grip.len().max(1) as f64
        });
        let tip_offset = vdot(vsub(gc, center), tangent);
        let bend_radius = BEND_RADIUS_CONTOURS * contour_radius;
        if !(finite(center)
            && finite(tangent)
            && contour_radius.is_finite()
            && contour_radius > 0.0
            && tip_offset.is_finite()
            && tip_offset >= 2.0 * bend_radius
            && rest_vertices
                .iter()
                .chain(&rest_normals)
                .all(|p| finite(*p)))
        {
            return Err(format!(
                "reposo invalido para el codo: radio de contorno {contour_radius}, punta axial {tip_offset}, codo {bend_radius}"
            ));
        }
        Ok(Self {
            rest_vertices,
            rest_normals,
            rest: RestLoop {
                center,
                tangent,
                contour_radius,
            },
            bend_radius,
            tip_offset,
            kinds,
            corner_ring,
            loop_offsets,
            rings: last_ring,
        })
    }
    pub fn rest_loop(&self) -> RestLoop {
        self.rest
    }
    /// Clasificacion de la plantilla (solo lectura) para otras familias de deformacion del crate:
    /// clase de cada vertice, giro de cada esquina, desplazamientos del lazo P y numero de anillos.
    pub(crate) fn layout(&self) -> RigLayout<'_> {
        (
            &self.kinds,
            &self.corner_ring,
            &self.loop_offsets,
            self.rings,
        )
    }
    pub fn bend_radius(&self) -> f64 {
        self.bend_radius
    }
    /// Distancia axial de la punta perseguida (centroide del agarre proyectado en la tangente) al
    /// centro del lazo, en reposo (OBJ).
    pub fn tip_offset(&self) -> f64 {
        self.tip_offset
    }
    /// Radio de EXACTITUD alrededor del lazo (OBJ): desde `bend + sqrt(bend^2 + a^2)` la punta axial de la
    /// malla coincide con la pedida en cualquier direccion (el tramo recto cubre al distal); mas cerca solo
    /// sobre el eje de reposo. Es la bola minima que usa el controlador.
    pub fn exact_radius(&self) -> f64 {
        let (rho, a) = (self.bend_radius, self.tip_offset);
        rho + (rho * rho + a * a).sqrt()
    }
    /// Punta de reposo (OBJ).
    pub fn rest_tip(&self) -> [f64; 3] {
        let (c, t, a) = (self.rest.center, self.rest.tangent, self.tip_offset);
        [c[0] + t[0] * a, c[1] + t[1] * a, c[2] + t[2] * a]
    }
    pub fn rest_vertices(&self) -> &[[f64; 3]] {
        &self.rest_vertices
    }
    pub fn rest_normals(&self) -> &[[f64; 3]] {
        &self.rest_normals
    }
    /// Escribe posiciones y normales del tramo para la punta `tip_obj` en `vertices`/`normals`
    /// (topologia de la plantilla). `side` es el lado del codo del cuadro anterior (histeresis).
    ///
    /// - Camino: codo circular de radio `bend_radius` desde el lazo P con su tangente, en el plano de la
    ///   tangente y la punta, y recta hasta la punta. El tramo procedural termina `tip_offset` antes de
    ///   la punta (sobre la recta o, de cerca, sobre el codo), donde va el lazo distal.
    /// - Lado del codo: el corto; con `side` anterior opuesto se sigue por el mismo lado mientras el
    ///   giro no pase de `pi + SIDE_HYSTERESIS` (cambio declarado en `side_switched`).
    /// - Anillos: copias rigidas del lazo P giradas con la tangente del camino (giro alrededor de un eje
    ///   fijo: sin torsion), repartidos por longitud dando al codo una fraccion minima.
    /// - Distal: rigido, girado con la tangente final. Proximal: intacto.
    /// - `Err` (punta no finita, dentro de la bola `tip_offset`, mas alla de `MAX_REACH_OBJ` o buffers
    ///   de otra topologia) ANTES de escribir.
    pub fn deform(
        &self,
        tip_obj: [f64; 3],
        side: Option<[f64; 3]>,
        vertices: &mut [[f64; 3]],
        normals: &mut [[f64; 3]],
    ) -> Result<ShaftPose, String> {
        if vertices.len() != self.rest_vertices.len() || normals.len() != self.rest_normals.len() {
            return Err("buffers sin la topologia del tramo".into());
        }
        if !finite(tip_obj) || side.is_some_and(|s| !finite(s) || !vlen(s).is_finite()) {
            return Err(format!("punta OBJ no finita: {tip_obj:?}"));
        }
        let (cp, t, rho, a) = (
            self.rest.center,
            self.rest.tangent,
            self.bend_radius,
            self.tip_offset,
        );
        let mut d = vsub(tip_obj, cp);
        let r = vlen(d);
        if !(r >= a * (1.0 - 1e-9) && r <= MAX_REACH_OBJ * (1.0 + 1e-12)) {
            return Err(format!(
                "punta a {r} del lazo: fuera de [{a}, {MAX_REACH_OBJ}] (OBJ)"
            ));
        }
        if r < a {
            d = vmul(d, a / r);
        }
        // Lado del codo con histeresis.
        let x = vdot(d, t);
        let yv = vsub(d, vmul(t, x));
        let y = vlen(yv);
        let previous = side
            .map(|s| vsub(s, vmul(t, vdot(s, t))))
            .filter(|s| vlen(*s) > 1e-9)
            .map(|s| vmul(s, 1.0 / vlen(s)));
        let mut switched = false;
        let b = if y > 1e-12 * r.max(1.0) {
            let short = vmul(yv, 1.0 / y);
            match previous {
                Some(p) if vdot(p, short) < 0.0 => {
                    let long = vmul(short, -1.0);
                    match Dubins::solve(d, t, long, rho) {
                        Some(s) if s.bend <= std::f64::consts::PI + SIDE_HYSTERESIS => long,
                        _ => {
                            switched = true;
                            short
                        }
                    }
                }
                _ => short,
            }
        } else {
            previous.unwrap_or_else(|| perpendicular(t))
        };
        let path = Dubins::solve(d, t, b, rho).ok_or("punta dentro del circulo del codo")?;
        let arc = rho * path.bend;
        let shaft = (arc + path.line - a).max(0.0);
        let arc_part = arc.min(shaft);
        let line_part = shaft - arc_part;
        // Fraccion de anillos en el codo: al menos la proporcional y hasta la mitad segun el giro.
        let ring_fraction = if shaft <= 0.0 {
            0.0
        } else if line_part <= 0.0 {
            1.0
        } else {
            (arc_part / shaft)
                .max(0.5 * (arc_part / rho).min(std::f64::consts::PI) / std::f64::consts::PI)
        };
        let curve = |s: f64| -> (V3, f64) {
            if s <= arc {
                let phi = s / rho;
                let p = vadd(
                    cp,
                    vadd(vmul(t, rho * phi.sin()), vmul(b, rho * (1.0 - phi.cos()))),
                );
                (p, phi)
            } else {
                let (sb, cb) = path.bend.sin_cos();
                let elbow = vadd(cp, vadd(vmul(t, rho * sb), vmul(b, rho * (1.0 - cb))));
                let u = vadd(vmul(t, cb), vmul(b, sb));
                (vadd(elbow, vmul(u, s - arc)), path.bend)
            }
        };
        let ring_s = |j: usize| -> f64 {
            let u = j as f64 / self.rings as f64;
            if ring_fraction > 0.0 && u <= ring_fraction {
                arc_part * (u / ring_fraction)
            } else if ring_fraction < 1.0 {
                arc_part + line_part * ((u - ring_fraction) / (1.0 - ring_fraction))
            } else {
                arc_part
            }
        };
        let (distal, end) = curve(shaft);
        let axis = path.axis;
        let axial_tip = vadd(distal, vmul(rotate(t, axis, end), a));
        let frames: Vec<(V3, f64)> = (0..=self.rings)
            .map(|j| match j {
                0 => (cp, 0.0),
                j if j == self.rings => (distal, end),
                j => curve(ring_s(j)),
            })
            .collect();
        if !(finite(distal)
            && finite(axial_tip)
            && end.is_finite()
            && frames.iter().all(|(p, a)| finite(*p) && a.is_finite()))
        {
            return Err("tramo no finito".into());
        }
        // Evaluate every derived output before touching either caller buffer. Finite rest data
        // alone does not imply finite Rodrigues products/sums (LP-R03).
        let vertex = |i: usize, kind: VertexKind| match kind {
            VertexKind::Proximal => self.rest_vertices[i],
            VertexKind::Distal => vadd(distal, rotate(vsub(self.rest_vertices[i], cp), axis, end)),
            VertexKind::Ring { j, k } => {
                let (c, phi) = frames[j];
                vadd(c, rotate(self.loop_offsets[k], axis, phi))
            }
        };
        let normal = |c: usize, kind: CornerKind| match kind {
            CornerKind::Fixed => self.rest_normals[c],
            CornerKind::Distal => rotate(self.rest_normals[c], axis, end),
            CornerKind::Ring(j) => rotate(self.rest_normals[c], axis, frames[j].1),
        };
        if ![
            path.bend,
            path.line,
            arc,
            shaft,
            arc_part,
            line_part,
            ring_fraction,
        ]
        .iter()
        .all(|v| v.is_finite())
            || !self
                .kinds
                .iter()
                .enumerate()
                .all(|(i, &k)| finite(vertex(i, k)))
            || !self
                .corner_ring
                .iter()
                .enumerate()
                .all(|(i, &k)| finite(normal(i, k)))
        {
            return Err("posiciones, normales o medidas derivadas no finitas".into());
        }
        // Publication is now infallible; repeat the same deterministic arithmetic without IO.
        for (i, &kind) in self.kinds.iter().enumerate() {
            vertices[i] = vertex(i, kind);
        }
        for (c, &kind) in self.corner_ring.iter().enumerate() {
            normals[c] = normal(c, kind);
        }
        Ok(ShaftPose {
            tip_obj,
            bend: path.bend,
            end_rotation: end,
            shaft_length: shaft,
            bend_side: b,
            side_switched: switched,
            distal_center: distal,
            axial_tip,
            line_length: path.line,
        })
    }
}

/// Lo que pide la linea de comandos con `--tentacle-live-pursuit` (mismos paquetes privados).
#[derive(Clone, Debug, PartialEq)]
pub struct LivePursuitRequest {
    pub capture_dir: PathBuf,
    pub growth_package: PathBuf,
    pub correspondence: PathBuf,
}

impl LivePursuitRequest {
    /// El pedido equivalente de la composicion (para cargar y proteger las MISMAS entradas).
    pub fn capture_request(&self) -> TentacleCaptureRequest {
        TentacleCaptureRequest {
            capture_dir: self.capture_dir.clone(),
            growth_package: self.growth_package.clone(),
            correspondence: self.correspondence.clone(),
            start_time: None,
        }
    }
    /// Rutas existentes del tipo esperado (solo metadatos).
    pub fn validate(&self) -> Result<(), String> {
        self.capture_request().validate()
    }
}

/// Combinaciones de `--tentacle-live-pursuit` sin tocar archivos. Sin el flag: `Ok(None)` y los
/// flags del tentaculo siguen las reglas de siempre (`validate_tentacle_capture_flags`).
pub fn validate_live_pursuit_flags(
    live: bool,
    capture: &TentacleCaptureFlags,
) -> Result<Option<LivePursuitRequest>, String> {
    if !live {
        return Ok(None);
    }
    if capture.enabled {
        return Err("--tentacle-live-pursuit y --tentacle-capture-debug son incompatibles (la persecucion viva no usa la instantanea F7)".into());
    }
    if capture.start_time.is_some() {
        return Err("--tentacle-live-pursuit no admite --tentacle-capture-time (el reloj de la composicion observadora)".into());
    }
    if capture.legacy {
        return Err("--legacy no admite --tentacle-live-pursuit".into());
    }
    if !capture.solar {
        return Err("--tentacle-live-pursuit requiere --creature-solar-reveal (persecucion en la ruta solar)".into());
    }
    let need = |v: &Option<PathBuf>, flag: &str| {
        v.clone()
            .ok_or(format!("--tentacle-live-pursuit requiere {flag}"))
    };
    let request = LivePursuitRequest {
        capture_dir: need(
            &capture.capture_dir,
            "--tentacle-capture-package <carpeta capture_v2>",
        )?,
        growth_package: need(
            &capture.growth_package,
            "--tentacle-growth-package <growth_package.txt>",
        )?,
        correspondence: need(
            &capture.correspondence,
            "--tentacle-arm-correspondence <arm_correspondence.txt>",
        )?,
    };
    request.validate()?;
    Ok(Some(request))
}

/// Lo verificado al enlazar (identidad de `SolarCaptureDebug::bind` + reposo de la deformacion).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveBindReport {
    pub capture: BindReport,
    pub rest_loop: RestLoop,
    pub bend_radius_obj: f64,
    pub tip_offset_obj: f64,
    pub max_reach_obj: f64,
    /// Vertices retenidos del tramo puestos en la posicion EXACTA del Meshy.
    pub exact_rest_vertices: usize,
}

/// Estado del ultimo paso publicado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveFrameStatus {
    pub step: PursuitStepReport,
    pub tip_world: Vec3,
    pub ship_world: Vec3,
    pub tip_ship_distance: f64,
    pub shaft: Option<ShaftPose>,
    pub world_per_obj: f64,
}

/// Persecucion viva: composicion enlazada, deformacion, controlador y capa `Body` propia.
pub struct LivePursuit {
    debug: SolarCaptureDebug,
    /// Exact, bind-time geometry and body partition; no IO or pointer-lifetime assumptions.
    bound_identity: Option<(Model, Model)>,
    rig: Option<ShaftRig>,
    composite: Option<Model>,
    prefix: Option<(usize, usize)>,
    controller: Option<PursuitController>,
    last: Option<LiveFrameStatus>,
    /// Lado del codo publicado (histeresis entre cuadros).
    side: Option<[f64; 3]>,
    /// Esfera OBJ del tramo publicado (descarte).
    extra_bounds_obj: (Vec3, f64),
    /// El ultimo cuadro publicado dibuja la capa propia.
    drawn: bool,
    /// Buffers de trabajo (se reutilizan; sin E/S ni recarga por cuadro).
    scratch_vertices: Vec<[f64; 3]>,
    scratch_normals: Vec<[f64; 3]>,
}

/// Inversa de una semejanza (rotacion * escala uniforme + traslacion): filas de la inversa lineal y
/// la escala. `Err` si no es finita, invertible o uniforme.
pub(crate) fn similarity_inverse(m: &crate::math::Mat4) -> Result<([Vec3; 3], f64), String> {
    let (c0, c1, c2) = (m.column(0), m.column(1), m.column(2));
    let det = c0.dot(c1.cross(c2));
    let rows = [c1.cross(c2), c2.cross(c0), c0.cross(c1)].map(|r| r * (1.0 / det));
    let scale = c0.length();
    let uniform = [c1.length(), c2.length()]
        .iter()
        .all(|l| (l - scale).abs() <= 1e-9 * scale);
    if !(det.is_finite()
        && det > 0.0
        && scale.is_finite()
        && scale > 0.0
        && uniform
        && rows.iter().all(|r| r.is_finite())
        && m.m.iter().flatten().all(|v| v.is_finite()))
    {
        return Err("transform de la criatura no finito, no invertible o no uniforme".into());
    }
    Ok((rows, scale))
}

impl LivePursuit {
    /// Envuelve una composicion YA cargada (sin activar). La identidad se verifica en `bind`.
    pub fn new(debug: SolarCaptureDebug) -> Self {
        Self {
            debug,
            bound_identity: None,
            rig: None,
            composite: None,
            prefix: None,
            controller: None,
            last: None,
            side: None,
            extra_bounds_obj: (Vec3::ZERO, 0.0),
            drawn: false,
            scratch_vertices: Vec::new(),
            scratch_normals: Vec::new(),
        }
    }
    /// Carga UNA vez los paquetes con las mismas validaciones de la composicion.
    pub fn load(request: &LivePursuitRequest) -> Result<Self, String> {
        Ok(Self::new(SolarCaptureDebug::load(
            &request.capture_request(),
        )?))
    }
    pub fn debug(&self) -> &SolarCaptureDebug {
        &self.debug
    }
    pub fn rig(&self) -> Option<&ShaftRig> {
        self.rig.as_ref()
    }
    pub fn controller(&self) -> Option<&PursuitController> {
        self.controller.as_ref()
    }
    pub fn last(&self) -> Option<LiveFrameStatus> {
        self.last
    }
    /// Capa compuesta del ultimo paso publicado (prefijo del cuerpo + tramo).
    pub fn composite(&self) -> Option<&Model> {
        self.composite.as_ref()
    }
    /// Tamanos del prefijo del cuerpo (vertices, normales) en la capa compuesta.
    pub fn body_prefix(&self) -> Option<(usize, usize)> {
        self.prefix
    }
    /// Enlaza con la criatura del World (una vez, tras `SolarGame::start`): la identidad y la
    /// procedencia son las de `SolarCaptureDebug::bind` (sin debilitarlas); despues el reposo EXACTO
    /// del tramo con las posiciones del Meshy (correspondencia) y una COPIA de la capa enlazada. Con
    /// `Err` nada queda enlazado.
    pub fn bind(&mut self, world: &World) -> Result<LiveBindReport, String> {
        let prepared = self.debug.prepare_bind(world)?;
        let capture = prepared.report();
        let model = world
            .creature()
            .ok_or("el World no tiene criatura")?
            .model();
        let arm_rest: Vec<[f64; 3]> = self
            .debug
            .correspondence()
            .arm_vertex()
            .iter()
            .map(|&m| model.vertices[m])
            .collect();
        let data = self.debug.package().data();
        let rig = ShaftRig::new(data, &arm_rest)?;
        let (layer, nv, nn) = prepared.layer();
        let (gv, gn) = (rig.rest_vertices.len(), rig.rest_normals.len());
        if layer.vertices.len() != nv + gv || layer.normals.len() != nn + gn {
            return Err("capa enlazada sin la plantilla del tramo al final".into());
        }
        let mut composite = copy_model(layer);
        composite.vertices[nv..].copy_from_slice(&rig.rest_vertices);
        composite.normals[nn..].copy_from_slice(&rig.rest_normals);
        let exact = rig
            .kinds
            .iter()
            .filter(|k| !matches!(k, VertexKind::Ring { .. }))
            .count();
        let report = LiveBindReport {
            capture,
            rest_loop: rig.rest,
            bend_radius_obj: rig.bend_radius,
            tip_offset_obj: rig.tip_offset,
            max_reach_obj: MAX_REACH_OBJ,
            exact_rest_vertices: exact,
        };
        let identity = (
            copy_model(model),
            copy_model(
                world
                    .staged_creature_parts()
                    .ok_or("el World no tiene particion")?
                    .body(),
            ),
        );
        self.debug.publish_bind(prepared);
        self.bound_identity = Some(identity);
        self.scratch_vertices = rig.rest_vertices.clone();
        self.scratch_normals = rig.rest_normals.clone();
        self.rig = Some(rig);
        self.composite = Some(composite);
        self.prefix = Some((nv, nn));
        self.controller = None;
        self.last = None;
        self.side = None;
        self.drawn = false;
        Ok(report)
    }
    /// Un paso con el estado ACTUAL del World (posicion de la nave, transform de la criatura, O2
    /// mostrado) y `dt` (s, el de la fisica). Valida y calcula TODO en buffers de trabajo y publica al
    /// final: con `Err` controlador, capa, esfera y estado no cambian. Nunca escribe el World.
    pub fn step(&mut self, world: &World, dt: f64) -> Result<LiveFrameStatus, String> {
        let rig = self.rig.as_ref().ok_or("persecucion sin enlazar (bind)")?;
        let (model, body) = self
            .bound_identity
            .as_ref()
            .ok_or("identidad sin enlazar")?;
        if !world
            .creature()
            .is_some_and(|c| same_model(model, c.model()))
            || !world
                .staged_creature_parts()
                .is_some_and(|p| same_model(body, p.body()))
        {
            return Err("criatura o particion distintas del bind de persecucion".into());
        }
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}"
            ));
        }
        let c = world
            .creature_render_transform()
            .ok_or("el World no tiene criatura")?
            .matrix();
        let (inverse, scale) = similarity_inverse(&c)?;
        let anchor = c.transform_point(Vec3::from_array(rig.rest.center));
        let rest_tip = c.transform_point(Vec3::from_array(rig.rest_tip()));
        let input = PursuitInput {
            ship: world.ship.position,
            anchor,
            rest_tip,
            oxygen01: world.debug_oxygen01,
            dt,
            min_radius: rig.exact_radius() * scale,
            max_radius: MAX_REACH_OBJ * scale,
        };
        let mut controller = match self.controller {
            Some(c) => c,
            None => PursuitController::at_rest(rest_tip)?,
        };
        let report = controller.step(&input)?;
        let draw = controller.mode() != PursuitMode::Idle;
        let mut shaft = None;
        let mut bounds = self.extra_bounds_obj;
        if draw {
            let rel = controller.tip() - c.column(3);
            let mut tip_obj = [
                inverse[0].dot(rel),
                inverse[1].dot(rel),
                inverse[2].dot(rel),
            ];
            // Si el giro del cuerpo dejo la punta mas cerca del lazo que la punta axial de reposo, la
            // malla usa su proyeccion a esa distancia (el controlador ya la saca radialmente).
            let rest = rig.rest;
            let d = vsub(tip_obj, rest.center);
            let r = vlen(d);
            if r < rig.tip_offset {
                let dir = if r > 0.0 {
                    vmul(d, 1.0 / r)
                } else {
                    rest.tangent
                };
                tip_obj = vadd(rest.center, vmul(dir, rig.tip_offset));
            }
            let pose = rig.deform(
                tip_obj,
                self.side,
                &mut self.scratch_vertices,
                &mut self.scratch_normals,
            )?;
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for v in &self.scratch_vertices {
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                }
            }
            let center = Vec3::new(
                (lo[0] + hi[0]) * 0.5,
                (lo[1] + hi[1]) * 0.5,
                (lo[2] + hi[2]) * 0.5,
            );
            let radius = self
                .scratch_vertices
                .iter()
                .map(|v| (Vec3::from_array(*v) - center).length())
                .fold(0.0, f64::max);
            if !(center.is_finite() && radius.is_finite()) {
                return Err("esfera del tramo no finita".into());
            }
            bounds = (center, radius);
            shaft = Some(pose);
        }
        // Publicacion (sin operaciones falibles desde aqui).
        let tip_world = controller.tip();
        let status = LiveFrameStatus {
            step: report,
            tip_world,
            ship_world: world.ship.position,
            tip_ship_distance: (world.ship.position - tip_world).length(),
            shaft,
            world_per_obj: scale,
        };
        if let (Some(pose), Some(composite), Some((nv, nn))) =
            (shaft, self.composite.as_mut(), self.prefix)
        {
            composite.vertices[nv..].copy_from_slice(&self.scratch_vertices);
            composite.normals[nn..].copy_from_slice(&self.scratch_normals);
            self.side = Some(pose.bend_side);
        } else if !draw {
            self.side = None;
        }
        self.extra_bounds_obj = bounds;
        self.drawn = draw;
        self.controller = Some(controller);
        self.last = Some(status);
        Ok(status)
    }
    /// Capa `Body` del ultimo paso; `None` en reposo (cuadro exacto de siempre).
    pub fn body_override(&self) -> Option<SolarBodyOverride<'_>> {
        if !self.drawn {
            return None;
        }
        Some(SolarBodyOverride {
            model: self.composite.as_ref()?,
            extra_bounds_obj: self.extra_bounds_obj,
        })
    }
    /// Estado para el titulo (sin HUD).
    pub fn title(&self) -> String {
        let Some(s) = self.last else {
            return "PERSECUCION VIVA (prototipo, sin agarre): sin paso".into();
        };
        let mode = match s.step.mode {
            PursuitMode::Idle => "reposo (O2 > 15 %)",
            PursuitMode::Pursuing => "persigue",
            PursuitMode::Retracting => "recoge",
        };
        format!(
            "PERSECUCION VIVA (prototipo, sin agarre) | {mode} | v max {:.0} u/s | punta-nave {:.0} u{}",
            s.step.max_speed,
            s.tip_ship_distance,
            if s.step.target_clamped {
                " | nave fuera de alcance/bola: objetivo recortado"
            } else {
                ""
            }
        )
    }
}

pub(crate) fn in_domain(v: Vec3) -> bool {
    [v.x, v.y, v.z]
        .iter()
        .all(|c| c.is_finite() && c.abs() <= NUMERIC_WORLD_LIMIT)
}

pub(crate) fn same_model(a: &Model, b: &Model) -> bool {
    a.vertices == b.vertices
        && a.uvs == b.uvs
        && a.normals == b.normals
        && a.material_libraries == b.material_libraries
        && a.materials == b.materials
        && a.bounds_center == b.bounds_center
        && a.bounds_radius == b.bounds_radius
        && a.faces.len() == b.faces.len()
        && a.faces.iter().zip(&b.faces).all(|(a, b)| {
            a.vertex_indices == b.vertex_indices
                && a.uv_indices == b.uv_indices
                && a.normal_indices == b.normal_indices
                && a.material == b.material
        })
}

pub(crate) fn copy_model(m: &Model) -> Model {
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
