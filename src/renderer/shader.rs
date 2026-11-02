//! Punto de extension de shaders de fragmentos (UMB-105). Propietario: Persona 1;
//! las implementaciones concretas son de Persona 3 (ver `docs/architecture.md`).
//!
//! Los colores son lineales en `0..=1` (pueden excederlo para emision; se recortan al
//! convertir a sRGB 8 bits).

use crate::math::{Vec2, Vec3};
use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;

pub type Color = Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderKind {
    Flat,
    LitPlanet,
    AnimatedSun,
    LucidSurface,
    VoidEye,
}

#[derive(Clone, Copy, Debug)]
pub struct FragmentInput {
    /// Pixel (x, y) y profundidad NDC `[-1, 1]`.
    pub screen_position: Vec3,
    pub world_position: Vec3,
    /// Interpolada con correccion de perspectiva; el shader la renormaliza.
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct ShaderContext {
    pub elapsed_time: f64,
    /// `0.0..=1.0`; lo produce gameplay (Persona 2).
    pub oxygen01: f64,
    pub camera_position: Vec3,
    pub ship_position: Vec3,
    pub light_position: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// Color lineal base.
    pub base_color: Vec3,
    /// Si es verdadero, cada cara usa el `Kd` de su material MTL en lugar de `base_color`.
    pub use_model_colors: bool,
    /// Fraccion de luz que recibe una cara en sombra (`0..=1`).
    pub ambient: f64,
    /// Descarta triangulos de espaldas (solo para mallas cerradas con orientacion antihoraria).
    pub cull_backfaces: bool,
}

impl Material {
    pub fn solid(r: f64, g: f64, b: f64) -> Self {
        Self {
            base_color: Vec3::new(r, g, b),
            use_model_colors: false,
            ambient: 0.04,
            cull_backfaces: false,
        }
    }
    pub fn from_model() -> Self {
        Self {
            base_color: Vec3::splat(0.7),
            use_model_colors: true,
            ambient: 0.3,
            cull_backfaces: false,
        }
    }
}

pub fn shade_fragment(
    kind: ShaderKind,
    fragment: &FragmentInput,
    context: &ShaderContext,
    material: &Material,
) -> Color {
    match kind {
        ShaderKind::Flat => material.base_color,
        // UMB-309: preview aislado de la hipotesis «pupila del vacio» (docs/void-eye-preview.md).
        ShaderKind::LitPlanet => lit_planet(fragment, context, material),
        ShaderKind::VoidEye => void_eye(fragment, context, material),
        ShaderKind::LucidSurface => lucid_surface(fragment, context, material),
        ShaderKind::AnimatedSun => animated_sun(fragment, context, material),
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Difuso Lambert con terminador suave y un patron por UV (para que la rotacion se vea).
fn lit_planet(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let n = f.normal.normalized();
    let l = (c.light_position - f.world_position).normalized();
    let diffuse = smoothstep(-0.08, 0.35, n.dot(l));
    let pattern = 0.5
        + 0.25 * (f.uv.x * TAU * 6.0).sin() * (f.uv.y * PI * 4.0).sin()
        + 0.25 * (f.uv.y * PI * 9.0 + (f.uv.x * TAU * 3.0).sin()).sin();
    let albedo = m.base_color * (0.55 + 0.7 * pattern);
    albedo * (m.ambient + (1.0 - m.ambient) * diffuse)
}

/// Candidata UMB-308: el planeta iluminado con grietas rojo-ambar que crecen al bajar el
/// oxigeno. Entre grietas la piedra queda exactamente como `lit_planet`.
fn lucid_surface(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let base = lit_planet(f, c, m);
    // Un oxigeno no finito (NaN o +-Inf) se trata como estable: superficie sin grietas.
    if !c.oxygen01.is_finite() {
        return base;
    }
    let oxygen = c.oxygen01.clamp(0.0, 1.0);
    if oxygen >= LUCID_ONSET_OXYGEN {
        return base;
    }
    // Suave en ambos extremos: sin salto al cruzar el umbral y lleno desde el 5 %.
    let strength = 1.0 - smoothstep(LUCID_FULL_OXYGEN, LUCID_ONSET_OXYGEN, oxygen);
    // Direccion de la esfera unidad derivada de la UV (la convencion de `uv_sphere`): periodica
    // en u y sin singularidad en los polos, y anclada al planeta, no al espacio del mundo.
    let (sin_t, cos_t) = (f.uv.y * PI).sin_cos();
    let (sin_p, cos_p) = (f.uv.x * TAU).sin_cos();
    let direction = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
    let p = crack_domain(direction);
    let d = crack_edge_distance(p);
    // Cada zona se enciende cuando la intensidad supera su umbral: las grietas aparecen por
    // zonas al bajar el oxigeno, no como un fundido uniforme. Las primeras zonas empiezan en
    // cuanto baja de 50 % y desde intensidad 0.8 todas estan encendidas.
    let threshold = 0.8 * smoothstep(0.2, 0.8, smooth_field(direction, 0.0));
    let on = strength * smoothstep(threshold - 0.2, threshold, strength);
    // Solo parte de la red llega a encenderse (venas, no una malla completa); el resto queda
    // como fisura oscura y fina. El brillo varia a lo largo de cada vena.
    // Variante 02, solo en el tramo critico (O2 15 % -> 5 %); con O2 >= 15 % `critical` es 0 y
    // todo es identico a la 01. Para que la red no se lea como un balon de placas cerradas borra
    // aristas enteras (y el arranque de los brazos vecinos) y, para no limitarse a apagar,
    // despierta mas venas y ensancha de forma irregular las que quedan.
    let critical = 1.0 - smoothstep(CRITICAL_FULL_OXYGEN, CRITICAL_ONSET_OXYGEN, oxygen);
    let awake = smoothstep(0.35, 0.6, smooth_field(direction * 2.0, 5.0));
    // Mezcla lineal (y no mover los umbrales): el cambio por paso de O2 queda acotado.
    let awake = awake + critical * CRITICAL_WAKE * (1.0 - awake);
    let gain = awake * (0.55 + 0.45 * smooth_field(direction, 2.0));
    let (cut, widen) = if critical > 0.0 {
        let vary = smooth_field(direction * 3.0, 37.0);
        (
            critical * erased_edges(p),
            1.0 + critical * CRITICAL_WIDEN * vary,
        )
    } else {
        (0.0, 1.0)
    };
    let width = CRACK_WIDTH * (0.5 + 0.5 * on) * widen;
    let line = (1.0 - smoothstep(0.0, width, d)) * (1.0 - cut);
    // Borde carbonizado alrededor de la grieta; fuera de el `halo` es exactamente 0.
    let halo = (1.0 - smoothstep(0.0, 1.5 * width, d)) * (1.0 - cut);
    let charred = 0.35 * on * halo * (0.4 + 0.6 * awake);
    let ember = EMBER_RED.lerp(EMBER_AMBER, line * line);
    // El tiempo solo modula el brillo de la grieta (respiracion lenta), nunca su posicion.
    // Tiempo no finito: como t = 0 (solo en este shader; los demas no cambian).
    let time = if c.elapsed_time.is_finite() {
        c.elapsed_time
    } else {
        0.0
    };
    let phase = (time / PULSE_PERIOD_SECONDS).rem_euclid(1.0);
    let pulse = 1.0 - PULSE_DEPTH * 0.5 * (1.0 - (phase * TAU).cos());
    base * (1.0 - charred) + ember * (line * on * gain * pulse)
}

const PULSE_PERIOD_SECONDS: f64 = 4.5;
/// Fraccion maxima en que baja el brillo durante la pulsacion (nunca se apaga).
const PULSE_DEPTH: f64 = 0.25;

/// Con este oxigeno o mas la superficie es exactamente `lit_planet`.
const LUCID_ONSET_OXYGEN: f64 = 0.5;
/// Con este oxigeno o menos las grietas estan en su maximo.
const LUCID_FULL_OXYGEN: f64 = 0.05;
/// Tramo critico de la variante 02: entra en 15 % y esta completo desde 5 %.
const CRITICAL_ONSET_OXYGEN: f64 = 0.15;
const CRITICAL_FULL_OXYGEN: f64 = 0.05;
/// Fraccion de aristas que se borran enteras a O2 <= 5 %.
const EDGE_ERASED_SHARE: f64 = 0.36;
/// Medio ancho de la banda que borra una arista (cubre el borde carbonizado mas ancho) y su
/// transicion suave, en radios del planeta; la banda termina con un desvanecido de
/// `ERASE_TAPER` centrado algo antes de cada union (en el vertice borra 0.35 por arista).
const ERASE_RADIUS: f64 = 0.05;
const ERASE_SOFT: f64 = 0.01;
const ERASE_TAPER: f64 = 0.03;
/// A O2 <= 5 %: que fraccion de brillo reciben las venas dormidas y cuanto puede ensancharse
/// una grieta (segun un campo suave, no por arista).
const CRITICAL_WAKE: f64 = 0.85;
const CRITICAL_WIDEN: f64 = 0.5;
/// Placas de piedra (celdas de Voronoi sobre la esfera) y su desorden.
const CRACK_SITES: usize = 48;
const CRACK_JITTER: f64 = 0.7;
/// Ondulacion del trazo y ancho maximo de la grieta, en radios del planeta.
const CRACK_WARP: f64 = 0.04;
const CRACK_WIDTH: f64 = 0.02;
const EMBER_RED: Vec3 = Vec3::new(0.55, 0.06, 0.02);
const EMBER_AMBER: Vec3 = Vec3::new(1.0, 0.42, 0.08);

/// Dominio de las placas: la direccion con una leve ondulacion para que el trazo no se vea
/// geometrico.
fn crack_domain(direction: Vec3) -> Vec3 {
    let warp = Vec3::new(
        (direction.y * 7.3 + direction.z * 3.1).sin(),
        (direction.z * 6.7 - direction.x * 4.3).sin(),
        (direction.x * 7.9 + direction.y * 2.9).sin(),
    );
    (direction + warp * CRACK_WARP).normalized()
}

/// Distancia al borde de la placa mas cercana (Voronoi sobre la esfera unidad), funcion
/// continua del punto `p` de `crack_domain`.
fn crack_edge_distance(p: Vec3) -> f64 {
    let sites = crack_sites();
    let nearest = (0..sites.len())
        .max_by(|&a, &b| p.dot(sites[a]).total_cmp(&p.dot(sites[b])))
        .unwrap_or(0);
    let near = sites[nearest];
    // Distancia a cada plano bisector (todos pasan por el centro): el borde de la celda.
    sites
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != nearest)
        .map(|(_, &s)| (p.dot(near) - p.dot(s)) / (near - s).length())
        .fold(f64::INFINITY, f64::min)
}

/// Cuanto borra la variante 02 en `p` (`0..=1`). Es continua por construccion: la suma, sobre
/// aristas fijas, de una banda suave a lo largo de cada una, sin depender de que arista esta
/// mas cerca. La banda se desvanece al llegar a cada union: con una sola arista borrada los
/// otros dos brazos siguen unidos por el vertice (la grieta se desvia en V o en L en vez de
/// cerrar la Y); con dos o tres, las bandas suman y el vertice no queda como un punto suelto.
fn erased_edges(p: Vec3) -> f64 {
    erased_edge_list()
        .iter()
        .map(|e| {
            let across = p.dot(e.normal).abs();
            if across >= ERASE_RADIUS + ERASE_SOFT {
                return 0.0;
            }
            // Posicion a lo largo del arco, desde el primer extremo hacia el segundo.
            let along = e.ends[0].cross(p).dot(e.normal).atan2(e.ends[0].dot(p));
            let beyond = (-along).max(along - e.length);
            (1.0 - smoothstep(ERASE_RADIUS, ERASE_RADIUS + ERASE_SOFT, across))
                * (1.0 - smoothstep(-0.6 * e.taper, 0.4 * e.taper, beyond))
        })
        .sum::<f64>()
        .min(1.0)
}

/// Arista borrada: el gran circulo (normal) entre sus dos uniones triples, su longitud y su
/// desvanecido (mas corto en aristas cortas, para que tambien se borren del todo).
struct ErasedEdge {
    normal: Vec3,
    ends: [Vec3; 2],
    length: f64,
    taper: f64,
}

/// Aristas de la red de placas que se borran (deterministas: hash del par de placas).
fn erased_edge_list() -> &'static [ErasedEdge] {
    static EDGES: OnceLock<Vec<ErasedEdge>> = OnceLock::new();
    EDGES.get_or_init(|| {
        let s = crack_sites();
        // Uniones triples: centro del circulo por tres placas sin otra placa mas cerca.
        let mut junctions = Vec::new();
        for i in 0..s.len() {
            for j in i + 1..s.len() {
                for k in j + 1..s.len() {
                    let mut c = (s[j] - s[i]).cross(s[k] - s[i]).normalized();
                    if c.dot(s[i]) < 0.0 {
                        c = c * -1.0;
                    }
                    let r = c.dot(s[i]);
                    if s.iter().all(|q| q.dot(c) <= r + 1e-12) {
                        junctions.push((c, [i, j, k]));
                    }
                }
            }
        }
        // Una arista une las dos uniones que comparten un par de placas.
        let mut edges = Vec::new();
        for (n, &(a, plates_a)) in junctions.iter().enumerate() {
            for &(b, plates_b) in &junctions[n + 1..] {
                let shared: Vec<usize> = plates_a
                    .iter()
                    .copied()
                    .filter(|x| plates_b.contains(x))
                    .collect();
                if let [x, y] = shared[..] {
                    let length = a.dot(b).clamp(-1.0, 1.0).acos();
                    // Las aristas muy cortas se borran siempre: si sus vecinas desaparecen,
                    // quedarian como un punto suelto.
                    let key = x.min(y) * CRACK_SITES + x.max(y);
                    // (Dos uniones identicas no forman arista: se descarta el caso degenerado.)
                    let erased =
                        length < 2.0 * ERASE_TAPER || unit_hash(key, 41) < EDGE_ERASED_SHARE;
                    if length > 1e-9 && erased {
                        edges.push(ErasedEdge {
                            normal: a.cross(b).normalized(),
                            ends: [a, b],
                            length,
                            taper: ERASE_TAPER.min(length / 1.5),
                        });
                    }
                }
            }
        }
        edges
    })
}

/// Campo suave en `0..=1` sobre la esfera (suma de senos de la direccion: sin costuras).
fn smooth_field(d: Vec3, seed: f64) -> f64 {
    let wave = 0.5 * (d.x * 2.3 + d.y * 1.1 + seed).sin()
        + 0.3 * (d.y * 3.7 - d.z * 2.9 + 1.7 * seed).sin()
        + 0.2 * (d.z * 5.3 + d.x * 4.1 - seed).sin();
    0.5 + 0.5 * wave
}

/// Centros de las placas: espiral de Fibonacci con desorden determinista, sobre la esfera.
fn crack_sites() -> &'static [Vec3] {
    static SITES: OnceLock<Vec<Vec3>> = OnceLock::new();
    SITES.get_or_init(|| {
        let n = CRACK_SITES as f64;
        let spacing = (4.0 * PI / n).sqrt();
        let golden = PI * (3.0 - 5f64.sqrt());
        (0..CRACK_SITES)
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f64 + 0.5) / n;
                let r = (1.0 - y * y).sqrt();
                let (sin_a, cos_a) = (golden * i as f64).sin_cos();
                let jitter =
                    Vec3::new(unit_hash(i, 1), unit_hash(i, 2), unit_hash(i, 3)) - Vec3::splat(0.5);
                (Vec3::new(r * cos_a, y, r * sin_a) + jitter * (CRACK_JITTER * spacing))
                    .normalized()
            })
            .collect()
    })
}

