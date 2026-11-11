//! GT1 P1: preparacion LOCAL autorada y cierre del clip aprobado, desde el Contact real de B1. Ver
//! `docs/tentacle-grasp-closure.md`. Nucleo puro: no lo llaman `app`/`main`, no escribe World, nave,
//! camara, controlador ni persecucion; sin arrastre, render ni ventana.
//!
//! **H-P1-01**: por cuadro autorado `f` (muestreo floor del paquete, de 'reposo' a `grip_frame`), la
//! forma LOCAL del distal del clip (normalizada por el marco rigido del lazo de corte, Horn/Jacobi) menos
//! la de la referencia se suma al distal publicado en el marco final del solver; el corte queda FIJO (seam
//! = anillo N del solver); las normales de esquina D son las autoradas en ese marco (con su norma de
//! reposo). En cada cuadro autorado nuevo la punta se recalibra con la memoria entrante para que el
//! centroide grip de la MALLA FINAL quede en el ancla 168 (carrier anclado, tolerancia de A).
use crate::{
    math::{Mat4, Vec3},
    visual::{
        tentacle_grasp_handoff::{GraspHandoff, HandoffPhase},
        tentacle_grasp_transition::{CONTACT_TOLERANCE_OBJ, ContactAdapter},
        tentacle_growth_cpu::{GrowthPackage, GrowthVertex, Part, Side},
        tentacle_live_pursuit::{MAX_PURSUIT_DT, MAX_REACH_OBJ, copy_model},
        tentacle_spatial_pursuit::{SpatialMemory, SpatialPose},
    },
};

type V3 = [f64; 3];
type Q = [f64; 4];
/// Gap relativo minimo entre los dos autovalores mayores de Horn: por debajo el marco del corte es ambiguo.
const MIN_FRAME_GAP: f64 = 1e-3;
/// Compatibilidad exigida entre la referencia autorada y el reposo del rig (export ~1e-6 medido).
const REFERENCE_TOLERANCE_OBJ: f64 = 1e-5;
/// Carrier anclado: tope de iteraciones de Newton por cuadro (d02: <= 8 bastan), precision de parada,
/// paso del Jacobiano y mitades de la busqueda lineal. Valores CANDIDATOS; la aceptacion es la de A.
const CLOSURE_MAX_ITERATIONS: usize = 16;
const CLOSURE_PRECISION_OBJ: f64 = 1e-10;
const JACOBIAN_STEP_OBJ: f64 = 1e-6;
const LINE_SEARCH_HALVINGS: usize = 12;

fn v(a: V3) -> Vec3 {
    Vec3::from_array(a)
}
fn arr(a: Vec3) -> V3 {
    [a.x, a.y, a.z]
}
fn finite_all(points: &[V3]) -> bool {
    points.iter().all(|p| p.iter().all(|x| x.is_finite()))
}
fn qrot(q: Q, p: Vec3) -> Vec3 {
    let u = Vec3::new(q[1], q[2], q[3]);
    let t = u.cross(p) * 2.0;
    p + t * q[0] + u.cross(t)
}
fn qconj(q: Q) -> Q {
    [q[0], -q[1], -q[2], -q[3]]
}
fn rows(m: [[f64; 3]; 3], p: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
        m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
        m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
    )
}
fn same_bits(a: &Mat4, b: &Mat4) -> bool {
    a.m.iter()
        .flatten()
        .zip(b.m.iter().flatten())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}
fn centroid(points: &[V3], ids: &[usize]) -> Vec3 {
    let mut s = Vec3::ZERO;
    for &i in ids {
        s += v(points[i]);
    }
    s * (1.0 / ids.len() as f64)
}
/// Desplazamiento maximo con finitud EXPLICITA (un NaN no se esconde tras `max`).
fn max_step(a: &[V3], b: &[V3]) -> Option<f64> {
    let mut m: f64 = 0.0;
    for (p, q) in a.iter().zip(b) {
        let d = (v(*p) - v(*q)).length();
        if !d.is_finite() {
            return None;
        }
        m = m.max(d);
    }
    Some(m)
}

