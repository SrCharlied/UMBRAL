//! UMB-311 preview CPU AISLADA del TRAMO DE CRECIMIENTO DEDICADO (aprobado por Charlie: «Aprobada, me
//! gusta»). Port FIEL de la ley `fixed` de `tentacle_growth_law_fix_01/tools/growth_lib.py` (sha 88f5ab...)
//! sobre la topologia aprobada (4801 v / 9585 f, 52 puntos de corte, 64 intervalos), dentro del `World`
//! real. No es la captura del juego ni define trigger, rango o reglas de juego. Ver
//! `docs/tentacle-growth-cpu-preview.md`.
//!
//! - **Objetivo:** POSICION de `World.ship` tomada en `plan` (instantanea; sin persecucion). La
//!   orientacion debe ser la de referencia: 6DoF general NO implementado (error explicito).
//! - **Ley:** `P` = pose aprobada; distal/agarre/espiral = pose + `o(f)`; raiz fija. `|o| = r(f)|Delta|`;
//!   la DIRECCION de `o` se limita a un cono alrededor de la tangente `T(f)` del lazo (Hermite de
//!   tangentes `T|o|`, radio minimo MUESTREADO con 2001 puntos, `K = 1`, saturacion C1 con rodilla 0.8).
//!   En el agarre `o = Delta` EXACTO o `Err` (dominio explicito); al final `o = 0`.
//! - **Nave:** antes del agarre, la instantanea EXACTA del plan; desde el agarre, la matriz del paquete
//!   + el MISMO `o(f)` (espacio OBJ), misma rotacion y escala.
//! - **Referencia (`Delta` = 0 exacto):** delega en la captura ORIGINAL (`capture::CapturePreview`), sin
//!   malla nueva dormida.
//! - **Tiempo:** cuadro base = `piso(t_base * fps) + 1`; malla y nave del MISMO cuadro, sin poses
//!   intermedias. Con objetivos lejanos, alcance y arrastre se estiran `sqrt(max(1, d/d0))`
//!   (retiming PROVISIONAL de preview, no regla de juego).
// Los bucles por indice reproducen el orden de operaciones de la ley Python (paridad bit a bit).
#![allow(clippy::needless_range_loop)]

#[allow(dead_code)]
#[path = "tentacle_capture_cpu.rs"]
pub mod capture;

use capture::{CaptureManifest, CapturePackage, CapturePreview, CaptureView, PackageFile, fnv1a64};
use std::path::Path;
use umbral::{
    assets::obj::{Face, Model, parse_obj},
    math::{Mat4, Vec3},
    renderer::{
        framebuffer::{Framebuffer, MAX_PIXELS},
        pipeline::draw_model_with,
        shader::Material,
        stats::RenderStats,
        texture::Texture,
    },
    scene::{
        camera::Camera,
        world::{BASE_FOV, SHIP_MODEL_YAW, SPACE_COLOR, World},
    },
    visual::{
        perception_game::apply_lucid_perception,
        static_creature::AMBIENT,
        textured::{TexturedMode, TexturedShader},
    },
};

/// Cabecera exacta del paquete de crecimiento.
pub const GROWTH_PACKAGE_MAGIC: &str = "UMB311_GROWTH_PACKAGE 1";
/// Umbral geometrico de la ley (`|o| G(theta_max) = K_FOLD * radio del contorno`).
pub const K_FOLD: f64 = 1.0;
/// Rodilla de la saturacion C1 (fraccion del cono por debajo de la cual `o = r Delta` intacto).
pub const X_KNEE: f64 = 0.8;
/// Intervalos del muestreo del radio minimo del Hermite de cuerda 1 (2001 puntos; NO es un minimo exacto).
pub const HERMITE_RADIUS_SAMPLES: usize = 2000;
/// Iteraciones de biseccion de `theta_max` en `[0, 179 grados]`.
pub const THETA_MAX_ITERATIONS: usize = 60;
/// Cota superior de la biseccion (grados).
pub const THETA_MAX_LIMIT_DEG: f64 = 179.0;
/// Intervalos del muestreo de longitud de arco del eje (401 puntos).
pub const ARC_SAMPLES: usize = 400;
/// Bordes de `r(f)` de la ley en segundos base: alcance [2, 5.5), sostenido [5.5, 7.5), arrastre [7.5, 11).
pub const R_BREAKS_S: [f64; 4] = [2.0, 5.5, 7.5, 11.0];
/// Exponente PROVISIONAL de la duracion de alcance y arrastre: `base * max(1, d / d0)^0.5`.
pub const REACH_TIME_EXPONENT: f64 = 0.5;
/// Dominio NUMERICO (no de juego): cada coordenada de mundo de la nave en `[-1e9, 1e9]`.
pub const NUMERIC_WORLD_LIMIT: f64 = 1e9;
/// Desvio maximo por componente entre la orientacion de la nave del World y la de referencia.
pub const TARGET_ORIENTATION_TOLERANCE: f64 = 1e-9;
/// Tolerancia de la cadena de transforms de la nave (la misma de la captura).
pub const SHIP_CHAIN_TOLERANCE: f64 = capture::SHIP_CHAIN_TOLERANCE;
/// Calibre del plan: `C * (nave(1) + Delta_obj)` frente a la posicion real, relativo a `max(1, |p|)`.
pub const CALIBRATION_TOLERANCE: f64 = 1e-9;
/// Desplazamiento OBJ de `L.CASES['8x']` de la ley (G): objetivo a 8 veces la distancia criatura-referencia.
pub const D8_OBJ: [f64; 3] = [-4.868041, -1.103508, 6.602466];
/// Desplazamiento OBJ de `L.CASES['lateral-40']` (-40 en x de mundo con la escala de la criatura).
pub const LATERAL40_OBJ: [f64; 3] = [-0.560429, 0.0, 0.0];
/// Distancia al objetivo de la camara de presentacion (solo direccion; igual que la captura).
const PRESENTATION_TARGET_DISTANCE: f64 = 100.0;
/// Margen del encuadre de la vista general CANDIDATA.
const OVERVIEW_MARGIN: f64 = 1.15;

type V3 = [f64; 3];
type Q4 = [f64; 4];

// ---------------- aritmetica identica a curve_lib.py (mismo orden de operaciones) ----------------
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn nrm(a: V3) -> V3 {
    let l = length(a);
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0; 3]
    }
}
/// `sum()` de CPython >= 3.12 sobre floats con el inicio entero 0: el primer termino entra como `0 + x` y
/// el resto con la suma COMPENSADA de Neumaier; la compensacion se suma al final si es finita y no nula.
/// La ley Python la usa en el centro del lazo, la normal del lazo y la norma de `q_between`.
fn py_sum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let mut it = xs.into_iter();
    let Some(first) = it.next() else {
        return 0.0;
    };
    let (mut f, mut c) = (0.0 + first, 0.0);
    for x in it {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}
/// `max(lo, min(hi, x))` de Python (sin NaN).
fn clamp_py(x: f64, lo: f64, hi: f64) -> f64 {
    let m = if x < hi { x } else { hi };
    if m > lo { m } else { lo }
}
fn finite3(v: V3) -> bool {
    v.iter().all(|c| c.is_finite())
}

const Q_ID: Q4 = [1.0, 0.0, 0.0, 0.0];

