//! UMB-311 PERSECUCION ESPACIAL 3D del tentaculo (variante opt-in `--tentacle-spatial-pursuit`, ruta
//! solar). Ver `docs/tentacle-spatial-pursuit.md`. NO es captura: sin agarre, arrastre, bloqueo de input,
//! muerte, audio, apagon, cooldown ni colisiones; nunca mueve la nave ni la camara.
//!
//! Hipotesis UNICA **GEO-SPATIAL-01 — codo espacial con memoria de lado**:
//!
//! - **Controlador**: el MISMO `PursuitController` de la persecucion viva (punta en mundo, nave ACTUAL,
//!   O2 15/10/5, llegada sin pasarse). Solo cambia la bola minima (la del dominio exacto de esta familia).
//! - **Curva** (OBJ, desde el lazo P con su tangente `t`): primer codo circular de radio `rho` hacia el
//!   LADO DE MEMORIA `m` (perpendicular a `t`) con un giro `f * giro_plano(punta)`, despues el camino plano
//!   codo + recta (Dubins) DESDE el final del primer codo hasta la punta. Si `m` coincide con el lado de la
//!   punta, la curva es exactamente la plana de la familia anterior; si no, es una curva 3D.
//! - **Memoria (ajuste A1, perezosa por margen)**: `m` gira alrededor de `t` hacia el lado de la punta lo
//!   MINIMO para mantener el margen a la singularidad del segundo codo `>= SPATIAL_SAFE_MARGIN`, mas una
//!   deriva lenta (`SPATIAL_SIDE_DRIFT * |dT| / |T|`), nunca mas que `SPATIAL_SIDE_RATE * |dT| / |T|` por
//!   paso. Cerca del polo trasero el lado de la punta no esta definido y `m` no salta: la curva depende con
//!   continuidad de (punta, `m`) y `m` cambia poco por paso.
//! - **H-FIX01 (reintento acotado)**: el giro que decide A1 se acota para que el desplazamiento de la
//!   malla que causa la memoria no supere `SPATIAL_CONTINUITY_GAIN * |dT|` menos el de la punta sola; por la
//!   desigualdad triangular cada cuadro cumple la cota de continuidad de LP-R01/LP-R02 (sin holgura extra).
//!   Ver `docs/tentacle-spatial-pursuit.md`.
//! - **Marcos**: rotaciones compuestas de los dos codos (minimas para arcos circulares); cada anillo es una
//!   copia RIGIDA del lazo aprobado y el distal/espiral es rigido con el marco final real.
//!
//! Valores numericos (fraccion del primer codo, rapidez de la memoria): AJUSTES CANDIDATOS.
use crate::{
    assets::obj::Model,
    math::Vec3,
    scene::world::{SolarBodyOverride, World},
    visual::{
        tentacle_growth_cpu::GrowthData,
        tentacle_live_pursuit::{
            CornerKind, LivePursuitRequest, MAX_PURSUIT_DT, MAX_REACH_OBJ, PursuitController,
            PursuitInput, PursuitMode, PursuitStepReport, RestLoop, ShaftRig, VertexKind,
            copy_model, in_domain, same_model, similarity_inverse,
        },
        tentacle_solar_game::{BindReport, SolarCaptureDebug, TentacleCaptureFlags},
    },
};

/// Fraccion CANDIDATA del giro plano equivalente que hace el primer codo hacia el lado de memoria.
pub const SPATIAL_FIRST_ELBOW_FRACTION: f64 = 0.5;
/// TOPE CANDIDATO del giro del lado de memoria: radianes por unidad de paso angular de la punta
/// (`|dT| / |T|` en OBJ). Ningun cuadro gira la memoria mas que esto.
pub const SPATIAL_SIDE_RATE: f64 = 8.0;
/// Ajuste A1: deriva CANDIDATA de la memoria hacia el lado de la punta (rad por unidad de paso angular).
pub const SPATIAL_SIDE_DRIFT: f64 = 1.0;
/// Ajuste A1: margen CANDIDATO a la singularidad del segundo codo que la memoria mantiene (rad, 50 grados).
pub const SPATIAL_SAFE_MARGIN: f64 = 50.0 * std::f64::consts::PI / 180.0;
/// H-FIX01: cota de continuidad del contrato LP-R01/LP-R02 (salto maximo de la malla por unidad de paso de
/// la punta). El giro de la memoria nunca gasta mas que lo que deja libre el paso de la punta.
pub const SPATIAL_CONTINUITY_GAIN: f64 = 8.0;