/// Numero pseudoaleatorio determinista en `0..1` (splitmix64).
fn unit_hash(i: usize, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt;
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// UMB-309, hipotesis «pupila del vacio»: la piedra iluminada, mas oscura, con una pupila negra
/// donde la superficie mira a la nave y un iris lleno por una sola corona rojo-ambar ancha y
/// continua que respira lentamente con el tiempo (variante 02; la 01 tenia anillos finos).
///
/// Politica geometrica: el ojo solo usa la normal del fragmento y la direccion del fragmento a
/// la nave (`ship_position - world_position`). No supone el centro ni el radio del cuerpo: en
/// cualquier esfera, `normal . direccion` es maximo en el punto que mira a la nave.
fn void_eye(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let base = lit_planet(f, c, m) * VOID_STONE;
    // Direccion a la nave, escalada antes de normalizar para que una nave enorme pero finita no
    // desborde. Nave no finita o coincidente con el fragmento: no hay a donde mirar y el ojo
    // queda cerrado (solo la piedra oscura).
    let offset = c.ship_position - f.world_position;
    let scale = offset.x.abs().max(offset.y.abs()).max(offset.z.abs());
    if !(scale.is_finite() && scale > 0.0) {
        return base;
    }
    let to_ship = (offset * (1.0 / scale)).normalized();
    // Angulo entre la normal y la direccion a la nave: 0 en el centro de la pupila.
    let angle = f.normal.normalized().dot(to_ship).clamp(-1.0, 1.0).acos();
    let pupil = 1.0 - smoothstep(PUPIL_RADIUS - PUPIL_SOFT, PUPIL_RADIUS + PUPIL_SOFT, angle);
    // Iris: banda entre la pupila e `IRIS_RADIUS`, llena por una sola corona ancha y continua.
    // El tiempo solo la hace respirar: su brillo sube y baja a la vez en toda la banda.
    let inner = PUPIL_RADIUS + PUPIL_SOFT;
    let iris = smoothstep(inner, inner + 0.06, angle)
        * (1.0 - smoothstep(IRIS_RADIUS - 0.12, IRIS_RADIUS, angle));
    // Tiempo no finito: como t = 0 (solo en este shader). Se reduce a una fraccion de periodo
    // para que un tiempo enorme no pierda la precision de la fase.
    let time = if c.elapsed_time.is_finite() {
        c.elapsed_time
    } else {
        0.0
    };
    let breath = 0.5 - 0.5 * ((time / RING_PERIOD_SECONDS).rem_euclid(1.0) * TAU).cos();
    let ring = 1.0 - CORONA_BREATH * breath;
    let glow = IRIS_RED.lerp(IRIS_AMBER, ring) * (IRIS_GLOW * iris * ring);
    base * (1.0 - PUPIL_DEPTH * pupil - IRIS_SHADE * iris) + glow
}

/// Cuanto de la piedra iluminada conserva el ojo del vacio fuera de la pupila y el iris.
const VOID_STONE: f64 = 0.6;
/// Radio angular de la pupila (radianes desde la direccion a la nave) y su borde suave.
const PUPIL_RADIUS: f64 = 0.3;
const PUPIL_SOFT: f64 = 0.04;
/// Cuanto oscurece la pupila (casi negra, sin emision).
const PUPIL_DEPTH: f64 = 0.96;
/// Borde exterior del iris (radianes desde la direccion a la nave) y cuanto oscurece la piedra.
const IRIS_RADIUS: f64 = 0.75;
const IRIS_SHADE: f64 = 0.35;
/// Periodo de la respiracion de la corona (segundos) y cuanto baja su brillo en el punto mas
/// tenue (la corona nunca se apaga ni se mueve).
const RING_PERIOD_SECONDS: f64 = 3.0;
const CORONA_BREATH: f64 = 0.2;
/// Acento de la corona: rojo profundo hacia ambar en su brillo maximo, emision discreta.
const IRIS_RED: Vec3 = Vec3::new(0.35, 0.04, 0.02);
const IRIS_AMBER: Vec3 = Vec3::new(0.8, 0.3, 0.06);
const IRIS_GLOW: f64 = 0.12;

/// Emisivo: turbulencia que cambia con `elapsed_time`, pulsacion y oscurecimiento del borde.
fn animated_sun(f: &FragmentInput, c: &ShaderContext, m: &Material) -> Color {
    let t = c.elapsed_time;
    let warp = (f.uv.y * PI * 6.0 + t * 1.3).sin() * 1.8;
    let pattern = 0.5 + 0.5 * (f.uv.x * TAU * 8.0 + t * 0.8 + warp).sin();
    let pattern = pattern * (0.6 + 0.4 * (f.uv.y * PI * 11.0 - t * 0.5).sin().abs());
    let view = (c.camera_position - f.world_position).normalized();
    let facing = f.normal.normalized().dot(view).abs().clamp(0.0, 1.0);
    let hot = Vec3::new(1.0, 0.95, 0.55);
    let pulse = 0.92 + 0.08 * (t * 2.0).sin();
    m.base_color.lerp(hot, pattern) * ((0.55 + 0.6 * facing.sqrt()) * pulse)
}

/// Lineal `0..=1` a sRGB de 8 bits empaquetado `0xRRGGBB`, con tabla para no llamar `powf`.
pub fn to_rgb8(color: Color) -> u32 {
    static TABLE: OnceLock<Vec<u8>> = OnceLock::new();
    const STEPS: usize = 4096;
    let table = TABLE.get_or_init(|| {
        (0..=STEPS)
            .map(|i| {
                let l = i as f64 / STEPS as f64;
                let s = if l <= 0.003_130_8 {
                    l * 12.92
                } else {
                    1.055 * l.powf(1.0 / 2.4) - 0.055
                };
                (s * 255.0 + 0.5) as u8
            })
            .collect()
    });
    let ch = |v: f64| {
        let v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        table[(v * STEPS as f64 + 0.5) as usize] as u32
    };
    (ch(color.x) << 16) | (ch(color.y) << 8) | ch(color.z)
}