fn q_between(a: V3, b: V3) -> Q4 {
    let (a, b) = (nrm(a), nrm(b));
    if length(a) < 0.5 || length(b) < 0.5 {
        return Q_ID;
    }
    let c = dot(a, b);
    if c <= -1.0 + 1e-12 {
        let h = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let x = nrm(cross(h, a));
        return [0.0, x[0], x[1], x[2]];
    }
    let x = cross(a, b);
    let q = [1.0 + c, x[0], x[1], x[2]];
    let n = py_sum(q.map(|x| x * x)).sqrt();
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

fn q_angle_axis(q: Q4) -> (f64, V3) {
    let (w, v) = if q[0] >= 0.0 {
        (q[0], [q[1], q[2], q[3]])
    } else {
        (-q[0], [-q[1], -q[2], -q[3]])
    };
    let s = length(v);
    if s < 1e-15 {
        (0.0, [1.0, 0.0, 0.0])
    } else {
        (2.0 * s.atan2(w), [v[0] / s, v[1] / s, v[2] / s])
    }
}

fn q_pow(q: Q4, t: f64) -> Q4 {
    let (a, ax) = q_angle_axis(q);
    if a == 0.0 || t == 0.0 {
        return Q_ID;
    }
    let h = 0.5 * a * t;
    let s = h.sin();
    [h.cos(), ax[0] * s, ax[1] * s, ax[2] * s]
}

fn q_rot(q: Q4, v: V3) -> V3 {
    let u = [q[1], q[2], q[3]];
    let t = mul(cross(u, v), 2.0);
    add(add(v, mul(t, q[0])), cross(u, t))
}

fn hermite(p0: V3, p1: V3, m: V3, t: f64) -> (V3, V3) {
    let (t2, t3) = (t * t, t * t * t);
    let pos = add(
        add(
            mul(p0, 2.0 * t3 - 3.0 * t2 + 1.0),
            mul(m, t3 - 2.0 * t2 + t),
        ),
        add(mul(p1, -2.0 * t3 + 3.0 * t2), mul(m, t3 - t2)),
    );
    let der = add(
        add(
            mul(p0, 6.0 * t2 - 6.0 * t),
            mul(m, 3.0 * t2 - 4.0 * t + 1.0),
        ),
        add(mul(p1, -6.0 * t2 + 6.0 * t), mul(m, 3.0 * t2 - 2.0 * t)),
    );
    (pos, der)
}

// ---------------- ley 'fixed' (funciones puras) ----------------

/// `smooth` de curve_lib: smoothstep con clamp a [0, 1].
pub fn smooth(x: f64) -> f64 {
    let x = clamp_py(x, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `r(f)` de la ley: 0 antes de 2 s; smoothstep hasta 5.5 s; 1 hasta 7.5 s; 1 - smoothstep hasta 11 s; 0.
pub fn reach_weight(frame: usize, fps: usize) -> f64 {
    let t = (frame as f64 - 1.0) / fps as f64;
    let [a, b, c, d] = R_BREAKS_S;
    if t < a {
        0.0
    } else if t < b {
        smooth((t - a) / (b - a))
    } else if t < c {
        1.0
    } else if t < d {
        1.0 - smooth((t - c) / (d - c))
    } else {
        0.0
    }
}

/// Radio de curvatura minimo del Hermite de cuerda 1 con tangentes unitarias en ambos extremos y angulo
/// `theta` cuerda-tangente, MUESTREADO en 2001 puntos (`|p' x p''| = sin(theta) |12t - 6|`). No es un
/// minimo exacto ni una cota conservadora certificada (el revisor midio ~11.9 % de sobreestimacion a 179).
pub fn hermite_min_radius_unit(theta: f64) -> f64 {
    let (st, ct) = (theta.sin(), theta.cos());
    if st <= 0.0 {
        return f64::INFINITY;
    }
    let samples = HERMITE_RADIUS_SAMPLES as f64;
    let mut best = f64::INFINITY;
    for i in 0..=HERMITE_RADIUS_SAMPLES {
        let t = i as f64 / samples;
        let h1 = 6.0 * t - 6.0 * t * t;
        let j1 = 6.0 * t * t - 6.0 * t + 1.0;
        let den = st * (12.0 * t - 6.0).abs();
        if den > 0.0 {
            let q = h1 * h1 + j1 * j1 + 2.0 * h1 * j1 * ct;
            let sp = (if q > 0.0 { q } else { 0.0 }).sqrt();
            let v = sp.powf(3.0) / den;
            if v < best {
                best = v;
            }
        }
    }
    best
}

/// Mayor `theta` en `[0, 179 grados]` con `G(theta) >= need` (biseccion de 60 pasos sobre `G` muestreada).
pub fn theta_max(need: f64) -> f64 {
    let mut hi = THETA_MAX_LIMIT_DEG * (std::f64::consts::PI / 180.0);
    if hermite_min_radius_unit(hi) >= need {
        return hi;
    }
    let mut lo = 0.0;
    for _ in 0..THETA_MAX_ITERATIONS {
        let mid = 0.5 * (lo + hi);
        if hermite_min_radius_unit(mid) >= need {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Saturacion C1 de la fraccion del cono: `x` si `x <= 0.8`; si no `0.8 + 0.2 tanh((x - 0.8) / 0.2)`.
/// En reales es < 1; en f64 llega a 1.0 EXACTO desde x ~ 4.4 (tanh redondea a 1): el angulo usado es entonces
/// `theta_max`, que sigue siendo factible porque la biseccion devuelve un angulo que cumple `G >= need`.
pub fn cone_saturation(x: f64) -> f64 {
    if x <= X_KNEE {
        x
    } else {
        X_KNEE + (1.0 - X_KNEE) * ((x - X_KNEE) / (1.0 - X_KNEE)).tanh()
    }
}

/// Casos con nombre de la ley (Delta en espacio OBJ de la criatura, los de `L.CASES` de G; 4x por la
/// misma regla `(K - 1) / 7 * D8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowthCase {
    Reference,
    TwoX,
    FourX,
    EightX,
    Lateral40,
}

impl GrowthCase {
    pub const ALL: [GrowthCase; 5] = [
        GrowthCase::Reference,
        GrowthCase::TwoX,
        GrowthCase::FourX,
        GrowthCase::EightX,
        GrowthCase::Lateral40,
    ];
    pub fn name(self) -> &'static str {
        match self {
            GrowthCase::Reference => "referencia",
            GrowthCase::TwoX => "2x",
            GrowthCase::FourX => "4x",
            GrowthCase::EightX => "8x",
            GrowthCase::Lateral40 => "lateral-40",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
    /// Delta OBJ exacto del caso (como en `L.CASES` de G: `2x = D8 / 7.0`, `8x = D8`).
    pub fn delta_obj(self) -> [f64; 3] {
        match self {
            GrowthCase::Reference => [0.0; 3],
            GrowthCase::TwoX => D8_OBJ.map(|x| x / 7.0),
            GrowthCase::FourX => D8_OBJ.map(|x| x * 3.0 / 7.0),
            GrowthCase::EightX => D8_OBJ,
            GrowthCase::Lateral40 => LATERAL40_OBJ,
        }
    }
}

// ---------------- paquete de crecimiento ----------------

/// Lado de un vertice conservado: proximal (pose aprobada) o distal (pose + `o`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    P,
    D,
}

/// Procedencia de un vertice conservado en la pose aprobada del cuadro.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VertexSource {
    /// Vertice original (`orig`, u `orig_on_cut` si esta sobre el corte).
    Orig { orig: usize, on_cut: bool },
    /// Punto de corte: `V[a] + (V[b] - V[a]) t`.
    Cut { a: usize, b: usize, t: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GrowthVertex {
    Retained {
        side: Side,
        source: VertexSource,
    },
    /// Vertice del anillo `j` (1..N-1), posicion `k` del lazo.
    Ring {
        j: usize,
        k: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    P,
    D,
    G,
}

/// Procedencia de cada esquina de cara (normal y binding con la malla aprobada).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CornerSource {
    /// Esquina `corner` de la cara original `face`.
    Corner { face: usize, corner: usize },
    /// Interpolada entre dos esquinas de la cara original `face` (normal = nrm(na + (nb - na) t)).
    Edge {
        face: usize,
        corner_a: usize,
        corner_b: usize,
        t: f64,
    },
    /// Anillo `ring` (0..=N), posicion `k` del lazo.
    Ring { ring: usize, k: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthFace {
    pub v: [usize; 3],
    pub part: Part,
    pub uv: [[f64; 2]; 3],
    pub src: [CornerSource; 3],
}

/// Topologia aprobada parseada (inmutable).
#[derive(Clone, Debug, PartialEq)]
pub struct GrowthTopology {
    capture_fnv: u64,
    hashes: [String; 3],
    frames: usize,
    vertices: Vec<GrowthVertex>,
    faces: Vec<GrowthFace>,
    loop_p: Vec<usize>,
    loop_d: Vec<usize>,
    rings: Vec<Vec<usize>>,
    root: Vec<usize>,
    grip: Vec<usize>,
    coil: Vec<usize>,
    guides: Vec<V3>,
}

struct Cursor<'a> {
    lines: std::str::Lines<'a>,
    line: usize,
}

impl<'a> Cursor<'a> {
    fn next(&mut self, key: &str) -> Result<Vec<&'a str>, String> {
        self.line += 1;
        let line = self
            .lines
            .next()
            .ok_or(format!("linea {}: falta {key:?}", self.line))?;
        let toks: Vec<&str> = line.split(' ').collect();
        if toks[0] != key || toks.iter().any(|t| t.is_empty()) {
            return Err(format!(
                "linea {}: se esperaba {key:?} y llego {line:?}",
                self.line
            ));
        }
        Ok(toks[1..].to_vec())
    }
}

fn int(t: &str, what: &str) -> Result<usize, String> {
    if t.bytes().all(|b| b.is_ascii_digit()) {
        t.parse::<usize>()
            .map_err(|_| format!("{what}: entero invalido {t:?}"))
    } else {
        Err(format!("{what}: entero invalido {t:?}"))
    }
}

fn real(t: &str, what: &str) -> Result<f64, String> {
    t.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or(format!("{what}: numero invalido o no finito {t:?}"))
}

fn exact(toks: &[&str], n: usize, what: &str) -> Result<(), String> {
    if toks.len() == n {
        Ok(())
    } else {
        Err(format!(
            "{what}: se esperaban {n} campos y hay {}",
            toks.len()
        ))
    }
}

fn hex_line(c: &mut Cursor, key: &str, digits: usize) -> Result<String, String> {
    let t = c.next(key)?;
    exact(&t, 1, key)?;
    if t[0].len() != digits || !t[0].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{key}: se esperaban {digits} cifras hexadecimales"));
    }
    Ok(t[0].to_string())
}

impl GrowthTopology {
    /// Parser ESTRICTO: orden fijo de lineas, conteos exactos, sin `\r`, sin tokens extra, nada tras
    /// `end`, floats finitos. Solo sintaxis y rangos internos; el binding con la captura lo valida
    /// `GrowthPackage::from_parts`.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.contains('\r') || !text.ends_with("end\n") {
            return Err("paquete de crecimiento: saltos de linea LF y final exacto 'end'".into());
        }
        let mut c = Cursor {
            lines: text.lines(),
            line: 0,
        };
        let head = c.next("UMB311_GROWTH_PACKAGE")?;
        if head != ["1"] {
            return Err(format!(
                "version de paquete de crecimiento no soportada: {head:?}"
            ));
        }
        let fnv = hex_line(&mut c, "capture_package_fnv1a64", 16)?;
        let capture_fnv = u64::from_str_radix(&fnv, 16).map_err(|e| e.to_string())?;
        let hashes = [
            hex_line(&mut c, "topology_sha256", 64)?,
            hex_line(&mut c, "law_sha256", 64)?,
            hex_line(&mut c, "approval_sha256", 64)?,
        ];
        let t = c.next("frames")?;
        exact(&t, 1, "frames")?;
        let frames = int(t[0], "frames")?;
        let t = c.next("counts")?;
        exact(&t, 4, "counts")?;
        let (nv, nf, k, n) = (
            int(t[0], "counts")?,
            int(t[1], "counts")?,
            int(t[2], "counts")?,
            int(t[3], "counts")?,
        );
        if !(2..=100_000).contains(&frames)
            || !(3..=4096).contains(&k)
            || !(2..=4096).contains(&n)
            || nv == 0
            || nf == 0
            || nv > 10_000_000
            || nf > 10_000_000
        {
            return Err(format!(
                "conteos fuera de rango: frames {frames}, v {nv}, f {nf}, K {k}, N {n}"
            ));
        }
        let mut vertices = Vec::with_capacity(nv);
        for i in 0..nv {
            let t = c.next("vertex")?;
            let what = format!("vertex {i}");
            let side = |s: &str| match s {
                "P" => Ok(Side::P),
                "D" => Ok(Side::D),
                _ => Err(format!("{what}: lado invalido {s:?}")),
            };
            let v = match t.first().copied() {
                Some(kind @ ("orig" | "orig_on_cut")) => {
                    exact(&t, 3, &what)?;
                    GrowthVertex::Retained {
                        side: side(t[1])?,
                        source: VertexSource::Orig {
                            orig: int(t[2], &what)?,
                            on_cut: kind == "orig_on_cut",
                        },
                    }
                }
                Some("cut") => {
                    exact(&t, 5, &what)?;
                    let tt = real(t[4], &what)?;
                    if !(tt > 0.0 && tt < 1.0) {
                        return Err(format!("{what}: t fuera de (0, 1)"));
                    }
                    GrowthVertex::Retained {
                        side: side(t[1])?,
                        source: VertexSource::Cut {
                            a: int(t[2], &what)?,
                            b: int(t[3], &what)?,
                            t: tt,
                        },
                    }
                }
                Some("ring") => {
                    exact(&t, 3, &what)?;
                    GrowthVertex::Ring {
                        j: int(t[1], &what)?,
                        k: int(t[2], &what)?,
                    }
                }
                _ => return Err(format!("{what}: tipo desconocido {t:?}")),
            };
            vertices.push(v);
        }
        let ids = |t: Vec<&str>, what: &str| -> Result<Vec<usize>, String> {
            let v = t
                .iter()
                .map(|x| int(x, what))
                .collect::<Result<Vec<_>, _>>()?;
            if v.iter().any(|&i| i >= nv) {
                return Err(format!("{what}: indice de vertice fuera de rango"));
            }
            Ok(v)
        };
        let loop_p = ids(c.next("loop_p")?, "loop_p")?;
        let loop_d = ids(c.next("loop_d")?, "loop_d")?;
        if loop_p.len() != k || loop_d.len() != k {
            return Err("loop_p/loop_d: K indices".into());
        }
        let mut rings = Vec::with_capacity(n + 1);
        for j in 0..=n {
            let t = c.next("ring")?;
            if t.first().map(|x| int(x, "ring")) != Some(Ok(j)) {
                return Err(format!("ring {j}: indice de anillo fuera de orden"));
            }
            let r = ids(t[1..].to_vec(), "ring")?;
            if r.len() != k {
                return Err(format!("ring {j}: K indices"));
            }
            rings.push(r);
        }
        let mut faces = Vec::with_capacity(nf);
        for fi in 0..nf {
            let t = c.next("face")?;
            let what = format!("face {fi}");
            exact(&t, 13, &what)?;
            let part = match t[0] {
                "P" => Part::P,
                "D" => Part::D,
                "G" => Part::G,
                p => return Err(format!("{what}: parte invalida {p:?}")),
            };
            let v = [int(t[1], &what)?, int(t[2], &what)?, int(t[3], &what)?];
            if v.iter().any(|&i| i >= nv) || v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
                return Err(format!("{what}: indices de vertice invalidos"));
            }
            let mut uv = [[0.0; 2]; 3];
            for (q, p) in uv.iter_mut().enumerate() {
                *p = [real(t[4 + 2 * q], &what)?, real(t[5 + 2 * q], &what)?];
            }
            let mut src = [CornerSource::Ring { ring: 0, k: 0 }; 3];
            for (q, s) in src.iter_mut().enumerate() {
                let f: Vec<&str> = t[10 + q].split(':').collect();
                *s = match (f[0], f.len()) {
                    ("c", 3) => CornerSource::Corner {
                        face: int(f[1], &what)?,
                        corner: int(f[2], &what)?,
                    },
                    ("e", 5) => CornerSource::Edge {
                        face: int(f[1], &what)?,
                        corner_a: int(f[2], &what)?,
                        corner_b: int(f[3], &what)?,
                        t: real(f[4], &what)?,
                    },
                    ("g", 3) => CornerSource::Ring {
                        ring: int(f[1], &what)?,
                        k: int(f[2], &what)?,
                    },
                    _ => return Err(format!("{what}: procedencia invalida {:?}", t[10 + q])),
                };
            }
            faces.push(GrowthFace { v, part, uv, src });
        }
        let mut mask = |name: &str| -> Result<Vec<usize>, String> {
            let t = c.next("mask")?;
            if t.len() < 2 || t[0] != name || int(t[1], name)? != t.len() - 2 {
                return Err(format!("mask {name}: nombre o conteo invalidos"));
            }
            let mut v = ids(t[2..].to_vec(), name)?;
            v.sort_unstable();
            if v.is_empty() || v.windows(2).any(|w| w[0] == w[1]) {
                return Err(format!("mask {name}: vacia o con repetidos"));
            }
            Ok(v)
        };
        let (root, grip, coil) = (mask("root")?, mask("grip")?, mask("coil")?);
        let mut guides = Vec::with_capacity(frames);
        for f in 1..=frames {
            let t = c.next("guide")?;
            exact(&t, 4, "guide")?;
            if int(t[0], "guide")? != f {
                return Err(format!("guide {f}: cuadro fuera de orden"));
            }
            let g = [
                real(t[1], "guide")?,
                real(t[2], "guide")?,
                real(t[3], "guide")?,
            ];
            if !(0.9..=1.1).contains(&length(g)) {
                return Err(format!("guide {f}: no unitaria"));
            }
            guides.push(g);
        }
        if c.next("end")? != Vec::<&str>::new() || c.lines.next().is_some() {
            return Err("contenido tras 'end'".into());
        }
        Ok(Self {
            capture_fnv,
            hashes,
            frames,
            vertices,
            faces,
            loop_p,
            loop_d,
            rings,
            root,
            grip,
            coil,
            guides,
        })
    }
    pub fn capture_fnv(&self) -> u64 {
        self.capture_fnv
    }
    /// sha256 (texto) de topologia, ley y aprobacion de G con los que se exporto el paquete.
    pub fn provenance(&self) -> &[String; 3] {
        &self.hashes
    }
    pub fn vertices(&self) -> &[GrowthVertex] {
        &self.vertices
    }
    pub fn faces(&self) -> &[GrowthFace] {
        &self.faces
    }
    pub fn loop_p(&self) -> &[usize] {
        &self.loop_p
    }
    pub fn loop_d(&self) -> &[usize] {
        &self.loop_d
    }
    /// Anillos 0..=N (0 = lazo P, N = lazo D).
    pub fn rings(&self) -> &[Vec<usize>] {
        &self.rings
    }
    pub fn root(&self) -> &[usize] {
        &self.root
    }
    pub fn grip(&self) -> &[usize] {
        &self.grip
    }
    pub fn coil(&self) -> &[usize] {
        &self.coil
    }
}

/// Pose aprobada del cuadro: posiciones y normales por esquina CRUDAS (las del OBJ, como en la ley Python).
struct ArmPose {
    positions: Vec<V3>,
    normals: Vec<V3>,
    /// Indices de normal por cara del MISMO cuadro (como `Fo` por cuadro en Python).
    normal_indices: Vec<[usize; 3]>,
}

/// Lazo proximal del cuadro (independiente de `o`): tangente `T` orientada con la guia, centro y radio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopFrame {
    pub tangent: [f64; 3],
    pub center: [f64; 3],
    pub contour_radius: f64,
}

/// `o(f)` de la ley y su diagnostico.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LawSample {
    pub frame: usize,
    pub r: f64,
    pub offset: [f64; 3],
    /// `None` cuando `|r Delta| = 0` (la ley no calcula el cono).
    pub theta_nominal: Option<f64>,
    pub theta_max: Option<f64>,
    pub cone_fraction: Option<f64>,
    pub clamped: bool,
    pub theta_used: Option<f64>,
}