/// Geometria de un cuadro para una punta efectiva `d` (relativa al lazo, OBJ) y un lado `side`.
struct Geometry {
    th1: f64,
    th2: f64,
    line: f64,
    arc: f64,
    shaft: f64,
    arc_part: f64,
    line_part: f64,
    ring_fraction: f64,
    r1: M3,
    r2: M3,
    distal: V3,
    end: M3,
    axial_tip: V3,
    frames: Vec<(V3, M3)>,
    margin: f64,
}

fn max_displacement(a: &[V3], b: &[V3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(p, q)| vlen(vsub(*p, *q)))
        .fold(0.0, f64::max)
}

type V3 = [f64; 3];
/// Matriz 3x3 por filas.
type M3 = [[f64; 3]; 3];

const IDENTITY: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

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
/// Rotacion de angulo `angle` alrededor del eje unitario `axis` (Rodrigues, por filas).
fn axis_angle(axis: V3, angle: f64) -> M3 {
    let (s, c) = angle.sin_cos();
    let [x, y, z] = axis;
    let k = 1.0 - c;
    [
        [c + x * x * k, x * y * k - z * s, x * z * k + y * s],
        [y * x * k + z * s, c + y * y * k, y * z * k - x * s],
        [z * x * k - y * s, z * y * k + x * s, c + z * z * k],
    ]
}
fn mmul(a: M3, b: M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}
fn mapply(m: M3, v: V3) -> V3 {
    std::array::from_fn(|i| vdot(m[i], v))
}
/// Rotacion propia finita: `M M^T = I` y `det = 1` (tolerancia 1e-9).
fn proper_rotation(m: M3) -> bool {
    m.iter().all(|r| finite(*r))
        && (0..3)
            .all(|i| (0..3).all(|j| (vdot(m[i], m[j]) - f64::from(u8::from(i == j))).abs() <= 1e-9))
        && (vdot(m[0], vcross(m[1], m[2])) - 1.0).abs() <= 1e-9
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

/// Camino plano codo (radio `rho`, hacia `b`) + recta desde el origen con tangente `t` hasta `d`
/// (relativo): `(giro, recta)`. `None` si `d` esta dentro del circulo de giro.
fn dubins(d: V3, t: V3, b: V3, rho: f64) -> Option<(f64, f64)> {
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
    Some((bend, line))
}

/// Memoria entre cuadros de la deformacion espacial (OBJ).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialMemory {
    /// Lado del primer codo (unitario, perpendicular a la tangente del lazo).
    pub side: [f64; 3],
    /// Punta efectiva del cuadro en que se fijo `side` (para acotar el giro por el paso de la punta).
    pub tip_obj: [f64; 3],
}

/// Geometria espacial de un cuadro (OBJ). NO hay `bend_side`/`side_switched`: no hay ramas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialPose {
    /// Punta pedida (OBJ).
    pub tip_obj: [f64; 3],
    /// Giro del primer codo hacia `memory.side` (rad).
    pub first_elbow: f64,
    /// Giro del segundo codo (plano final hacia la punta) (rad).
    pub second_elbow: f64,
    /// Giro total recorrido por la tangente a lo largo del tramo (suma de codos, rad).
    pub turn: f64,
    /// Giro de la memoria de lado aplicado en este cuadro (rad, con signo alrededor de la tangente).
    pub side_rotation: f64,
    /// Margen a la singularidad del segundo codo: pi - angulo(tangente tras el primer codo, punta) (rad).
    pub singular_margin: f64,
    /// Rotacion RIGIDA aplicada al distal (filas), igual al marco final del tramo.
    pub end_rotation: [[f64; 3]; 3],
    /// Longitud del tramo procedural (OBJ).
    pub shaft_length: f64,
    /// Longitud de la recta final del camino (OBJ).
    pub line_length: f64,
    /// Centro del lazo distal (fin del tramo) y punta axial resultante.
    pub distal_center: [f64; 3],
    pub axial_tip: [f64; 3],
    /// Memoria a pasar al cuadro siguiente.
    pub memory: SpatialMemory,
}

/// Deformacion espacial sobre los mismos datos de reposo de `ShaftRig` (cacheados una vez).
pub struct SpatialRig {
    base: ShaftRig,
}