/// Autovalores y autovectores (columnas) de una simetrica 4x4 por Jacobi ciclico.
#[allow(clippy::needless_range_loop)] // aritmetica de indices de la rotacion de Jacobi
fn jacobi4(mut a: [[f64; 4]; 4]) -> ([f64; 4], [[f64; 4]; 4]) {
    let mut e = [[0.0; 4]; 4];
    for (i, row) in e.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    let scale: f64 = a.iter().flatten().map(|x| x * x).sum();
    for _ in 0..64 {
        let mut off = 0.0;
        for i in 0..4 {
            for j in 0..4 {
                if i != j {
                    off += a[i][j] * a[i][j];
                }
            }
        }
        if off.is_nan() || off <= 1e-30 * scale {
            break;
        }
        for p in 0..4 {
            for q in p + 1..4 {
                if a[p][q] == 0.0 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = if theta == 0.0 {
                    1.0
                } else {
                    theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt())
                };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for row in a.iter_mut() {
                    let (x, y) = (row[p], row[q]);
                    row[p] = c * x - s * y;
                    row[q] = s * x + c * y;
                }
                for k in 0..4 {
                    let (x, y) = (a[p][k], a[q][k]);
                    a[p][k] = c * x - s * y;
                    a[q][k] = s * x + c * y;
                }
                for row in e.iter_mut() {
                    let (x, y) = (row[p], row[q]);
                    row[p] = c * x - s * y;
                    row[q] = s * x + c * y;
                }
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2], a[3][3]], e)
}

/// Ajuste rigido de Horn `b ~ cb + q (a - ca)` con comprobacion de finitud y de marco bien determinado.
#[allow(clippy::needless_range_loop)] // matriz de covarianza por indices
fn horn(a: &[Vec3], b: &[Vec3]) -> Result<(Q, Vec3, Vec3), String> {
    let n = a.len() as f64;
    let ca = a.iter().fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / n);
    let cb = b.iter().fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / n);
    let mut s = [[0.0; 3]; 3];
    for (p, q) in a.iter().zip(b) {
        let (x, y) = (*p - ca, *q - cb);
        let (x, y) = ([x.x, x.y, x.z], [y.x, y.y, y.z]);
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += x[i] * y[j];
            }
        }
    }
    let m = [
        [
            s[0][0] + s[1][1] + s[2][2],
            s[1][2] - s[2][1],
            s[2][0] - s[0][2],
            s[0][1] - s[1][0],
        ],
        [
            s[1][2] - s[2][1],
            s[0][0] - s[1][1] - s[2][2],
            s[0][1] + s[1][0],
            s[2][0] + s[0][2],
        ],
        [
            s[2][0] - s[0][2],
            s[0][1] + s[1][0],
            -s[0][0] + s[1][1] - s[2][2],
            s[1][2] + s[2][1],
        ],
        [
            s[0][1] - s[1][0],
            s[2][0] + s[0][2],
            s[1][2] + s[2][1],
            -s[0][0] - s[1][1] + s[2][2],
        ],
    ];
    if !(m.iter().flatten().all(|x| x.is_finite()) && ca.is_finite() && cb.is_finite()) {
        return Err("marco del corte no finito".into());
    }
    let (w, e) = jacobi4(m);
    let mut order = [0, 1, 2, 3];
    order.sort_by(|i, j| w[*j].total_cmp(&w[*i]));
    let (top, second) = (w[order[0]], w[order[1]]);
    let gap = (top - second) / top.abs();
    if !(gap.is_finite() && gap >= MIN_FRAME_GAP) {
        return Err(format!("marco del corte ambiguo (gap {gap})"));
    }
    let k = order[0];
    let mut q: Q = [e[0][k], e[1][k], e[2][k], e[3][k]];
    let l = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !(l.is_finite() && l > 0.0) {
        return Err("cuaternion del corte degenerado".into());
    }
    q = q.map(|x| x / l);
    if q[0] < 0.0 {
        q = q.map(|x| -x);
    }
    Ok((q, ca, cb))
}

fn stage(package: &GrowthPackage, name: &str) -> Result<usize, String> {
    package
        .capture()
        .manifest()
        .stages()
        .iter()
        .find(|(_, s)| s == name)
        .map(|(f, _)| *f)
        .ok_or(format!("el manifest no tiene el marcador '{name}'"))
}

/// Forma local de un cuadro: desplazamiento (respecto a la referencia) de los vertices D movibles y
/// normales de esquina D, ambos en coordenadas de reposo del distal.
struct Local {
    disp: Vec<Vec3>,
    normals: Vec<Vec3>,
}