/// Geometria evaluada de un cuadro (ademas de la malla escrita en el `Model`).
#[derive(Clone, Debug, PartialEq)]
pub struct GrowthPose {
    pub law: LawSample,
    pub tangent: [f64; 3],
    pub center: [f64; 3],
    /// Centros de los anillos 0..=N.
    pub centers: Vec<[f64; 3]>,
    /// Rotaciones `R_j` (w, x, y, z) de los anillos 0..=N.
    pub rotations: Vec<[f64; 4]>,
    pub growth_length: f64,
    pub contour_radius: f64,
    /// Radio de curvatura minimo discreto del eje (401 puntos); infinito si el tramo esta dormido.
    pub axis_min_curvature_radius: f64,
}

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

/// Datos de la ley cacheados una vez: topologia, poses propias, lazos por cuadro, listas de esquinas del
/// lazo y plantillas (malla con tramo y malla original).
pub struct GrowthData {
    topology: GrowthTopology,
    poses: Vec<ArmPose>,
    arm_faces: Vec<[usize; 3]>,
    loops: Vec<LoopFrame>,
    /// Por posicion k del lazo P, las esquinas no-G que lo tocan (orden de caras y esquinas de Python).
    loop_corners: Vec<Vec<CornerSource>>,
    fps: usize,
    grip_frame: usize,
    template: Model,
    original_template: Model,
}

impl GrowthData {
    pub fn topology(&self) -> &GrowthTopology {
        &self.topology
    }
    pub fn frames(&self) -> usize {
        self.poses.len()
    }
    pub fn fps(&self) -> usize {
        self.fps
    }
    pub fn grip_frame(&self) -> usize {
        self.grip_frame
    }
    /// Malla con tramo (caras, UV por esquina y material fijos; posiciones/normales por cuadro).
    pub fn template(&self) -> &Model {
        &self.template
    }
    /// Posiciones aprobadas (OBJ) del cuadro 1-based.
    pub fn base_positions(&self, frame: usize) -> Result<&[[f64; 3]], String> {
        self.pose(frame).map(|p| p.positions.as_slice())
    }
    pub fn loop_frame(&self, frame: usize) -> Result<LoopFrame, String> {
        self.pose(frame)?;
        Ok(self.loops[frame - 1])
    }
    fn pose(&self, frame: usize) -> Result<&ArmPose, String> {
        frame
            .checked_sub(1)
            .and_then(|i| self.poses.get(i))
            .ok_or(format!("cuadro {frame} fuera de 1..={}", self.poses.len()))
    }

    fn base_point(p: &ArmPose, s: &VertexSource) -> V3 {
        match *s {
            VertexSource::Orig { orig, .. } => p.positions[orig],
            VertexSource::Cut { a, b, t } => {
                add(p.positions[a], mul(sub(p.positions[b], p.positions[a]), t))
            }
        }
    }

    fn compute_loop(&self, frame: usize, p: &ArmPose) -> LoopFrame {
        let topo = &self.topology;
        let pts: Vec<V3> = topo
            .loop_p
            .iter()
            .map(|&i| match &topo.vertices[i] {
                GrowthVertex::Retained { source, .. } => Self::base_point(p, source),
                GrowthVertex::Ring { .. } => [0.0; 3],
            })
            .collect();
        let k = pts.len();
        let s: V3 = std::array::from_fn(|a| py_sum(pts.iter().map(|q| q[a])));
        let cp = mul(s, 1.0 / k as f64);
        let mut area = [0.0; 3];
        for i in 0..k {
            area = add(area, cross(sub(pts[i], cp), sub(pts[(i + 1) % k], cp)));
        }
        let mut t = nrm(area);
        if dot(t, topo.guides[frame - 1]) < 0.0 {
            t = mul(t, -1.0);
        }
        let mut rc = f64::NEG_INFINITY;
        for q in &pts {
            let l = length(sub(*q, cp));
            if l > rc {
                rc = l;
            }
        }
        LoopFrame {
            tangent: t,
            center: cp,
            contour_radius: rc,
        }
    }

    /// `o(f)` de la ley `fixed` para un `Delta` OBJ. `Err` explicito fuera de dominio: recorte necesario en
    /// el agarre (o(agarre) = Delta imposible), Delta antiparalelo a `T`, cono nulo o valores no finitos.
    pub fn offset(&self, frame: usize, delta: [f64; 3]) -> Result<LawSample, String> {
        self.pose(frame)?;
        if !finite3(delta) {
            return Err("Delta no finito".into());
        }
        let r = reach_weight(frame, self.fps);
        let o_nom = mul(delta, r);
        let n = length(o_nom);
        let mut law = LawSample {
            frame,
            r,
            offset: o_nom,
            theta_nominal: None,
            theta_max: None,
            cone_fraction: None,
            clamped: false,
            theta_used: None,
        };
        if !n.is_finite() {
            return Err(format!("f{frame}: |r Delta| no finito"));
        }
        if n == 0.0 {
            return Ok(law);
        }
        let lf = self.loops[frame - 1];
        let t = lf.tangent;
        let c = clamp_py(dot(t, o_nom) / n, -1.0, 1.0);
        let th = c.acos();
        let tmax = theta_max(K_FOLD * lf.contour_radius / n);
        if tmax == 0.0 {
            return Err(format!(
                "f{frame}: cono factible nulo (|o| = {n:e} frente al radio del contorno {}): fuera de dominio",
                lf.contour_radius
            ));
        }
        let x = th / tmax;
        law.theta_nominal = Some(th);
        law.theta_max = Some(tmax);
        law.cone_fraction = Some(x);
        if x <= X_KNEE {
            return Ok(law);
        }
        if frame == self.grip_frame {
            return Err(format!(
                "Delta fuera de dominio en el agarre f{frame} (fraccion del cono {x:.4} > {X_KNEE}): o(agarre) = Delta imposible sin pliegue"
            ));
        }
        let u = sub(mul(o_nom, 1.0 / n), mul(t, c));
        if length(u) < 1e-12 {
            return Err(format!(
                "f{frame}: Delta antiparalelo a la tangente del lazo; direccion de despliegue indefinida"
            ));
        }
        let u = nrm(u);
        let tu = tmax * (X_KNEE + (1.0 - X_KNEE) * ((x - X_KNEE) / (1.0 - X_KNEE)).tanh());
        law.offset = mul(add(mul(t, tu.cos()), mul(u, tu.sin())), n);
        law.clamped = true;
        law.theta_used = Some(tu);
        if !finite3(law.offset) {
            return Err(format!("f{frame}: o no finito"));
        }
        Ok(law)
    }