impl SpatialRig {
    /// Reposo EXACTO y validaciones de `ShaftRig::new` (sin debilitarlas).
    pub fn new(data: &GrowthData, arm_rest: &[[f64; 3]]) -> Result<Self, String> {
        Ok(Self {
            base: ShaftRig::new(data, arm_rest)?,
        })
    }
    /// Datos de reposo compartidos con la familia plana (solo lectura).
    pub fn base(&self) -> &ShaftRig {
        &self.base
    }
    pub fn rest_loop(&self) -> RestLoop {
        self.base.rest_loop()
    }
    pub fn bend_radius(&self) -> f64 {
        self.base.bend_radius()
    }
    pub fn tip_offset(&self) -> f64 {
        self.base.tip_offset()
    }
    pub fn rest_tip(&self) -> [f64; 3] {
        self.base.rest_tip()
    }
    pub fn rest_vertices(&self) -> &[[f64; 3]] {
        self.base.rest_vertices()
    }
    pub fn rest_normals(&self) -> &[[f64; 3]] {
        self.base.rest_normals()
    }
    /// Radio de EXACTITUD de esta familia alrededor del lazo (OBJ): `3 rho + sqrt(rho^2 + a^2)`. El final
    /// del primer codo dista a lo sumo `2 rho` del lazo, asi que desde aqui el segundo camino siempre deja
    /// una recta `>= a` y la punta axial coincide con la pedida para CUALQUIER memoria. Mas cerca solo
    /// sobre el eje de reposo. Es la bola minima que usa el controlador en esta ruta.
    pub fn min_radius(&self) -> f64 {
        let (rho, a) = (self.base.bend_radius(), self.base.tip_offset());
        3.0 * rho + (rho * rho + a * a).sqrt()
    }
    /// Geometria de un cuadro (camino, marcos de anillos, distal, punta axial y margen) para la punta
    /// efectiva `d` (relativa al lazo) y el lado `side`. Compartida por `deform` y por el presupuesto de la
    /// memoria: misma aritmetica, mismos bits.
    fn geometry(&self, d: V3, side: V3) -> Result<Geometry, String> {
        let base = &self.base;
        let rest = base.rest_loop();
        let (cp, t, rho, a) = (
            rest.center,
            rest.tangent,
            base.bend_radius(),
            base.tip_offset(),
        );
        let x = vdot(d, t);
        let y = vlen(vsub(d, vmul(t, x)));
        let (planar, _) = dubins([x, y, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], rho)
            .ok_or("punta dentro del circulo del codo")?;
        let th1 = SPATIAL_FIRST_ELBOW_FRACTION * planar;
        let axis1 = vcross(t, side);
        let r1 = axis_angle(axis1, th1);
        let p1 = vadd(
            vmul(t, rho * th1.sin()),
            vmul(side, rho * (1.0 - th1.cos())),
        );
        let t1 = mapply(r1, t);
        // Segundo codo + recta: plano de t1 y la punta vista desde el final del primer codo.
        let d2 = vsub(d, p1);
        let x2 = vdot(d2, t1);
        let yv2 = vsub(d2, vmul(t1, x2));
        let y2 = vlen(yv2);
        let b2 = if y2 > 1e-12 * vlen(d2).max(1.0) {
            vmul(yv2, 1.0 / y2)
        } else {
            mapply(r1, side)
        };
        let (th2, line) =
            dubins(d2, t1, b2, rho).ok_or("punta dentro del circulo del segundo codo")?;
        let axis2 = vcross(t1, b2);
        let r2 = mmul(axis_angle(axis2, th2), r1);
        let l1 = rho * th1;
        let arc = rho * (th1 + th2);
        let shaft = (arc + line - a).max(0.0);
        let arc_part = arc.min(shaft);
        let line_part = shaft - arc_part;
        let ring_fraction = if shaft <= 0.0 {
            0.0
        } else if line_part <= 0.0 {
            1.0
        } else {
            (arc_part / shaft)
                .max(0.5 * (arc_part / rho).min(std::f64::consts::PI) / std::f64::consts::PI)
        };
        let elbow2 = vadd(
            p1,
            vadd(vmul(t1, rho * th2.sin()), vmul(b2, rho * (1.0 - th2.cos()))),
        );
        let u_end = mapply(r2, t);
        let curve = |s: f64| -> (V3, M3) {
            if s <= l1 {
                let phi = s / rho;
                (
                    vadd(
                        vmul(t, rho * phi.sin()),
                        vmul(side, rho * (1.0 - phi.cos())),
                    ),
                    axis_angle(axis1, phi),
                )
            } else if s <= arc {
                let phi = (s - l1) / rho;
                (
                    vadd(
                        p1,
                        vadd(vmul(t1, rho * phi.sin()), vmul(b2, rho * (1.0 - phi.cos()))),
                    ),
                    mmul(axis_angle(axis2, phi), r1),
                )
            } else {
                (vadd(elbow2, vmul(u_end, s - arc)), r2)
            }
        };
        let rings = self.base.layout().3;
        let ring_s = |j: usize| -> f64 {
            let u = j as f64 / rings as f64;
            if ring_fraction > 0.0 && u <= ring_fraction {
                arc_part * (u / ring_fraction)
            } else if ring_fraction < 1.0 {
                arc_part + line_part * ((u - ring_fraction) / (1.0 - ring_fraction))
            } else {
                arc_part
            }
        };
        let (distal_rel, end) = curve(shaft);
        let distal = vadd(cp, distal_rel);
        let axial_tip = vadd(distal, vmul(mapply(end, t), a));
        let frames: Vec<(V3, M3)> = (0..=rings)
            .map(|j| match j {
                0 => (cp, IDENTITY),
                j if j == rings => (distal, end),
                j => {
                    let (p, m) = curve(ring_s(j));
                    (vadd(cp, p), m)
                }
            })
            .collect();
        let margin = {
            let l = vlen(d2);
            if l > 0.0 {
                std::f64::consts::PI - (vdot(d2, t1) / l).clamp(-1.0, 1.0).acos()
            } else {
                0.0
            }
        };
        Ok(Geometry {
            th1,
            th2,
            line,
            arc,
            shaft,
            arc_part,
            line_part,
            ring_fraction,
            r1,
            r2,
            distal,
            end,
            axial_tip,
            frames,
            margin,
        })
    }
    /// Posicion del vertice `i` de la plantilla en la geometria `g`.
    fn position(&self, g: &Geometry, i: usize, kind: VertexKind) -> V3 {
        let cp = self.base.rest_loop().center;
        let (_, _, loop_offsets, _) = self.base.layout();
        match kind {
            VertexKind::Proximal => self.base.rest_vertices()[i],
            VertexKind::Distal => vadd(
                g.distal,
                mapply(g.end, vsub(self.base.rest_vertices()[i], cp)),
            ),
            VertexKind::Ring { j, k } => {
                let (c, m) = g.frames[j];
                vadd(c, mapply(m, loop_offsets[k]))
            }
        }
    }
    fn positions(&self, g: &Geometry) -> Vec<V3> {
        let (kinds, _, _, _) = self.base.layout();
        kinds
            .iter()
            .enumerate()
            .map(|(i, &kind)| self.position(g, i, kind))
            .collect()
    }
    /// H-FIX01: mayor giro `<= turn` de la memoria (de `m` hacia el lado de la punta, via `rotate`) cuyo
    /// desplazamiento maximo de la malla a la punta nueva no supera `SPATIAL_CONTINUITY_GAIN * step` menos
    /// el desplazamiento que causa la punta sola (malla anterior con la memoria de entrada). Por la
    /// desigualdad triangular el salto del cuadro queda dentro de la cota. Solo se aceptan giros
    /// VERIFICADOS (0 siempre lo es); sin presupuesto o sin geometria previa la memoria no gira.
    fn memory_budget(
        &self,
        d: V3,
        m: V3,
        previous: V3,
        step: f64,
        turn: f64,
        rotate: &dyn Fn(f64) -> V3,
    ) -> f64 {
        let cp = self.base.rest_loop().center;
        let (Ok(before), Ok(target)) = (self.geometry(vsub(previous, cp), m), self.geometry(d, m))
        else {
            return 0.0;
        };
        let target = self.positions(&target);
        let budget =
            SPATIAL_CONTINUITY_GAIN * step - max_displacement(&self.positions(&before), &target);
        if !(budget.is_finite() && budget > 0.0) {
            return 0.0;
        }
        let fits = |psi: f64| {
            self.geometry(d, rotate(psi))
                .is_ok_and(|g| max_displacement(&self.positions(&g), &target) <= budget)
        };
        if fits(turn) {
            return turn;
        }
        let (mut lo, mut hi) = (0.0, turn);
        for _ in 0..50 {
            let mid = 0.5 * (lo + hi);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
    /// Escribe posiciones y normales del tramo para la punta `tip_obj` (OBJ) con la memoria del cuadro
    /// anterior (`None` al empezar). Todo se valida ANTES de escribir: punta/memoria finitas, punta en
    /// `[a, MAX_REACH_OBJ]`, marcos rotaciones propias, vertices y normales finitos, normales unitarias y
    /// anillos rigidos; con `Err` ningun buffer cambia.
    pub fn deform(
        &self,
        tip_obj: [f64; 3],
        memory: Option<SpatialMemory>,
        vertices: &mut [[f64; 3]],
        normals: &mut [[f64; 3]],
    ) -> Result<SpatialPose, String> {
        let base = &self.base;
        if vertices.len() != base.rest_vertices().len()
            || normals.len() != base.rest_normals().len()
        {
            return Err("buffers sin la topologia del tramo".into());
        }
        if !finite(tip_obj) {
            return Err(format!("punta OBJ no finita: {tip_obj:?}"));
        }
        let rest = base.rest_loop();
        let (cp, t, rho, a) = (
            rest.center,
            rest.tangent,
            base.bend_radius(),
            base.tip_offset(),
        );
        // Memoria: lado normalizado SIN desbordar (escala por la mayor componente) y perpendicular a t.
        let memory = match memory {
            None => None,
            Some(m) => {
                let big = m.side.iter().fold(0.0_f64, |acc, c| acc.max(c.abs()));
                if !(finite(m.side) && finite(m.tip_obj) && big > 0.0 && big.is_finite()) {
                    return Err(format!("memoria de lado invalida: {m:?}"));
                }
                // Un lado ya unitario y perpendicular (el publicado) se usa TAL CUAL: repetir el
                // cuadro (dt 0) no puede cambiar ni un bit.
                if (vlen(m.side) - 1.0).abs() <= 1e-12 && vdot(m.side, t).abs() <= 1e-12 {
                    Some((m.side, m.tip_obj))
                } else {
                    let s = vmul(m.side, 1.0 / big);
                    let s = vsub(s, vmul(t, vdot(s, t)));
                    let l = vlen(s);
                    if !(l.is_finite() && l > 1e-9) {
                        return Err(format!("memoria de lado paralela a la tangente: {m:?}"));
                    }
                    Some((vmul(s, 1.0 / l), m.tip_obj))
                }
            }
        };
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
        let r = vlen(d);
        let tip_eff = vadd(cp, d);
        let x = vdot(d, t);
        let yv = vsub(d, vmul(t, x));
        let y = vlen(yv);
        let target_side = (y > 1e-12 * r.max(1.0)).then(|| vmul(yv, 1.0 / y));
        // Primer codo: fraccion del giro plano equivalente (depende solo de (x, y): continuo en el polo).
        let (planar, _) = dubins([x, y, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], rho)
            .ok_or("punta dentro del circulo del codo")?;
        let th1 = SPATIAL_FIRST_ELBOW_FRACTION * planar;
        // Margen a la singularidad del segundo codo con el lado `m` (pi - angulo(t1, punta desde p1)).
        let margin_for = |m: V3| -> f64 {
            let r1 = axis_angle(vcross(t, m), th1);
            let p1 = vadd(vmul(t, rho * th1.sin()), vmul(m, rho * (1.0 - th1.cos())));
            let d2 = vsub(d, p1);
            let l = vlen(d2);
            if l > 0.0 {
                std::f64::consts::PI - (vdot(d2, mapply(r1, t)) / l).clamp(-1.0, 1.0).acos()
            } else {
                0.0
            }
        };
        let (side, side_rotation) = match (memory, target_side) {
            (None, Some(s)) => (s, 0.0),
            (None, None) => (perpendicular(t), 0.0),
            (Some((m, _)), None) => (m, 0.0),
            (Some((m, previous)), Some(s)) => {
                // Ajuste A1 (memoria perezosa por margen): gira lo MINIMO para mantener el margen a la
                // singularidad >= SPATIAL_SAFE_MARGIN, mas una deriva lenta hacia el lado de la punta, y
                // nunca mas que el tope por paso. En el polo trasero el margen sobra y casi no gira.
                let angle = vdot(m, s).clamp(-1.0, 1.0).acos();
                let step = vlen(vsub(tip_eff, previous));
                let unit = step / r;
                let cap = if unit.is_nan() {
                    angle
                } else {
                    SPATIAL_SIDE_RATE * unit
                };
                let sign = if vdot(vcross(m, s), t) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let rotate_side = |psi: f64| {
                    let rotated = mapply(axis_angle(t, sign * psi), m);
                    let rotated = vsub(rotated, vmul(t, vdot(rotated, t)));
                    vmul(rotated, 1.0 / vlen(rotated))
                };
                let need = if margin_for(m) >= SPATIAL_SAFE_MARGIN {
                    0.0
                } else if margin_for(rotate_side(angle)) < SPATIAL_SAFE_MARGIN {
                    angle
                } else {
                    let (mut lo, mut hi) = (0.0, angle);
                    for _ in 0..50 {
                        let mid = 0.5 * (lo + hi);
                        if margin_for(rotate_side(mid)) >= SPATIAL_SAFE_MARGIN {
                            hi = mid;
                        } else {
                            lo = mid;
                        }
                    }
                    hi
                };
                let drift = SPATIAL_SIDE_DRIFT * unit;
                let turn = angle.min(cap).min(if drift.is_nan() {
                    need
                } else {
                    need.max(drift)
                });
                // H-FIX01: el giro nunca gasta mas desplazamiento de malla que el presupuesto del cuadro.
                let turn = if turn > 0.0 {
                    self.memory_budget(d, m, previous, step, turn, &rotate_side)
                } else {
                    0.0
                };
                if turn == 0.0 {
                    // Sin giro: la memoria se conserva bit a bit (sin renormalizar).
                    (m, 0.0)
                } else {
                    (rotate_side(turn), sign * turn)
                }
            }
        };
        let g = self.geometry(d, side)?;
        let Geometry {
            th1,
            th2,
            line,
            arc,
            shaft,
            arc_part,
            line_part,
            ring_fraction,
            r1,
            r2,
            distal,
            end,
            axial_tip,
            ref frames,
            margin,
        } = g;
        let scalars = [
            th1,
            th2,
            line,
            arc,
            shaft,
            arc_part,
            line_part,
            ring_fraction,
            side_rotation,
            margin,
        ];
        if !(scalars.iter().all(|v| v.is_finite())
            && finite(side)
            && finite(distal)
            && finite(axial_tip)
            && proper_rotation(r1)
            && proper_rotation(r2)
            && frames
                .iter()
                .all(|(p, m)| finite(*p) && proper_rotation(*m)))
        {
            return Err("marcos o medidas del tramo no finitos o no rigidos".into());
        }
        let (kinds, corners, loop_offsets, _) = base.layout();
        let rest_normals = base.rest_normals();
        let vertex = |i: usize, kind: VertexKind| self.position(&g, i, kind);
        let normal = |c: usize, kind: CornerKind| match kind {
            CornerKind::Fixed => rest_normals[c],
            CornerKind::Distal => mapply(end, rest_normals[c]),
            CornerKind::Ring(j) => mapply(frames[j].1, rest_normals[c]),
        };
        // Validacion COMPLETA antes de publicar (LP-R03): finitud, normales rotadas con la MISMA norma que
        // en reposo (unitarias si el reposo lo es) y anillos rigidos (cada vertice de anillo a la misma
        // distancia de su centro que en el lazo de reposo).
        for (i, &kind) in kinds.iter().enumerate() {
            let v = vertex(i, kind);
            if !finite(v) {
                return Err(format!("vertice derivado {i} no finito"));
            }
            if let VertexKind::Ring { j, k } = kind {
                let l0 = vlen(loop_offsets[k]);
                if (vlen(vsub(v, frames[j].0)) - l0).abs() > 1e-9 * l0.max(1.0) {
                    return Err(format!("anillo {j} no rigido en el vertice {i}"));
                }
            }
        }
        for (c, &kind) in corners.iter().enumerate() {
            let n = normal(c, kind);
            if !(finite(n) && (vlen(n) - vlen(rest_normals[c])).abs() <= 1e-9) {
                return Err(format!(
                    "normal derivada {c} no finita o sin la norma de reposo"
                ));
            }
        }
        // Publicacion infalible: misma aritmetica determinista, sin E/S.
        for (i, &kind) in kinds.iter().enumerate() {
            vertices[i] = vertex(i, kind);
        }
        for (c, &kind) in corners.iter().enumerate() {
            normals[c] = normal(c, kind);
        }
        Ok(SpatialPose {
            tip_obj,
            first_elbow: th1,
            second_elbow: th2,
            turn: th1 + th2,
            side_rotation,
            singular_margin: margin,
            end_rotation: end,
            shaft_length: shaft,
            line_length: line,
            distal_center: distal,
            axial_tip,
            memory: SpatialMemory {
                side,
                tip_obj: tip_eff,
            },
        })
    }
}

/// Combinaciones de `--tentacle-spatial-pursuit` sin tocar archivos. Sin el flag: `Ok(None)` y las
/// reglas de siempre (persecucion viva / composicion observadora) no cambian. Mismos paquetes privados.
pub fn validate_spatial_pursuit_flags(
    spatial: bool,
    live: bool,
    capture: &TentacleCaptureFlags,
) -> Result<Option<LivePursuitRequest>, String> {
    if !spatial {
        return Ok(None);
    }
    if live {
        return Err("--tentacle-spatial-pursuit y --tentacle-live-pursuit son incompatibles (dos deformaciones del mismo brazo)".into());
    }
    if capture.enabled {
        return Err("--tentacle-spatial-pursuit y --tentacle-capture-debug son incompatibles (la persecucion espacial no usa la instantanea F7)".into());
    }
    if capture.start_time.is_some() {
        return Err("--tentacle-spatial-pursuit no admite --tentacle-capture-time (el reloj de la composicion observadora)".into());
    }
    if capture.legacy {
        return Err("--legacy no admite --tentacle-spatial-pursuit".into());
    }
    if !capture.solar {
        return Err("--tentacle-spatial-pursuit requiere --creature-solar-reveal (persecucion en la ruta solar)".into());
    }
    let need = |v: &Option<std::path::PathBuf>, flag: &str| {
        v.clone()
            .ok_or(format!("--tentacle-spatial-pursuit requiere {flag}"))
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

/// Lo verificado al enlazar (identidad de `SolarCaptureDebug` + reposo de la deformacion espacial).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialBindReport {
    pub capture: BindReport,
    pub rest_loop: RestLoop,
    pub bend_radius_obj: f64,
    pub tip_offset_obj: f64,
    /// Bola minima de esta familia (radio de exactitud espacial, OBJ).
    pub min_radius_obj: f64,
    pub max_reach_obj: f64,
    /// Vertices retenidos del tramo puestos en la posicion EXACTA del Meshy.
    pub exact_rest_vertices: usize,
}

/// Estado del ultimo paso publicado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialFrameStatus {
    pub step: PursuitStepReport,
    pub tip_world: Vec3,
    pub ship_world: Vec3,
    pub tip_ship_distance: f64,
    pub shaft: Option<SpatialPose>,
    pub world_per_obj: f64,
}

/// Persecucion espacial: composicion enlazada, deformacion espacial, controlador compartido y capa
/// `Body` propia (copia). En reposo NO hay capa: el cuadro es exactamente el de siempre.
pub struct SpatialPursuit {
    debug: SolarCaptureDebug,
    /// Geometria y particion exactas del bind (sin E/S ni punteros).
    bound_identity: Option<(Model, Model)>,
    rig: Option<SpatialRig>,
    composite: Option<Model>,
    prefix: Option<(usize, usize)>,
    controller: Option<PursuitController>,
    last: Option<SpatialFrameStatus>,
    /// Memoria de lado publicada (continuidad entre cuadros).
    memory: Option<SpatialMemory>,
    extra_bounds_obj: (Vec3, f64),
    drawn: bool,
    scratch_vertices: Vec<[f64; 3]>,
    scratch_normals: Vec<[f64; 3]>,
}

impl SpatialPursuit {
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
            memory: None,
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
    pub fn rig(&self) -> Option<&SpatialRig> {
        self.rig.as_ref()
    }
    pub fn controller(&self) -> Option<&PursuitController> {
        self.controller.as_ref()
    }
    pub fn last(&self) -> Option<SpatialFrameStatus> {
        self.last
    }
    pub fn memory(&self) -> Option<SpatialMemory> {
        self.memory
    }
    pub fn composite(&self) -> Option<&Model> {
        self.composite.as_ref()
    }
    pub fn body_prefix(&self) -> Option<(usize, usize)> {
        self.prefix
    }
    /// Enlaza con la criatura del World: identidad y procedencia de `SolarCaptureDebug` (preparadas
    /// sin publicar), reposo EXACTO del Meshy y COPIA de la capa enlazada. Con `Err` nada cambia.
    pub fn bind(&mut self, world: &World) -> Result<SpatialBindReport, String> {
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
        let rig = SpatialRig::new(self.debug.package().data(), &arm_rest)?;
        let (layer, nv, nn) = prepared.layer();
        let (gv, gn) = (rig.rest_vertices().len(), rig.rest_normals().len());
        if layer.vertices.len() != nv + gv || layer.normals.len() != nn + gn {
            return Err("capa enlazada sin la plantilla del tramo al final".into());
        }
        let mut composite = copy_model(layer);
        composite.vertices[nv..].copy_from_slice(rig.rest_vertices());
        composite.normals[nn..].copy_from_slice(rig.rest_normals());
        let exact = rig
            .base()
            .layout()
            .0
            .iter()
            .filter(|k| !matches!(k, VertexKind::Ring { .. }))
            .count();
        let report = SpatialBindReport {
            capture,
            rest_loop: rig.rest_loop(),
            bend_radius_obj: rig.bend_radius(),
            tip_offset_obj: rig.tip_offset(),
            min_radius_obj: rig.min_radius(),
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
        // Publicacion (sin operaciones falibles desde aqui).
        self.debug.publish_bind(prepared);
        self.bound_identity = Some(identity);
        self.scratch_vertices = rig.rest_vertices().to_vec();
        self.scratch_normals = rig.rest_normals().to_vec();
        self.rig = Some(rig);
        self.composite = Some(composite);
        self.prefix = Some((nv, nn));
        self.controller = None;
        self.last = None;
        self.memory = None;
        self.extra_bounds_obj = (Vec3::ZERO, 0.0);
        self.drawn = false;
        Ok(report)
    }
    /// Identidad EXACTA del bind (criatura y particion): la misma comprobacion con la que abre `step`,
    /// sin efectos. La ruta congelada (terminal/reinicio) la reutiliza para no aceptar un World que la
    /// ruta activa rechaza.
    pub(crate) fn check_bound_identity(&self, world: &World) -> Result<(), String> {
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
        Ok(())
    }
    /// Un paso con el estado ACTUAL del World (nave, transform de la criatura, O2 mostrado) y `dt` (s).
    /// Valida y calcula TODO en buffers de trabajo y publica al final: con `Err` controlador, memoria,
    /// capa, esfera y estado no cambian. Nunca escribe el World.
    pub fn step(&mut self, world: &World, dt: f64) -> Result<SpatialFrameStatus, String> {
        let rig = self.rig.as_ref().ok_or("persecucion sin enlazar (bind)")?;
        self.check_bound_identity(world)?;
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
        let anchor = c.transform_point(Vec3::from_array(rig.rest_loop().center));
        let rest_tip = c.transform_point(Vec3::from_array(rig.rest_tip()));
        let input = PursuitInput {
            ship: world.ship.position,
            anchor,
            rest_tip,
            oxygen01: world.debug_oxygen01,
            dt,
            min_radius: rig.min_radius() * scale,
            max_radius: MAX_REACH_OBJ * scale,
        };
        if !(in_domain(anchor) && in_domain(rest_tip)) {
            return Err("anclaje o punta de reposo fuera del dominio numerico".into());
        }
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
            let rest = rig.rest_loop();
            let d = vsub(tip_obj, rest.center);
            let r = vlen(d);
            if r < rig.tip_offset() {
                let dir = if r > 0.0 {
                    vmul(d, 1.0 / r)
                } else {
                    rest.tangent
                };
                tip_obj = vadd(rest.center, vmul(dir, rig.tip_offset()));
            }
            let pose = rig.deform(
                tip_obj,
                self.memory,
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
        let status = SpatialFrameStatus {
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
            self.memory = Some(pose.memory);
        } else if !draw {
            self.memory = None;
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
            return "PERSECUCION ESPACIAL 3D (prototipo, sin agarre): sin paso".into();
        };
        let mode = match s.step.mode {
            PursuitMode::Idle => "reposo (O2 > 15 %)",
            PursuitMode::Pursuing => "persigue",
            PursuitMode::Retracting => "recoge",
        };
        format!(
            "PERSECUCION ESPACIAL 3D (prototipo, sin agarre) | {mode} | v max {:.0} u/s | punta-nave {:.0} u{}{}",
            s.step.max_speed,
            s.tip_ship_distance,
            s.shaft.map_or(String::new(), |p| format!(
                " | codos {:.0}+{:.0} grados",
                p.first_elbow.to_degrees(),
                p.second_elbow.to_degrees()
            )),
            if s.step.target_clamped {
                " | nave fuera de alcance/bola: objetivo recortado"
            } else {
                ""
            }
        )
    }
}