/// Clip de cierre ligado de forma inseparable a su paquete, reposo y adaptador de contacto.
pub struct ClosureClip<'p> {
    package: &'p GrowthPackage,
    adapter: ContactAdapter,
    reference: usize,
    closing: usize,
    grip: usize,
    /// Vertices D fuera del lazo de corte (reciben el desplazamiento autorado).
    moved: Vec<usize>,
    /// Esquinas de caras D (reciben las normales autoradas).
    dcorners: Vec<usize>,
    loop_d: Vec<usize>,
    rest_loop: Vec<Vec3>,
    /// Posiciones locales de la referencia para `moved`.
    reference_local: Vec<Vec3>,
}

impl<'p> ClosureClip<'p> {
    /// Construye y POSEE el adaptador de contacto del MISMO paquete y reposo; lee y valida marcadores,
    /// comprueba o(f) = 0 y que la forma local de la referencia es la del reposo del rig. Sin efectos.
    pub fn new(package: &'p GrowthPackage, arm_rest: &[[f64; 3]]) -> Result<Self, String> {
        let adapter = ContactAdapter::new(package, arm_rest)?;
        let manifest = package.capture().manifest();
        let reference = stage(package, "reposo")?;
        let closing = stage(package, "cierre")?;
        let gripping = stage(package, "agarre")?;
        let drag = stage(package, "arrastre")?;
        let grip = manifest.grip_frame();
        if !(1 <= reference
            && reference < closing
            && closing < grip
            && grip <= gripping
            && gripping < drag
            && drag <= package.capture().frames())
        {
            return Err(format!(
                "marcadores incompatibles: reposo {reference}, cierre {closing}, grip_frame {grip}, agarre {gripping}, arrastre {drag}"
            ));
        }
        let topo = package.data().topology();
        let rings = topo.rings();
        if rings.last().map(|r| r.as_slice()) != Some(topo.loop_d()) {
            return Err("el ultimo anillo no es el lazo de corte D".into());
        }
        let loop_d = topo.loop_d().to_vec();
        let is_loop = |i: usize| loop_d.contains(&i);
        let moved: Vec<usize> = topo
            .vertices()
            .iter()
            .enumerate()
            .filter(|(i, x)| {
                matches!(x, GrowthVertex::Retained { side: Side::D, .. }) && !is_loop(*i)
            })
            .map(|(i, _)| i)
            .collect();
        if !topo.grip().iter().all(|i| moved.binary_search(i).is_ok()) {
            return Err("el grip no esta en el distal movible".into());
        }
        let dcorners: Vec<usize> = topo
            .faces()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.part == Part::D)
            .flat_map(|(fi, _)| [3 * fi, 3 * fi + 1, 3 * fi + 2])
            .collect();
        let rest = adapter.rig().rest_vertices();
        let rest_loop: Vec<Vec3> = loop_d.iter().map(|&i| v(rest[i])).collect();
        let mut clip = Self {
            package,
            adapter,
            reference,
            closing,
            grip,
            moved,
            dcorners,
            loop_d,
            rest_loop,
            reference_local: Vec::new(),
        };
        let (positions, normals) = clip.absolute_local(reference)?;
        let rest = clip.adapter.rig().rest_vertices();
        let rest_n = clip.adapter.rig().rest_normals();
        let dp = positions
            .iter()
            .zip(&clip.moved)
            .map(|(p, &i)| (*p - v(rest[i])).length())
            .fold(0.0, f64::max);
        let dn = normals
            .iter()
            .zip(&clip.dcorners)
            .map(|(p, &c)| (*p - v(rest_n[c])).length())
            .fold(0.0, f64::max);
        if !(dp <= REFERENCE_TOLERANCE_OBJ && dn <= REFERENCE_TOLERANCE_OBJ) {
            return Err(format!(
                "la referencia autorada no coincide con el reposo del rig (posicion {dp}, normal {dn}): enlace ajeno"
            ));
        }
        clip.reference_local = positions;
        Ok(clip)
    }
    /// Posiciones locales de `moved` y normales locales de `dcorners` del cuadro `f` (marco del corte).
    fn absolute_local(&self, f: usize) -> Result<(Vec<Vec3>, Vec<Vec3>), String> {
        let data = self.package.data();
        let mut scratch = copy_model(data.template());
        let pose = data.evaluate(f, [0.0; 3], &mut scratch)?;
        if pose.law.offset != [0.0; 3] {
            return Err(format!(
                "o({f}) != 0 con delta 0: el clip no es la pose aprobada"
            ));
        }
        if !(finite_all(&scratch.vertices) && finite_all(&scratch.normals)) {
            return Err(format!("cuadro {f} no finito"));
        }
        let lf: Vec<Vec3> = self
            .loop_d
            .iter()
            .map(|&i| v(scratch.vertices[i]))
            .collect();
        let (q, ca, cb) = horn(&self.rest_loop, &lf)?;
        let qi = qconj(q);
        let positions = self
            .moved
            .iter()
            .map(|&i| ca + qrot(qi, v(scratch.vertices[i]) - cb))
            .collect();
        // Direccion AUTORADA del cuadro con la norma de reposo de la esquina (las normales del paquete son
        // las crudas del OBJ, sin normalizar; el rig publica cada normal con su norma de reposo).
        let rest_n = self.adapter.rig().rest_normals();
        let mut normals = Vec::with_capacity(self.dcorners.len());
        for &c in &self.dcorners {
            let raw = v(scratch.normals[c]);
            let (len, rest_len) = (raw.length(), v(rest_n[c]).length());
            if !(len.is_finite() && len > 0.0 && rest_len.is_finite()) {
                return Err(format!(
                    "normal autorada degenerada en la esquina {c} del cuadro {f}"
                ));
            }
            normals.push(qrot(qi, raw * (rest_len / len)));
        }
        Ok((positions, normals))
    }
    fn local(&self, f: usize) -> Result<Local, String> {
        let (positions, normals) = self.absolute_local(f)?;
        let disp = positions
            .iter()
            .zip(&self.reference_local)
            .map(|(p, r)| *p - *r)
            .collect();
        Ok(Local { disp, normals })
    }
    pub fn adapter(&self) -> &ContactAdapter {
        &self.adapter
    }
    pub fn package(&self) -> &'p GrowthPackage {
        self.package
    }
    pub fn reference_frame(&self) -> usize {
        self.reference
    }
    pub fn closing_frame(&self) -> usize {
        self.closing
    }
    pub fn grip_frame(&self) -> usize {
        self.grip
    }
    pub fn duration_s(&self) -> f64 {
        (self.grip - self.reference) as f64 / self.package.capture().fps() as f64
    }
    fn phase(&self, f: usize) -> ClosurePhase {
        if f >= self.grip {
            ClosurePhase::Gripped
        } else if f >= self.closing {
            ClosurePhase::Closing
        } else {
            ClosurePhase::Preparing
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosurePhase {
    /// Preparacion local autorada (referencia .. cierre).
    Preparing,
    /// Cierre autorado (cierre .. grip_frame).
    Closing,
    /// grip_frame publicado; hold determinista.
    Gripped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosureStatus {
    pub phase: ClosurePhase,
    /// Cuadro autorado publicado (1-based del clip).
    pub frame: usize,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub axial_target_obj: [f64; 3],
    /// Distancia del centroide grip PUBLICADO al ancla transportada (OBJ).
    pub contact_error_obj: f64,
    pub last_step_obj: f64,
    pub iterations: usize,
}

pub struct GraspClosure<'c, 'p> {
    clip: &'c ClosureClip<'p>,
    ship: Mat4,
    body: Mat4,
    desired: Vec3,
    degenerate_at_start: Vec<bool>,
    vertices: Vec<V3>,
    normals: Vec<V3>,
    pose: SpatialPose,
    status: ClosureStatus,
}

/// Area nula, negativa o no finita (NaN incluido).
fn degenerate(area: f64) -> bool {
    area.is_nan() || area <= 0.0
}

fn face_area(points: &[V3], f: [usize; 3]) -> f64 {
    let [a, b, c] = f.map(|i| v(points[i]));
    (b - a).cross(c - a).length()
}

impl<'c, 'p> GraspClosure<'c, 'p> {
    /// Inicia P1 sobre el Contact PUBLICADO por B1. Exige que B1 se creara con el adaptador de ESTE clip
    /// (identidad, no tamanos), fase Contact y que el rig reproduzca los buffers bit a bit con su punta y
    /// memoria. El primer cuadro es una COPIA exacta (buffers, pose, memoria). Con `Err` no se crea nada.
    pub fn start(clip: &'c ClosureClip<'p>, handoff: &GraspHandoff) -> Result<Self, String> {
        if !std::ptr::eq(handoff.adapter(), &clip.adapter) {
            return Err(
                "el traspaso B1 no se creo con el adaptador de este clip (enlace ajeno); nada iniciado"
                    .into(),
            );
        }
        let s = handoff.status();
        let contact = match (s.phase, s.contact) {
            (HandoffPhase::Contact, Some(c)) => c,
            _ => return Err("el traspaso B1 no ha publicado su Contact; nada iniciado".into()),
        };
        let rig = clip.adapter.rig();
        let pose = handoff.pose();
        let (mut v, mut n) = (handoff.vertices().to_vec(), handoff.normals().to_vec());
        rig.deform(pose.tip_obj, Some(pose.memory), &mut v, &mut n)
            .map_err(|e| format!("Contact no reproducible: {e}"))?;
        if v != handoff.vertices() || n != handoff.normals() {
            return Err("el rig del clip no reproduce el Contact publicado; nada iniciado".into());
        }
        let (ship, body) = handoff.transforms();
        let desired = Vec3::from_array(clip.adapter.anchor().desired_obj(&ship, &body)?);
        let degenerate_at_start = clip
            .package
            .data()
            .topology()
            .faces()
            .iter()
            .map(|f| degenerate(face_area(handoff.vertices(), f.v)))
            .collect();
        Ok(Self {
            clip,
            ship,
            body,
            desired,
            degenerate_at_start,
            vertices: handoff.vertices().to_vec(),
            normals: handoff.normals().to_vec(),
            pose,
            status: ClosureStatus {
                phase: ClosurePhase::Preparing,
                frame: clip.reference,
                elapsed_s: 0.0,
                duration_s: clip.duration_s(),
                axial_target_obj: pose.tip_obj,
                contact_error_obj: contact.contact_error_obj,
                last_step_obj: 0.0,
                iterations: 0,
            },
        })
    }

    /// Composicion en temporales: `deform(t, memoria)` + desplazamiento local autorado del distal (corte
    /// fijo) y normales autoradas D, en el marco final del solver.
    fn compose(
        &self,
        local: &Local,
        t: V3,
        memory: SpatialMemory,
    ) -> Result<(Vec<V3>, Vec<V3>, SpatialPose), String> {
        let rig = self.clip.adapter.rig();
        let (mut verts, mut norms) = (self.vertices.clone(), self.normals.clone());
        let pose = rig.deform(t, Some(memory), &mut verts, &mut norms)?;
        let e = pose.end_rotation;
        for (d, &i) in local.disp.iter().zip(&self.clip.moved) {
            verts[i] = arr(v(verts[i]) + rows(e, *d));
        }
        for (m, &c) in local.normals.iter().zip(&self.clip.dcorners) {
            norms[c] = arr(rows(e, *m));
        }
        Ok((verts, norms, pose))
    }

    /// Carrier anclado (H-P1-01): punta `T` tal que el centroide grip de la malla FINAL compuesta del
    /// cuadro quede en el ancla, con la MEMORIA ENTRANTE real. Newton acotado con Jacobiano por diferencias
    /// finitas de la MISMA composicion, guardas de determinante/finitud y busqueda lineal monotona. Devuelve
    /// la mejor punta encontrada; la aceptacion (<= tolerancia) se mide despues sobre lo que se publica.
    fn calibrate(&self, local: &Local, memory: SpatialMemory) -> Result<(V3, usize), String> {
        let grip = self.clip.adapter.grip();
        let contact = |t: Vec3| -> Option<Vec3> {
            let (verts, _, _) = self.compose(local, arr(t), memory).ok()?;
            let c = centroid(&verts, grip);
            c.is_finite().then_some(c)
        };
        let mut t = v(self.status.axial_target_obj);
        let mut c =
            contact(t).ok_or("composicion no evaluable en la punta previa; nada publicado")?;
        let mut error = (c - self.desired).length();
        let mut iterations = 0;
        while error > CLOSURE_PRECISION_OBJ && iterations < CLOSURE_MAX_ITERATIONS {
            let mut j = [Vec3::ZERO; 3];
            let mut ok = true;
            for (k, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
                match contact(t + axis * JACOBIAN_STEP_OBJ) {
                    Some(ck) => j[k] = (ck - c) * (1.0 / JACOBIAN_STEP_OBJ),
                    None => ok = false,
                }
            }
            let det = j[0].dot(j[1].cross(j[2]));
            let scale = j[0].length() * j[1].length() * j[2].length();
            if !(ok && det.is_finite() && scale.is_finite() && det.abs() > 1e-12 * scale) {
                break;
            }
            let rhs = self.desired - c;
            let dx = Vec3::new(
                rhs.dot(j[1].cross(j[2])) / det,
                j[0].dot(rhs.cross(j[2])) / det,
                j[0].dot(j[1].cross(rhs)) / det,
            );
            if !dx.is_finite() {
                break;
            }
            let mut step = 1.0;
            let mut accepted = None;
            for _ in 0..=LINE_SEARCH_HALVINGS {
                let candidate = t + dx * step;
                if let Some(cc) = contact(candidate) {
                    let e = (cc - self.desired).length();
                    if e < error {
                        accepted = Some((candidate, cc, e));
                        break;
                    }
                }
                step *= 0.5;
            }
            let Some((candidate, cc, e)) = accepted else {
                break;
            };
            (t, c, error) = (candidate, cc, e);
            iterations += 1;
        }
        Ok((arr(t), iterations))
    }

    /// Validacion COMPLETA antes de publicar; devuelve (error de contacto, salto max).
    fn validate(&self, vertices: &[V3], normals: &[V3], t: V3) -> Result<(f64, f64), String> {
        let rig = self.clip.adapter.rig();
        if !(finite_all(vertices) && finite_all(normals)) {
            return Err("malla compuesta no finita; nada publicado".into());
        }
        if normals.iter().zip(rig.rest_normals()).any(|(a, b)| {
            let d = (v(*a).length() - v(*b).length()).abs();
            d.is_nan() || d > 1e-9
        }) {
            return Err("normal compuesta sin la norma de reposo; nada publicado".into());
        }
        if self
            .clip
            .adapter
            .root()
            .iter()
            .any(|&i| vertices[i] != rig.rest_vertices()[i])
        {
            return Err("la raiz no quedo exacta; nada publicado".into());
        }
        let faces = self.clip.package.data().topology().faces();
        for (k, f) in faces.iter().enumerate() {
            if !self.degenerate_at_start[k] && degenerate(face_area(vertices, f.v)) {
                return Err(format!(
                    "cara {k} degenerada por la composicion; nada publicado"
                ));
            }
        }
        let r = (v(t) - v(rig.rest_loop().center)).length();
        if !(r >= rig.min_radius() * (1.0 - 1e-12) && r <= MAX_REACH_OBJ) {
            return Err(format!(
                "punta a {r} OBJ del lazo fuera del dominio; nada publicado"
            ));
        }
        let error = (centroid(vertices, self.clip.adapter.grip()) - self.desired).length();
        let step = max_step(vertices, &self.vertices).ok_or("salto no finito; nada publicado")?;
        if !error.is_finite() {
            return Err("contacto no finito; nada publicado".into());
        }
        Ok((error, step))
    }

    /// Avanza el reloj PROPIO del estudio `dt` s (finito, en `[0, MAX_PURSUIT_DT]`) con las MISMAS
    /// matrices capturadas por B1 (bit a bit). El cuadro autorado sale de `CapturePackage::frame_at`
    /// (floor) desde 'reposo', recortado a `grip_frame`; un cuadro repetido solo avanza el reloj; en
    /// `grip_frame` hold. Todo se prepara en temporales: con `Err` nada cambia ni el reloj avanza.
    pub fn advance(&mut self, ship: &Mat4, body: &Mat4, dt: f64) -> Result<ClosureStatus, String> {
        if !(dt.is_finite() && (0.0..=MAX_PURSUIT_DT).contains(&dt)) {
            return Err(format!(
                "dt invalido (finito en [0, {MAX_PURSUIT_DT}]): {dt}; nada publicado"
            ));
        }
        if !(same_bits(ship, &self.ship) && same_bits(body, &self.body)) {
            return Err(
                "transform de nave/cuerpo distinto del capturado (limite del estudio); nada publicado"
                    .into(),
            );
        }
        if self.status.phase == ClosurePhase::Gripped || dt == 0.0 {
            return Ok(self.status);
        }
        let duration = self.status.duration_s;
        let elapsed = (self.status.elapsed_s + dt).min(duration);
        if elapsed == self.status.elapsed_s {
            return Ok(self.status);
        }
        let capture = self.clip.package.capture();
        let frame = if elapsed >= duration {
            self.clip.grip
        } else {
            capture
                .frame_at((self.clip.reference - 1) as f64 / capture.fps() as f64 + elapsed)?
                .clamp(self.clip.reference, self.clip.grip)
        };
        if frame == self.status.frame {
            self.status.elapsed_s = elapsed;
            self.status.last_step_obj = 0.0;
            return Ok(self.status);
        }
        let local = self.clip.local(frame)?;
        let (t, iterations) = self.calibrate(&local, self.pose.memory)?;
        let (vertices, normals, pose) = self.compose(&local, t, self.pose.memory)?;
        let (error, step) = self.validate(&vertices, &normals, t)?;
        if error > CONTACT_TOLERANCE_OBJ {
            return Err(format!(
                "contacto sin converger en el cuadro {frame}: {error} OBJ tras {iterations} iteraciones; nada publicado"
            ));
        }
        // Publicacion conjunta (sin operaciones falibles desde aqui).
        self.vertices = vertices;
        self.normals = normals;
        self.pose = pose;
        self.status = ClosureStatus {
            phase: self.clip.phase(frame),
            frame,
            elapsed_s: elapsed,
            duration_s: duration,
            axial_target_obj: t,
            contact_error_obj: error,
            last_step_obj: step,
            iterations,
        };
        Ok(self.status)
    }
    pub fn vertices(&self) -> &[[f64; 3]] {
        &self.vertices
    }
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.normals
    }
    pub fn pose(&self) -> SpatialPose {
        self.pose
    }
    pub fn memory(&self) -> SpatialMemory {
        self.pose.memory
    }
    pub fn status(&self) -> ClosureStatus {
        self.status
    }
}

// ---- GT1-CARRY (aditivo, solo lectura; quitar este bloque restaura P1 byte a byte) ----

/// Forma CERRADA retenida de un P1 en `Gripped` para el arrastre: copia CONGELADA del cierre (mismo clip,
/// buffers, pose, memoria, ancla y caras degeneradas de inicio) mas la forma local de `grip_frame`. Evalua
/// con la MISMA `compose` + `validate` de P1 sin tocar el cierre vivo ni su reloj.
pub struct HeldClosure<'c, 'p> {
    frozen: GraspClosure<'c, 'p>,
    local: Local,
}

impl<'c, 'p> GraspClosure<'c, 'p> {
    /// Instantanea de la forma cerrada; `Err` si el cierre no publico su `grip_frame`.
    pub fn held(&self) -> Result<HeldClosure<'c, 'p>, String> {
        if self.status.phase != ClosurePhase::Gripped || self.status.frame != self.clip.grip {
            return Err("el cierre P1 no esta en Gripped; nada retenido".into());
        }
        let local = self.clip.local(self.status.frame)?;
        Ok(HeldClosure {
            frozen: GraspClosure {
                clip: self.clip,
                ship: self.ship,
                body: self.body,
                desired: self.desired,
                degenerate_at_start: self.degenerate_at_start.clone(),
                vertices: self.vertices.clone(),
                normals: self.normals.clone(),
                pose: self.pose,
                status: self.status,
            },
            local,
        })
    }
}

impl<'c, 'p> HeldClosure<'c, 'p> {
    pub fn clip(&self) -> &'c ClosureClip<'p> {
        self.frozen.clip
    }
    /// Buffers y pose EXACTOS del cierre en el instante de la instantanea.
    pub fn vertices(&self) -> &[[f64; 3]] {
        &self.frozen.vertices
    }
    pub fn normals(&self) -> &[[f64; 3]] {
        &self.frozen.normals
    }
    pub fn pose(&self) -> SpatialPose {
        self.frozen.pose
    }
    pub fn status(&self) -> ClosureStatus {
        self.frozen.status
    }
    /// Forma cerrada en la punta `tip_obj` con la memoria `memory`: `compose` de P1 y `validate` COMPLETA de
    /// P1 (finitud, norma de normales, raiz exacta, sin caras degeneradas nuevas, dominio de la punta). Devuelve
    /// (vertices, normales, pose, salto max respecto a la instantanea). Sin efectos; `Err` sin publicar nada.
    #[allow(clippy::type_complexity)] // tupla de publicacion de P1
    pub fn evaluate(
        &self,
        tip_obj: [f64; 3],
        memory: SpatialMemory,
    ) -> Result<(Vec<V3>, Vec<V3>, SpatialPose, f64), String> {
        let (vertices, normals, pose) = self.frozen.compose(&self.local, tip_obj, memory)?;
        let (_, step) = self.frozen.validate(&vertices, &normals, tip_obj)?;
        Ok((vertices, normals, pose, step))
    }
}