    /// Evalua el cuadro con `Delta` OBJ y escribe posiciones y normales por esquina en `out` (debe tener
    /// la topologia de `template`). Valida finitud de TODO lo escrito; con `Err`, `out` puede quedar a
    /// medias: el llamador escribe en una malla temporal.
    pub fn evaluate(
        &self,
        frame: usize,
        delta: [f64; 3],
        out: &mut Model,
    ) -> Result<GrowthPose, String> {
        let law = self.offset(frame, delta)?;
        self.evaluate_with(law, out)
    }

    /// Igual que `evaluate` con un `o(f)` ya calculado por la ley (cache del plan).
    pub fn evaluate_with(&self, law: LawSample, out: &mut Model) -> Result<GrowthPose, String> {
        let frame = law.frame;
        let p = self.pose(frame)?;
        let topo = &self.topology;
        if out.vertices.len() != topo.vertices.len()
            || out.normals.len() != 3 * topo.faces.len()
            || out.faces.len() != topo.faces.len()
        {
            return Err("malla de salida sin la topologia del tramo".into());
        }
        let o = law.offset;
        for (i, v) in topo.vertices.iter().enumerate() {
            if let GrowthVertex::Retained { side, source } = v {
                let q = Self::base_point(p, source);
                out.vertices[i] = if *side == Side::D { add(q, o) } else { q };
            }
        }
        let lf = self.loops[frame - 1];
        let (t, cp, rc) = (lf.tangent, lf.center, lf.contour_radius);
        let lp: Vec<V3> = topo.loop_p.iter().map(|&i| out.vertices[i]).collect();
        let n_int = topo.rings.len() - 1;
        let olen = length(o);
        let m = mul(t, olen);
        let cd = add(cp, o);
        let mut rotations = vec![Q_ID; n_int + 1];
        let mut centers = vec![cp; n_int + 1];
        let (mut growth_length, mut axis_radius) = (0.0, f64::INFINITY);
        if olen == 0.0 {
            for ring in &topo.rings[1..n_int] {
                for (kk, &vi) in ring.iter().enumerate() {
                    out.vertices[vi] = lp[kk];
                }
            }
        } else {
            let s_n = ARC_SAMPLES;
            let ts: Vec<f64> = (0..=s_n).map(|i| i as f64 / s_n as f64).collect();
            let pts: Vec<V3> = ts.iter().map(|&tt| hermite(cp, cd, m, tt).0).collect();
            let mut cum = Vec::with_capacity(s_n + 1);
            cum.push(0.0);
            for i in 1..=s_n {
                let l = cum[i - 1] + length(sub(pts[i], pts[i - 1]));
                cum.push(l);
            }
            let total = cum[s_n];
            growth_length = total;
            let mut q = 0;
            for j in 1..n_int {
                let s = total * j as f64 / n_int as f64;
                while cum[q + 1] < s {
                    q += 1;
                }
                let tt = ts[q] + (ts[q + 1] - ts[q]) * (s - cum[q]) / (cum[q + 1] - cum[q]);
                let (c, der) = hermite(cp, cd, m, tt);
                let r = q_pow(q_between(t, der), 1.0);
                centers[j] = c;
                rotations[j] = r;
                for (kk, &vi) in topo.rings[j].iter().enumerate() {
                    out.vertices[vi] = add(c, q_rot(r, sub(lp[kk], cp)));
                }
            }
            centers[n_int] = cd;
            let mut kmax: f64 = 0.0;
            for i in 1..s_n {
                let (a, b, c) = (pts[i - 1], pts[i], pts[i + 1]);
                let (ab, bc) = (sub(b, a), sub(c, b));
                let ang = clamp_py(dot(nrm(ab), nrm(bc)), -1.0, 1.0).acos();
                let seg = 0.5 * (length(ab) + length(bc));
                if seg > 0.0 && ang / seg > kmax {
                    kmax = ang / seg;
                }
            }
            axis_radius = if kmax > 0.0 {
                1.0 / kmax
            } else {
                f64::INFINITY
            };
        }
        let normal = |s: &CornerSource| -> V3 {
            match *s {
                CornerSource::Corner { face, corner } => p.normals[p.normal_indices[face][corner]],
                CornerSource::Edge {
                    face,
                    corner_a,
                    corner_b,
                    t,
                } => {
                    let na = p.normals[p.normal_indices[face][corner_a]];
                    let nb = p.normals[p.normal_indices[face][corner_b]];
                    nrm(add(na, mul(sub(nb, na), t)))
                }
                CornerSource::Ring { .. } => [0.0; 3],
            }
        };
        let loop_n: Vec<V3> = self
            .loop_corners
            .iter()
            .map(|list| {
                let ns: Vec<V3> = list.iter().map(&normal).collect();
                nrm(std::array::from_fn(|a| py_sum(ns.iter().map(|n| n[a]))))
            })
            .collect();
        for (fi, face) in topo.faces.iter().enumerate() {
            for (c, s) in face.src.iter().enumerate() {
                out.normals[3 * fi + c] = match *s {
                    CornerSource::Ring { ring, k } => q_rot(rotations[ring], loop_n[k]),
                    _ => normal(s),
                };
            }
        }
        if !out.vertices.iter().all(|v| finite3(*v)) || !out.normals.iter().all(|v| finite3(*v)) {
            return Err(format!("f{frame}: geometria no finita"));
        }
        Ok(GrowthPose {
            law,
            tangent: t,
            center: cp,
            centers,
            rotations,
            growth_length,
            contour_radius: rc,
            axis_min_curvature_radius: axis_radius,
        })
    }

    /// Pose ORIGINAL aprobada del cuadro (malla del brazo de la captura, normales renormalizadas con la
    /// misma aritmetica que la captura). Ruta de referencia en la vista general.
    pub fn original_pose(&self, frame: usize, out: &mut Model) -> Result<(), String> {
        let p = self.pose(frame)?;
        if out.vertices.len() != p.positions.len() || out.faces.len() != self.arm_faces.len() {
            return Err("malla de salida sin la topologia del brazo original".into());
        }
        out.vertices.clone_from(&p.positions);
        out.normals = p
            .normals
            .iter()
            .map(|n| {
                let v = Vec3::from_array(*n);
                let u = v * (1.0 / v.length());
                [u.x, u.y, u.z]
            })
            .collect();
        for (face, n) in out.faces.iter_mut().zip(&p.normal_indices) {
            face.normal_indices = Some(*n);
        }
        Ok(())
    }
}

/// Paquete completo: captura ORIGINAL validada (ruta de referencia) + datos de la ley.
pub struct GrowthPackage {
    capture: CapturePackage,
    data: GrowthData,
}

fn arm_edge(faces: &[[usize; 3]], a: usize, b: usize) -> bool {
    faces.iter().any(|f| {
        (0..3).any(|c| {
            let (x, y) = (f[c], f[(c + 1) % 3]);
            (x == a && y == b) || (x == b && y == a)
        })
    })
}

impl GrowthPackage {
    /// Valida TODO antes de aceptar: la captura con `CapturePackage::from_parts` (mismos modelos) y la
    /// topologia contra la malla aprobada: vinculo FNV del manifest, indices, aristas de corte reales,
    /// procedencia de cada esquina (vertice y cara original coherentes), anillos y lazos, mascaras raiz y
    /// agarre = las del manifest (por correspondencia), espiral distal, material unico y guia por cuadro.
    pub fn from_parts(
        capture_text: &str,
        arm_frames: Vec<Model>,
        body: Model,
        growth_text: &str,
    ) -> Result<Self, String> {
        let topo = GrowthTopology::parse(growth_text)?;
        if topo.capture_fnv != fnv1a64(capture_text.as_bytes()) {
            return Err(
                "paquete de crecimiento vinculado a OTRO capture_package.txt (FNV distinto)".into(),
            );
        }
        let first = arm_frames.first().ok_or("sin poses")?;
        let arm_faces: Vec<[usize; 3]> = first.faces.iter().map(|f| f.vertex_indices).collect();
        let material = first.faces.first().and_then(|f| f.material.clone());
        if first.faces.iter().any(|f| f.material != material) {
            return Err(
                "el brazo aprobado mezcla materiales: el tramo no sabe cual heredar".into(),
            );
        }
        let mut original_template = copy_model(first);
        let poses: Vec<ArmPose> = arm_frames
            .iter()
            .map(|m| ArmPose {
                positions: m.vertices.clone(),
                normals: m.normals.clone(),
                normal_indices: m
                    .faces
                    .iter()
                    .map(|f| f.normal_indices.unwrap_or([usize::MAX; 3]))
                    .collect(),
            })
            .collect();
        let capture = CapturePackage::from_parts(capture_text, arm_frames, body)?;
        let manifest: &CaptureManifest = capture.manifest();
        if topo.frames != manifest.frames() {
            return Err("paquete de crecimiento con otro numero de cuadros".into());
        }
        let nv_arm = manifest_arm_vertices(&poses)?;
        let (k, n) = (topo.loop_p.len(), topo.rings.len() - 1);
        // vertices
        let mut seen_orig = vec![false; nv_arm];
        let mut ring_seen = vec![vec![false; k]; n + 1];
        for (i, v) in topo.vertices.iter().enumerate() {
            match *v {
                GrowthVertex::Retained { source, .. } => match source {
                    VertexSource::Orig { orig, .. } => {
                        if orig >= nv_arm {
                            return Err(format!("vertex {i}: orig fuera del brazo"));
                        }
                        seen_orig[orig] = true;
                    }
                    VertexSource::Cut { a, b, .. } => {
                        if a >= nv_arm || b >= nv_arm || a == b || !arm_edge(&arm_faces, a, b) {
                            return Err(format!("vertex {i}: corte sobre una arista inexistente"));
                        }
                    }
                },
                GrowthVertex::Ring { j, k: kk } => {
                    if !(1..n).contains(&j) || kk >= k || topo.rings[j][kk] != i || ring_seen[j][kk]
                    {
                        return Err(format!("vertex {i}: anillo ({j}, {kk}) incoherente"));
                    }
                    ring_seen[j][kk] = true;
                }
            }
        }
        if seen_orig.iter().any(|s| !s) {
            return Err("falta algun vertice del brazo aprobado en la topologia".into());
        }
        if topo.rings[0] != topo.loop_p || topo.rings[n] != topo.loop_d {
            return Err("anillos 0 y N deben ser los lazos P y D".into());
        }
        let retained = |i: usize| match topo.vertices[i] {
            GrowthVertex::Retained { side, source } => Some((side, source)),
            GrowthVertex::Ring { .. } => None,
        };
        for kk in 0..k {
            let (Some((Side::P, sp)), Some((Side::D, sd))) =
                (retained(topo.loop_p[kk]), retained(topo.loop_d[kk]))
            else {
                return Err(format!("lazo {kk}: P debe ser proximal y D distal"));
            };
            if sp != sd {
                return Err(format!("lazo {kk}: P y D con distinta procedencia"));
            }
        }
        // caras y procedencia de esquinas
        let nf_arm = arm_faces.len();
        for (fi, f) in topo.faces.iter().enumerate() {
            for (c, s) in f.src.iter().enumerate() {
                let vi = f.v[c];
                let ok = match (*s, f.part) {
                    (CornerSource::Ring { ring, k: kk }, Part::G) => {
                        ring <= n && kk < k && topo.rings[ring][kk] == vi
                    }
                    (CornerSource::Corner { face, corner }, Part::P | Part::D) => {
                        face < nf_arm
                            && corner < 3
                            && matches!(retained(vi), Some((side, VertexSource::Orig { orig, .. }))
                                if orig == arm_faces[face][corner] && side_of(f.part) == side)
                    }
                    (
                        CornerSource::Edge {
                            face,
                            corner_a,
                            corner_b,
                            t,
                        },
                        Part::P | Part::D,
                    ) => {
                        face < nf_arm
                            && corner_a < 3
                            && corner_b < 3
                            && corner_a != corner_b
                            && t > 0.0
                            && t < 1.0
                            && matches!(retained(vi), Some((side, VertexSource::Cut { a, b, t: tv }))
                                if side == side_of(f.part)
                                && ((a == arm_faces[face][corner_a] && b == arm_faces[face][corner_b] && tv == t)
                                    || (b == arm_faces[face][corner_a] && a == arm_faces[face][corner_b]
                                        && (tv + t - 1.0).abs() <= 1e-12)))
                    }
                    _ => false,
                };
                if !ok {
                    return Err(format!(
                        "face {fi} esquina {c}: procedencia incoherente con la malla aprobada"
                    ));
                }
                // La captura ya valido indices y finitud; aun asi no indexar procedencia externa.
                let source_uv = |face: usize, corner: usize| -> Result<[f64; 2], String> {
                    original_template
                        .faces
                        .get(face)
                        .and_then(|f| f.uv_indices)
                        .and_then(|ids| ids.get(corner).copied())
                        .and_then(|i| original_template.uvs.get(i).copied())
                        .ok_or_else(|| format!("face {fi} esquina {c}: UV original inexistente"))
                };
                let expected = match *s {
                    CornerSource::Corner { face, corner } => Some(source_uv(face, corner)?),
                    CornerSource::Edge {
                        face,
                        corner_a,
                        corner_b,
                        t,
                    } => {
                        let a = source_uv(face, corner_a)?;
                        let b = source_uv(face, corner_b)?;
                        Some([a[0] * (1.0 - t) + b[0] * t, a[1] * (1.0 - t) + b[1] * t])
                    }
                    CornerSource::Ring { .. } => None, // UV de G dedicada: no modificar.
                };
                if let Some(expected) = expected {
                    // Exportador UV f32: error medido max 2.979e-8 (esquinas), 2.730e-8
                    // (cortes) en el paquete aprobado; admitir un epsilon f32, no 1e-2.
                    if expected
                        .iter()
                        .zip(f.uv[c])
                        .any(|(&a, b)| (a - b).abs() > f32::EPSILON as f64 * a.abs().max(1.0))
                    {
                        return Err(format!(
                            "face {fi} esquina {c}: UV distinta de su procedencia original"
                        ));
                    }
                }
            }
        }
        // mascaras por correspondencia
        let orig_set = |ids: &[usize], side: Side| -> Option<Vec<usize>> {
            let mut out = Vec::new();
            for &i in ids {
                match retained(i) {
                    Some((s, VertexSource::Orig { orig, .. })) if s == side => out.push(orig),
                    _ => return None,
                }
            }
            out.sort_unstable();
            Some(out)
        };
        if orig_set(&topo.root, Side::P).as_deref() != Some(manifest.root()) {
            return Err("mascara raiz distinta de la raiz del manifest de la captura".into());
        }
        if orig_set(&topo.grip, Side::D).as_deref() != Some(manifest.grip()) {
            return Err("mascara de agarre distinta del agarre del manifest de la captura".into());
        }
        if orig_set(&topo.coil, Side::D).is_none() {
            return Err("mascara de espiral fuera del lado distal".into());
        }
        // tramos de la captura = bordes de r(f) de la ley
        let fps = manifest.fps();
        let timing = base_timing(&capture)?;
        if timing.reach != (R_BREAKS_S[0], R_BREAKS_S[1])
            || timing.carry != (R_BREAKS_S[2], R_BREAKS_S[3])
        {
            return Err(format!(
                "tramos de la captura {:?}/{:?} distintos de los bordes de r(f) de la ley {R_BREAKS_S:?}",
                timing.reach, timing.carry
            ));
        }
        // esquinas del lazo para su normal (orden de caras y esquinas de Python)
        let mut loop_corners = vec![Vec::new(); k];
        for f in &topo.faces {
            if f.part == Part::G {
                continue;
            }
            for (c, s) in f.src.iter().enumerate() {
                if let Some(kk) = topo.loop_p.iter().position(|&x| x == f.v[c]) {
                    loop_corners[kk].push(*s);
                }
            }
        }
        if loop_corners.iter().any(|l| l.is_empty()) {
            return Err("algun punto del lazo no toca caras originales (normal indefinida)".into());
        }
        // plantilla de la malla con tramo
        let nf = topo.faces.len();
        let template = Model {
            vertices: vec![[0.0; 3]; topo.vertices.len()],
            uvs: topo.faces.iter().flat_map(|f| f.uv).collect(),
            normals: vec![[0.0, 0.0, 1.0]; 3 * nf],
            faces: (0..nf)
                .map(|fi| Face {
                    vertex_indices: topo.faces[fi].v,
                    uv_indices: Some([3 * fi, 3 * fi + 1, 3 * fi + 2]),
                    normal_indices: Some([3 * fi, 3 * fi + 1, 3 * fi + 2]),
                    material: material.clone(),
                })
                .collect(),
            material_libraries: original_template.material_libraries.clone(),
            materials: original_template.materials.clone(),
            bounds_center: [0.0; 3],
            bounds_radius: 0.0,
        };
        let grip_frame = manifest.grip_frame();
        let mut data = GrowthData {
            topology: topo,
            poses,
            arm_faces,
            loops: Vec::new(),
            loop_corners,
            fps,
            grip_frame,
            template,
            original_template: copy_model(&original_template),
        };
        let loops: Vec<LoopFrame> = (1..=data.poses.len())
            .map(|f| data.compute_loop(f, &data.poses[f - 1]))
            .collect();
        if loops.iter().any(|l| {
            !(finite3(l.tangent)
                && finite3(l.center)
                && l.contour_radius.is_finite()
                && l.contour_radius > 0.0
                && (length(l.tangent) - 1.0).abs() < 1e-9)
        }) {
            return Err("lazo degenerado en algun cuadro (tangente o radio invalidos)".into());
        }
        data.loops = loops;
        let mut template = copy_model(&data.template);
        data.evaluate(1, [0.0; 3], &mut template)?;
        template.update_bounds();
        data.template = template;
        data.original_pose(1, &mut original_template)?;
        data.original_template = original_template;
        Ok(Self { capture, data })
    }

    /// Lee la captura UNA vez (manifest y cada OBJ verificado por tamano y FNV-1a sobre los mismos bytes)
    /// y el paquete de crecimiento, y llama a `from_parts`. Un archivo ajeno o tocado da `Err`.
    pub fn load(capture_dir: &Path, growth_file: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(capture_dir.join("capture_package.txt"))
            .map_err(|e| format!("capture_package.txt: {e}"))?;
        let manifest = CaptureManifest::parse(&text)?;
        let read = |f: &PackageFile| -> Result<Model, String> {
            let bytes = std::fs::read(capture_dir.join(&f.relative))
                .map_err(|e| format!("{}: {e}", f.relative))?;
            if bytes.len() as u64 != f.bytes || fnv1a64(&bytes) != f.fnv1a64 {
                return Err(format!(
                    "{}: tamano o huella distintos del manifest (paquete ajeno)",
                    f.relative
                ));
            }
            let source =
                String::from_utf8(bytes).map_err(|_| format!("{}: no es UTF-8", f.relative))?;
            parse_obj(&source).map_err(|e| format!("{}: {e}", f.relative))
        };
        let files: Vec<&PackageFile> = manifest.files().collect();
        let (body_file, arm_files) = files.split_first().ok_or("manifest sin archivos")?;
        let body = read(body_file)?;
        let arms = arm_files
            .iter()
            .map(|f| read(f))
            .collect::<Result<Vec<_>, _>>()?;
        let growth = std::fs::read_to_string(growth_file)
            .map_err(|e| format!("{}: {e}", growth_file.display()))?;
        Self::from_parts(&text, arms, body, &growth)
    }
    pub fn capture(&self) -> &CapturePackage {
        &self.capture
    }
    pub fn data(&self) -> &GrowthData {
        &self.data
    }
}

fn side_of(part: Part) -> Side {
    if part == Part::P { Side::P } else { Side::D }
}

fn manifest_arm_vertices(poses: &[ArmPose]) -> Result<usize, String> {
    poses
        .first()
        .map(|p| p.positions.len())
        .ok_or("sin poses".into())
}

// ---------------- plan, tiempo y nave ----------------

fn finite_mat(m: &Mat4) -> bool {
    m.m.iter().flatten().all(|v| v.is_finite())
}

fn in_domain(v: Vec3) -> bool {
    [v.x, v.y, v.z]
        .iter()
        .all(|c| c.is_finite() && c.abs() <= NUMERIC_WORLD_LIMIT)
}

/// Escala uniforme de la parte lineal (mismas reglas que la captura) o `Err`.
fn rigid_scale(m: &Mat4, tolerance: f64) -> Result<f64, String> {
    let c = [m.column(0), m.column(1), m.column(2)];
    let s = c[0].length();
    let ok = s.is_finite()
        && s > 0.0
        && c.iter().all(|v| ((v.length() - s) / s).abs() <= tolerance)
        && (c[0].dot(c[1]) / (s * s)).abs() <= tolerance
        && (c[0].dot(c[2]) / (s * s)).abs() <= tolerance
        && (c[1].dot(c[2]) / (s * s)).abs() <= tolerance
        && c[0].cross(c[1]).dot(c[2]) > 0.0
        && m.m[3] == [0.0, 0.0, 0.0, 1.0];
    if ok {
        Ok(s)
    } else {
        Err("matriz no rigida (ortogonalidad, escala uniforme o mano)".into())
    }
}

/// Orientacion de la nave del World para una matriz de mundo (misma aritmetica que la captura).
fn ship_orientation(w: &Mat4) -> Mat4 {
    let rotation = w.orthonormalized() * Mat4::rotation_y(-SHIP_MODEL_YAW);
    let mut orientation = rotation.orthonormalized();
    orientation.m[3] = [0.0, 0.0, 0.0, 1.0];
    orientation
}

/// Inversa de la parte lineal (filas) de una matriz afin.
fn linear_inverse(m: &Mat4) -> Result<[Vec3; 3], String> {
    let (c0, c1, c2) = (m.column(0), m.column(1), m.column(2));
    let det = c0.dot(c1.cross(c2));
    let rows = [c1.cross(c2), c2.cross(c0), c0.cross(c1)].map(|r| r * (1.0 / det));
    if !(det.is_finite() && det != 0.0 && rows.iter().all(|r| r.is_finite())) {
        return Err("transform de la criatura no invertible".into());
    }
    Ok(rows)
}

/// Tramos de la captura, en segundos base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaseTiming {
    pub reach: (f64, f64),
    pub carry: (f64, f64),
    pub duration: f64,
}

fn base_timing(base: &CapturePackage) -> Result<BaseTiming, String> {
    let m = base.manifest();
    let st = m.stages();
    let find = |name: &str| st.iter().position(|(_, n)| n == name);
    let (Some(i), Some(j)) = (find("alcance"), find("arrastre")) else {
        return Err("faltan los tramos 'alcance' y 'arrastre'".into());
    };
    if !(i < j && j + 1 < st.len()) {
        return Err(
            "'alcance' debe ir antes de 'arrastre' y ambos deben tener tramo siguiente".into(),
        );
    }
    let (rs, re, cs, ce) = (st[i].0, st[i + 1].0, st[j].0, st[j + 1].0);
    if !(re <= m.grip_frame() && m.grip_frame() <= cs) {
        return Err("el agarre debe caer entre el fin del alcance y el inicio del arrastre".into());
    }
    let at = |f: usize| (f as f64 - 1.0) / m.fps() as f64;
    Ok(BaseTiming {
        reach: (at(rs), at(re)),
        carry: (at(cs), at(ce)),
        duration: base.duration_s(),
    })
}

/// Pose de la nave de un cuadro del plan (posicion, orientacion y matriz de mundo esperada).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipPose {
    pub position: Vec3,
    pub orientation: Mat4,
    pub expected: Mat4,
}

/// Plan inmutable: instantanea de la nave real, tiempos y, PRECALCULADOS y validados para TODOS los
/// cuadros, `o(f)` de la ley y la pose de la nave. Solo se crea con `GrowthPreview::plan`.
#[derive(Clone, Debug, PartialEq)]
pub struct GrowthPlan {
    target_position: Vec3,
    target_orientation: Mat4,
    delta_world: Vec3,
    delta_obj: [f64; 3],
    distance: f64,
    reference_distance: f64,
    factor: f64,
    base: BaseTiming,
    reach: (f64, f64),
    carry: (f64, f64),
    duration: f64,
    fps: usize,
    frames: usize,
    laws: Vec<LawSample>,
    ships: Vec<ShipPose>,
}

/// Instante del plan: tiempo base mostrado y cuadro de pose (indices separados del tiempo de reproduccion).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrowthSample {
    pub base_seconds: f64,
    pub frame: usize,
}

impl GrowthPlan {
    pub fn target_position(&self) -> Vec3 {
        self.target_position
    }
    pub fn target_orientation(&self) -> Mat4 {
        self.target_orientation
    }
    pub fn delta_world(&self) -> Vec3 {
        self.delta_world
    }
    /// Delta en espacio OBJ de la criatura (bounds ORIGINALES).
    pub fn delta_obj(&self) -> [f64; 3] {
        self.delta_obj
    }
    pub fn distance(&self) -> f64 {
        self.distance
    }
    pub fn reference_distance(&self) -> f64 {
        self.reference_distance
    }
    /// `max(1, d / d0)^REACH_TIME_EXPONENT` (PROVISIONAL de preview).
    pub fn factor(&self) -> f64 {
        self.factor
    }
    pub fn reach_window(&self) -> (f64, f64) {
        self.reach
    }
    pub fn carry_window(&self) -> (f64, f64) {
        self.carry
    }
    pub fn duration_s(&self) -> f64 {
        self.duration
    }
    pub fn base_timing(&self) -> BaseTiming {
        self.base
    }
    /// Objetivo EXACTO de referencia (`Delta` de mundo = 0): ruta de la captura original.
    pub fn is_reference(&self) -> bool {
        self.delta_world == Vec3::ZERO
    }
    pub fn law(&self, frame: usize) -> Result<&LawSample, String> {
        frame
            .checked_sub(1)
            .and_then(|i| self.laws.get(i))
            .ok_or(format!("cuadro {frame} fuera de 1..={}", self.frames))
    }
    pub fn ship(&self, frame: usize) -> Result<&ShipPose, String> {
        frame
            .checked_sub(1)
            .and_then(|i| self.ships.get(i))
            .ok_or(format!("cuadro {frame} fuera de 1..={}", self.frames))
    }

    /// Tiempo base y cuadro en `seconds`: identidad fuera de alcance/arrastre (desplazada); alcance y
    /// arrastre estirados por `factor`. Con `factor == 1` el tiempo base ES `seconds`. Cuadro = piso de
    /// `t_base * fps` + 1, clamp al ultimo (igual que la captura).
    pub fn sample(&self, seconds: f64) -> Result<GrowthSample, String> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(format!("tiempo invalido (finito, >= 0): {seconds}"));
        }
        let b = &self.base;
        let ((ra0, ra1), (rc0, rc1)) = (self.reach, self.carry);
        let base_seconds = if self.factor == 1.0 || seconds < ra0 {
            seconds
        } else if seconds < ra1 {
            let u = (seconds - ra0) / (ra1 - ra0);
            b.reach.0 + u * (b.reach.1 - b.reach.0)
        } else if seconds < rc0 {
            b.reach.1 + (seconds - ra1)
        } else if seconds < rc1 {
            let u = (seconds - rc0) / (rc1 - rc0);
            b.carry.0 + u * (b.carry.1 - b.carry.0)
        } else {
            b.carry.1 + (seconds - rc1)
        };
        if !base_seconds.is_finite() {
            return Err(format!("tiempo base no finito para {seconds}"));
        }
        let f = base_seconds * self.fps as f64 + 1e-9;
        let frame = if f >= self.frames as f64 {
            self.frames
        } else {
            f.floor() as usize + 1
        };
        Ok(GrowthSample {
            base_seconds,
            frame,
        })
    }

    /// Inversa de `sample` en el tiempo: instante del plan que muestra `base_seconds`.
    pub fn time_for_base(&self, base_seconds: f64) -> Result<f64, String> {
        if !(base_seconds.is_finite() && base_seconds >= 0.0) {
            return Err(format!("tiempo base invalido: {base_seconds}"));
        }
        if self.factor == 1.0 {
            return Ok(base_seconds);
        }
        let b = &self.base;
        let ((ra0, ra1), (rc0, rc1)) = (self.reach, self.carry);
        let t = if base_seconds < b.reach.0 {
            base_seconds
        } else if base_seconds < b.reach.1 {
            let u = (base_seconds - b.reach.0) / (b.reach.1 - b.reach.0);
            ra0 + u * (ra1 - ra0)
        } else if base_seconds < b.carry.0 {
            ra1 + (base_seconds - b.reach.1)
        } else if base_seconds < b.carry.1 {
            let u = (base_seconds - b.carry.0) / (b.carry.1 - b.carry.0);
            rc0 + u * (rc1 - rc0)
        } else {
            rc1 + (base_seconds - b.carry.1)
        };
        if !t.is_finite() {
            return Err(format!("tiempo no finito para la base {base_seconds}"));
        }
        Ok(t)
    }
}

// ---------------- preview ----------------

/// Vista de la preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrowthView {
    /// Camara de presentacion inspeccionada en Blender (la de la captura; por defecto).
    Presentation,
    /// CANDIDATA de la captura: formula de `snap_camera` con `chase_distance`/`chase_height` del World.
    ChaseCandidate,
    /// CANDIDATA: lateral fija por plan que encuadra criatura y objetivo (solo encuadre).
    OverviewCandidate,
}

/// Ruta de la ultima pose aplicada/dibujada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Captura ORIGINAL delegada (referencia en presentacion/seguimiento).
    OriginalCapture,
    /// Malla ORIGINAL aprobada dibujada por esta preview (referencia en la vista general).
    OriginalMesh,
    /// Malla con tramo dedicado y ley `fixed`.
    Growth,
}

/// Lo aplicado/dibujado en un instante (solo salida).
#[derive(Clone, Copy, Debug)]
pub struct GrowthFrame {
    pub frame: usize,
    pub seconds: f64,
    pub base_seconds: f64,
    pub reach_weight: f64,
    pub offset_obj: [f64; 3],
    pub clamped: bool,
    pub growth_length: f64,
    pub ship_position: Vec3,
    pub route: Route,
    pub stats: RenderStats,
}

struct Prepared {
    frame: usize,
    sample: GrowthSample,
    law: LawSample,
    ship: ShipPose,
    camera: Camera,
}

/// Preview: posee la captura ORIGINAL (con su textura; ruta de referencia), los datos de la ley, una
/// segunda copia de la textura (la de la captura es privada), el plan vigente y las mallas de trabajo
/// (mostrada y temporal) del tramo y del brazo original.
pub struct GrowthPreview {
    capture: CapturePreview,
    data: GrowthData,
    texture: Texture,
    arm: Model,
    scratch: Model,
    original: Model,
    original_scratch: Model,
    plan: Option<GrowthPlan>,
    lens: Option<(f64, f64)>,
    expected_ship: Option<(Vec3, Mat4)>,
    posed: (usize, Route),
    reference_position: Vec3,
    reference_orientation: Mat4,
    destination: Vec3,
    reference_distance: f64,
    inverse_linear: [Vec3; 3],
}

impl GrowthPreview {
    /// `texture` se llama DOS veces: una copia para la captura original delegada y otra para el tramo
    /// (`Texture` no es `Clone`; 48 MiB por copia con la textura real).
    pub fn new(
        package: GrowthPackage,
        mut texture: impl FnMut() -> Result<Texture, String>,
    ) -> Result<Self, String> {
        let GrowthPackage { capture, data } = package;
        let first = capture.ship_world_matrix(1)?;
        let last = capture.ship_world_matrix(capture.frames())?;
        if !(finite_mat(&first) && finite_mat(&last)) {
            return Err("matrices de la nave no finitas".into());
        }
        let reference_position = first.column(3);
        let destination = last.column(3);
        let reference_distance = (reference_position - destination).length();
        if !(in_domain(reference_position)
            && in_domain(destination)
            && reference_distance.is_finite()
            && reference_distance > 0.0)
        {
            return Err("la captura no desplaza la nave o sale del dominio numerico".into());
        }
        let inverse_linear = linear_inverse(&capture.creature_transform().matrix())?;
        let arm = copy_model(&data.template);
        let scratch = copy_model(&data.template);
        let original = copy_model(&data.original_template);
        let original_scratch = copy_model(&data.original_template);
        let capture_texture = texture()?;
        let own_texture = texture()?;
        Ok(Self {
            reference_orientation: ship_orientation(&first),
            capture: CapturePreview::new(capture, capture_texture),
            data,
            texture: own_texture,
            arm,
            scratch,
            original,
            original_scratch,
            plan: None,
            lens: None,
            expected_ship: None,
            posed: (1, Route::OriginalCapture),
            reference_position,
            destination,
            reference_distance,
            inverse_linear,
        })
    }
    pub fn capture_package(&self) -> &CapturePackage {
        self.capture.package()
    }
    pub fn data(&self) -> &GrowthData {
        &self.data
    }
    /// Pose de la nave del cuadro 1 de la captura (posicion, orientacion del World).
    pub fn reference_pose(&self) -> (Vec3, Mat4) {
        (self.reference_position, self.reference_orientation)
    }
    /// Posicion de mundo de la nave en el ultimo cuadro de la captura (destino aprobado).
    pub fn destination_world(&self) -> Vec3 {
        self.destination
    }
    /// Delta OBJ -> mundo (parte lineal del transform de la criatura con bounds ORIGINALES).
    pub fn obj_to_world(&self, d: [f64; 3]) -> Vec3 {
        self.capture
            .package()
            .creature_transform()
            .matrix()
            .transform_dir(Vec3::from_array(d))
    }
    fn world_to_obj(&self, d: Vec3) -> [f64; 3] {
        let [r0, r1, r2] = self.inverse_linear;
        [r0.dot(d), r1.dot(d), r2.dot(d)]
    }
    /// Pose de mundo de la nave para un caso con nombre: la referencia EXACTA (sin aritmetica) o
    /// `referencia + C_lineal * Delta_obj` del caso. Es la pose a COLOCAR en `World.ship` antes de `plan`.
    pub fn case_target(&self, case: GrowthCase) -> (Vec3, Mat4) {
        match case {
            GrowthCase::Reference => (self.reference_position, self.reference_orientation),
            c => (
                self.reference_position + self.obj_to_world(c.delta_obj()),
                self.reference_orientation,
            ),
        }
    }
    pub fn current_plan(&self) -> Option<&GrowthPlan> {
        self.plan.as_ref()
    }
    /// Malla del brazo tal como se aplico/dibujo por ultima vez (solo lectura; la ruta dice cual).
    pub fn posed_arm(&self) -> &Model {
        match self.posed.1 {
            Route::OriginalCapture => self.capture.posed_arm(),
            Route::OriginalMesh => &self.original,
            Route::Growth => &self.arm,
        }
    }
    /// Cuadro y ruta de la ultima pose aplicada/dibujada.
    pub fn posed(&self) -> (usize, Route) {
        self.posed
    }

    /// La nave del World debe ser la del paquete (bounds y huella de geometria) y valida.
    fn check_ship_model(&self, ship: &Model) -> Result<(), String> {
        // Mismas reglas de validate_model de la captura, antes de acceder a atributos o mutar.
        if ship.vertices.iter().flatten().any(|c| !c.is_finite())
            || ship.uvs.iter().flatten().any(|c| !c.is_finite())
            || ship.normals.iter().flatten().any(|c| !c.is_finite())
        {
            return Err("nave del World: valores no finitos".into());
        }
        for f in &ship.faces {
            if f.vertex_indices.iter().any(|&i| i >= ship.vertices.len())
                || f.uv_indices
                    .is_some_and(|ids| ids.iter().any(|&i| i >= ship.uvs.len()))
                || f.normal_indices
                    .is_some_and(|ids| ids.iter().any(|&i| i >= ship.normals.len()))
            {
                return Err(
                    "nave del World: cara con indice de vertice, UV o normal fuera de rango".into(),
                );
            }
        }
        let m = self.capture.package().manifest();
        let (sc, sr) = m.ship_bounds();
        if Vec3::from_array(ship.bounds_center) != sc
            || ship.bounds_radius != sr
            || capture::geometry_fnv(ship) != m.ship_geometry()
        {
            return Err(
                "la nave del World no es la nave del paquete (bounds o geometria distintos)".into(),
            );
        }
        Ok(())
    }

    /// Instantanea de la pose REAL de `world.ship` como objetivo. Calcula y valida `o(f)` y la nave de
    /// TODOS los cuadros antes de aceptar: un `Err` (dominio, orientacion, calibre, finitud) no cambia nada.
    /// El primer plan fija el lente (near, far) del World para todas las vistas.
    pub fn plan(&mut self, world: &World) -> Result<GrowthPlan, String> {
        let plan = self.build_plan(world)?;
        let lens = self.lens.unwrap_or((world.camera.near, world.camera.far));
        if !(lens.0.is_finite() && lens.1.is_finite() && 0.0 < lens.0 && lens.0 < lens.1) {
            return Err(format!("lente del World invalido: {lens:?}"));
        }
        self.lens = Some(lens);
        self.plan = Some(plan.clone());
        self.expected_ship = Some((world.ship.position, world.ship.orientation));
        Ok(plan)
    }

    fn build_plan(&self, world: &World) -> Result<GrowthPlan, String> {
        self.check_ship_model(&world.ship_model)?;
        let (position, orientation) = (world.ship.position, world.ship.orientation);
        if !in_domain(position) {
            return Err(format!(
                "posicion de la nave fuera del dominio numerico (finita, |c| <= {NUMERIC_WORLD_LIMIT:e}): {position:?}"
            ));
        }
        if !finite_mat(&orientation) {
            return Err("orientacion de la nave no finita".into());
        }
        for r in 0..4 {
            for c in 0..4 {
                let d = (orientation.m[r][c] - self.reference_orientation.m[r][c]).abs();
                if d.is_nan() || d > TARGET_ORIENTATION_TOLERANCE {
                    return Err(format!(
                        "orientacion de la nave distinta de la de referencia ({d:e}): 6DoF general NO implementado, solo se adapta la POSICION"
                    ));
                }
            }
        }
        let capture = self.capture.package();
        let manifest = capture.manifest();
        let delta_world = position - self.reference_position;
        let reference = delta_world == Vec3::ZERO;
        let delta_obj = if reference {
            [0.0; 3]
        } else {
            self.world_to_obj(delta_world)
        };
        if !(delta_world.is_finite() && finite3(delta_obj)) {
            return Err("Delta no finito".into());
        }
        if !reference && delta_obj == [0.0; 3] {
            return Err("Delta de mundo no nulo con Delta OBJ nulo (subdesbordamiento)".into());
        }
        let distance = (position - self.destination).length();
        let d0 = self.reference_distance;
        let factor = if distance <= d0 {
            1.0
        } else {
            (distance / d0).powf(REACH_TIME_EXPONENT)
        };
        if !(distance.is_finite() && factor.is_finite() && factor >= 1.0) {
            return Err("distancia o factor de tiempo no finitos".into());
        }
        let t = base_timing(capture)?;
        let (reach, carry, duration) = if factor == 1.0 {
            (t.reach, t.carry, t.duration)
        } else {
            let ra1 = t.reach.0 + factor * (t.reach.1 - t.reach.0);
            let rc0 = ra1 + (t.carry.0 - t.reach.1);
            let rc1 = rc0 + factor * (t.carry.1 - t.carry.0);
            ((t.reach.0, ra1), (rc0, rc1), rc1 + (t.duration - t.carry.1))
        };
        if !duration.is_finite() {
            return Err("duracion no finita".into());
        }
        let frames = capture.frames();
        let grip = manifest.grip_frame();
        let creature = capture.creature_transform().matrix();
        if !reference {
            // calibre real: C * (nave(1) + Delta_obj) debe ser la posicion del World
            let mut obj = manifest.ship_obj_matrix(1)?;
            for (r, d) in delta_obj.iter().enumerate() {
                obj.m[r][3] += d;
            }
            let p = (creature * obj).column(3);
            let scale = position.length().max(1.0);
            let err = [p.x - position.x, p.y - position.y, p.z - position.z];
            if err
                .iter()
                .any(|e| e.is_nan() || e.abs() > CALIBRATION_TOLERANCE * scale)
            {
                return Err(format!(
                    "calibre de la nave fallido: C * (nave(1) + Delta_obj) se separa {err:?} de la posicion real"
                ));
            }
        }
        let mut laws = Vec::with_capacity(frames);
        let mut ships = Vec::with_capacity(frames);
        for f in 1..=frames {
            let law = self.data.offset(f, delta_obj)?;
            let mut obj = manifest.ship_obj_matrix(f)?;
            let ship = if f < grip {
                let mut expected = creature * obj;
                (expected.m[0][3], expected.m[1][3], expected.m[2][3]) =
                    (position.x, position.y, position.z);
                ShipPose {
                    position,
                    orientation,
                    expected,
                }
            } else {
                for (r, d) in law.offset.iter().enumerate() {
                    obj.m[r][3] += d;
                }
                let expected = creature * obj;
                ShipPose {
                    position: expected.column(3),
                    orientation: ship_orientation(&expected),
                    expected,
                }
            };
            if !(finite_mat(&ship.expected) && in_domain(ship.position)) {
                return Err(format!("f{f}: nave fuera del dominio numerico o no finita"));
            }
            let s = rigid_scale(&ship.expected, SHIP_CHAIN_TOLERANCE)
                .map_err(|e| format!("f{f}: {e}"))?;
            if (s - 1.0).abs() > SHIP_CHAIN_TOLERANCE {
                return Err(format!("f{f}: la nave quedaria escalada en mundo: {s}"));
            }
            laws.push(law);
            ships.push(ship);
        }
        if laws[grip - 1].offset != delta_obj || laws[frames - 1].offset != [0.0; 3] {
            return Err("invariante violado: o(agarre) != Delta u o(final) != 0".into());
        }
        Ok(GrowthPlan {
            target_position: position,
            target_orientation: orientation,
            delta_world,
            delta_obj,
            distance,
            reference_distance: d0,
            factor,
            base: t,
            reach,
            carry,
            duration,
            fps: capture.fps(),
            frames,
            laws,
            ships,
        })
    }

    fn check_current_ship(&self, world: &World) -> Result<(), String> {
        let (ep, eo) = self.expected_ship.ok_or("sin pose vigente de la nave")?;
        if world.ship.position != ep || world.ship.orientation != eo {
            return Err(
                "plan obsoleto: la nave del World cambio desde el plan o el ultimo cuadro; vuelve a planificar"
                    .into(),
            );
        }
        Ok(())
    }

    fn view_camera(
        &self,
        world: &World,
        plan: &GrowthPlan,
        aspect: f64,
        ship: &ShipPose,
        view: GrowthView,
    ) -> Result<Camera, String> {
        let base = self.capture.package();
        let mut camera = world.camera;
        let (near, far) = self.lens.ok_or("sin lente: falta el plan")?;
        let pc = base.manifest().camera();
        let c = base.creature_transform().matrix();
        match view {
            GrowthView::Presentation => {
                camera.position = c.transform_point(pc.position);
                let forward = c.transform_dir(pc.forward).normalized();
                camera.target = camera.position + forward * PRESENTATION_TARGET_DISTANCE;
                camera.up = c.transform_dir(pc.up).normalized();
                camera.set_lens(pc.fov_y, aspect, near, far)?;
            }
            GrowthView::ChaseCandidate => {
                let (d, h) = (world.chase_distance, world.chase_height);
                if !(d.is_finite() && h.is_finite()) {
                    return Err("chase_distance/chase_height no finitos".into());
                }
                let back = ship.orientation.transform_dir(Vec3::Z);
                let up = ship.orientation.transform_dir(Vec3::Y);
                camera.position = ship.position + back * d + up * h;
                camera.target = ship.position - back * 30.0;
                camera.up = up;
                camera.set_lens(BASE_FOV, aspect, near, far)?;
            }
            GrowthView::OverviewCandidate => {
                let (bc, br) = base.manifest().creature_bounds();
                let creature = c.transform_point(bc);
                let creature_radius = br * base.creature_transform().scale.x;
                let target = plan.target_position;
                let mid = (creature + target) * 0.5;
                let extent = ((creature - mid).length() + creature_radius)
                    .max((target - mid).length() + world.ship_model.bounds_radius)
                    * OVERVIEW_MARGIN;
                let presentation = c.transform_point(pc.position);
                let up = c.transform_dir(pc.up).normalized();
                let line = target - creature;
                let cross = line.cross(up);
                let mut side = if cross.length() > 1e-9 * line.length().max(1.0) {
                    cross.normalized()
                } else {
                    c.transform_dir(pc.forward).normalized() * -1.0
                };
                if (presentation - mid).dot(side) < 0.0 {
                    side = side * -1.0;
                }
                let vertical_half = pc.fov_y * 0.5;
                let horizontal_half = (vertical_half.tan() * aspect).atan();
                let distance = extent / vertical_half.min(horizontal_half).sin();
                camera.position = mid + side * distance;
                camera.target = mid;
                camera.up = up;
                camera.set_lens(pc.fov_y, aspect, near, far.max(distance + 2.0 * extent))?;
            }
        }
        let vp = camera.view_projection();
        if !(camera.position.is_finite() && camera.target.is_finite() && camera.up.is_finite())
            || (camera.target - camera.position).length() <= 0.0
            || !finite_mat(&vp)
        {
            return Err("camara resultante no usable".into());
        }
        Ok(camera)
    }

    fn check_world(world: &World, width: usize, height: usize) -> Result<(), String> {
        if !(0.0..=1.0).contains(&world.debug_oxygen01) {
            return Err(format!(
                "O2 del mundo fuera de [0, 1]: {}",
                world.debug_oxygen01
            ));
        }
        if !(world.time.is_finite() && world.time >= 0.0) {
            return Err(format!("tiempo del mundo invalido: {}", world.time));
        }
        if world.creature().is_some() {
            return Err("el World ya tiene una criatura: se dibujarian dos cuerpos".into());
        }
        if width
            .checked_mul(height)
            .filter(|&n| n > 0 && n <= MAX_PIXELS)
            .is_none()
        {
            return Err(format!("cuadro invalido: {width}x{height}"));
        }
        Ok(())
    }

    /// Valida TODO sin tocar nada: plan, tiempo, World, nave (modelo, pose vigente, cadena), cuadro y
    /// camara (ruta propia: crecimiento o malla original en la vista general).
    fn prepare(
        &self,
        world: &World,
        width: usize,
        height: usize,
        seconds: f64,
        view: GrowthView,
    ) -> Result<Prepared, String> {
        let plan = self
            .plan
            .as_ref()
            .ok_or("sin plan: llama a plan(world) con la nave real en su sitio")?;
        let sample = plan.sample(seconds)?;
        Self::check_world(world, width, height)?;
        self.check_ship_model(&world.ship_model)?;
        self.check_current_ship(world)?;
        let law = *plan.law(sample.frame)?;
        let ship = *plan.ship(sample.frame)?;
        let mut probe = world.ship;
        probe.position = ship.position;
        probe.orientation = ship.orientation;
        let drawn = probe.transform(SHIP_MODEL_YAW).matrix();
        if !finite_mat(&drawn) {
            return Err("matriz dibujada de la nave no finita".into());
        }
        for (r, row) in drawn.m.iter().enumerate().take(3) {
            for (c, &value) in row.iter().enumerate() {
                let difference = (value - ship.expected.m[r][c]).abs();
                if difference.is_nan() || difference > SHIP_CHAIN_TOLERANCE {
                    return Err(format!(
                        "cadena de transforms de la nave rota: {difference}"
                    ));
                }
            }
        }
        let aspect = width as f64 / height as f64;
        let camera = self.view_camera(world, plan, aspect, &ship, view)?;
        Ok(Prepared {
            frame: sample.frame,
            sample,
            law,
            ship,
            camera,
        })
    }

    /// Escribe la pose del cuadro preparado en las mallas TEMPORALES (y valida su finitud).
    fn pose_scratch(&mut self, p: &Prepared, reference: bool) -> Result<(Route, f64), String> {
        if reference {
            self.data
                .original_pose(p.frame, &mut self.original_scratch)?;
            Ok((Route::OriginalMesh, 0.0))
        } else {
            let g = self.data.evaluate_with(p.law, &mut self.scratch)?;
            Ok((Route::Growth, g.growth_length))
        }
    }

    fn commit_pose(&mut self, route: Route, frame: usize) {
        match route {
            Route::Growth => std::mem::swap(&mut self.arm, &mut self.scratch),
            Route::OriginalMesh => std::mem::swap(&mut self.original, &mut self.original_scratch),
            Route::OriginalCapture => {}
        }
        self.posed = (frame, route);
    }

    fn capture_view(view: GrowthView) -> Option<CaptureView> {
        match view {
            GrowthView::Presentation => Some(CaptureView::Presentation),
            GrowthView::ChaseCandidate => Some(CaptureView::ChaseCandidate),
            GrowthView::OverviewCandidate => None,
        }
    }

    fn frame_out(
        p: &Prepared,
        seconds: f64,
        route: Route,
        length: f64,
        stats: RenderStats,
    ) -> GrowthFrame {
        GrowthFrame {
            frame: p.frame,
            seconds,
            base_seconds: p.sample.base_seconds,
            reach_weight: p.law.r,
            offset_obj: p.law.offset,
            clamped: p.law.clamped,
            growth_length: length,
            ship_position: p.ship.position,
            route,
            stats,
        }
    }

    /// Ruta de referencia delegada: la captura ORIGINAL con el lente fijado por el primer plan. Con
    /// `Err`, World intacto (la captura es atomica; aqui se restaura el lente).
    fn delegate(
        &mut self,
        world: &mut World,
        call: impl FnOnce(&mut CapturePreview, &mut World) -> Result<capture::CaptureFrame, String>,
    ) -> Result<GrowthFrame, String> {
        let plan = self.plan.as_ref().ok_or("sin plan")?;
        self.check_current_ship(world)?;
        self.check_ship_model(&world.ship_model)?;
        let (near, far) = self.lens.ok_or("sin lente: falta el plan")?;
        let saved = world.camera;
        world.camera.near = near;
        world.camera.far = far;
        match call(&mut self.capture, world) {
            Ok(f) => {
                let law = *plan.law(f.frame)?;
                self.posed = (f.frame, Route::OriginalCapture);
                self.expected_ship = Some((world.ship.position, world.ship.orientation));
                Ok(GrowthFrame {
                    frame: f.frame,
                    seconds: f.seconds,
                    base_seconds: f.seconds,
                    reach_weight: law.r,
                    offset_obj: law.offset,
                    clamped: false,
                    growth_length: 0.0,
                    ship_position: f.ship_position,
                    route: Route::OriginalCapture,
                    stats: f.stats,
                })
            }
            Err(e) => {
                world.camera = saved;
                Err(e)
            }
        }
    }

    fn delegated_view(&self, view: GrowthView) -> Option<CaptureView> {
        let plan = self.plan.as_ref()?;
        if plan.is_reference() {
            Self::capture_view(view)
        } else {
            None
        }
    }

    /// Aplica el instante al World (nave y camara) y a la malla mostrada, sin dibujar. Con `Err` nada
    /// cambia.
    pub fn apply(
        &mut self,
        world: &mut World,
        width: usize,
        height: usize,
        seconds: f64,
        view: GrowthView,
    ) -> Result<GrowthFrame, String> {
        if let Some(cv) = self.delegated_view(view) {
            return self.delegate(world, |c, w| c.apply(w, width, height, seconds, cv));
        }
        let p = self.prepare(world, width, height, seconds, view)?;
        let reference = self.plan.as_ref().is_some_and(|pl| pl.is_reference());
        let (route, length) = self.pose_scratch(&p, reference)?;
        self.commit_pose(route, p.frame);
        world.ship.position = p.ship.position;
        world.ship.orientation = p.ship.orientation;
        world.camera = p.camera;
        self.expected_ship = Some((p.ship.position, p.ship.orientation));
        Ok(Self::frame_out(
            &p,
            seconds,
            route,
            length,
            RenderStats::default(),
        ))
    }

    /// Cuadro completo en el World REAL, en el orden de la captura: World -> cuerpo -> brazo ->
    /// percepcion AL FINAL. Referencia exacta en presentacion/seguimiento: DELEGA en la captura original.
    /// Resto: cuadro y malla temporales que solo se intercambian si todo salio bien; un `Err` deja
    /// pixeles, profundidad, nave, camara y pose intactos.
    pub fn render(
        &mut self,
        world: &mut World,
        framebuffer: &mut Framebuffer,
        seconds: f64,
        view: GrowthView,
        perception: bool,
        with_arm: bool,
    ) -> Result<GrowthFrame, String> {
        if let Some(cv) = self.delegated_view(view) {
            return self.delegate(world, |c, w| {
                c.render(w, framebuffer, seconds, cv, perception, with_arm)
            });
        }
        let (w, h) = (framebuffer.width, framebuffer.height);
        let n = w
            .checked_mul(h)
            .filter(|&n| n > 0 && n <= MAX_PIXELS)
            .ok_or("cuadro invalido")?;
        if framebuffer.pixels.len() != n {
            return Err(format!(
                "el cuadro tiene {} pixeles para {w}x{h}",
                framebuffer.pixels.len()
            ));
        }
        let p = self.prepare(world, w, h, seconds, view)?;
        let reference = self.plan.as_ref().is_some_and(|pl| pl.is_reference());
        let (route, length) = self.pose_scratch(&p, reference)?;
        let shader = TexturedShader::new(
            &self.texture,
            TexturedMode::Lit,
            world.system.light_position(),
            AMBIENT,
        )?;
        let mut target = Framebuffer::new(w, h, SPACE_COLOR)?;
        let saved = (world.ship, world.camera);
        world.ship.position = p.ship.position;
        world.ship.orientation = p.ship.orientation;
        world.camera = p.camera;
        let mut stats = world.render(&mut target);
        let material = Material::from_model();
        let creature = self.capture.package().creature_transform();
        let arm = if reference {
            &self.original_scratch
        } else {
            &self.scratch
        };
        let mut models: Vec<&Model> = vec![self.capture.package().body()];
        if with_arm {
            models.push(arm);
        }
        for model in models {
            stats += draw_model_with(
                &mut target,
                model,
                &creature,
                &material,
                &world.camera,
                |f, _| shader.shade(f),
            );
        }
        if perception && let Err(e) = apply_lucid_perception(&mut target, world) {
            (world.ship, world.camera) = saved;
            return Err(e);
        }
        std::mem::swap(framebuffer, &mut target);
        self.commit_pose(route, p.frame);
        self.expected_ship = Some((p.ship.position, p.ship.orientation));
        Ok(Self::frame_out(&p, seconds, route, length, stats))
    }
}
