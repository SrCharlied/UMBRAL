//! Espacio procedural alrededor del sistema solar: planetas errantes y tanques de oxigeno que se
//! generan por **sectores** (celdas cubicas de `SECTOR_SIZE`) conforme la nave avanza.
//!
//! - **Determinista:** el contenido de un sector depende solo de sus coordenadas enteras, asi
//!   que al volver se encuentran los mismos planetas en el mismo lugar.
//! - **Streaming:** `SectorField::update` genera los sectores a `SECTOR_RANGE` celdas de la nave
//!   y descarta los que quedan mas lejos. Los tanques recogidos se recuerdan por id aunque su
//!   sector se descarte, hasta la partida nueva.
//! - **Fuera del sistema:** nada se genera a menos de `INNER_EXCLUSION` del Sol (la orbita de
//!   Vigilia es 520), para no tapar ni chocar con los planetas de la historia.
//! - Los planetas son estaticos (giran sobre su eje), colisionan con la nave y llevan props de
//!   `planet_props::procedural_layers`. Cada uno tiene un tanque de oxigeno en orbita fija.

use std::collections::{HashMap, HashSet};

use crate::{
    math::{Transform, Vec3},
    visual::planet_props::{PlanetProps, build_planet_props, procedural_layers},
};

/// Lado de un sector, en unidades de mundo.
pub const SECTOR_SIZE: f64 = 700.0;
/// Sectores generados alrededor del de la nave en cada eje (2 -> cubo de 5x5x5).
pub const SECTOR_RANGE: i64 = 2;
/// Radio alrededor del Sol sin contenido procedural.
pub const INNER_EXCLUSION: f64 = 800.0;
/// Probabilidad de que un sector tenga planeta.
pub const PLANET_CHANCE: f64 = 0.5;
/// Probabilidad de un tanque suelto adicional por sector.
pub const FREE_TANK_CHANCE: f64 = 0.35;

/// Probabilidad de que un sector tenga un campo de asteroides.
pub const ASTEROID_FIELD_CHANCE: f64 = 0.55;
/// O2 que se pierde en cada choque con un asteroide (fuga, no dano a la nave).
pub const ASTEROID_OXYGEN_LOSS: f64 = 0.08;
/// Segundos separado de la misma roca para que un nuevo choque vuelva a perder O2.
pub const ASTEROID_CONTACT_GAP: f64 = 1.0;
/// Cinturon de asteroides del sistema: entre las orbitas de Osario (340) y Vigilia (520).
pub const BELT_INNER: f64 = 395.0;
pub const BELT_OUTER: f64 = 470.0;
pub const BELT_COUNT: usize = 150;
/// Radio libre de planetas errantes y asteroides alrededor de cada baliza de registro.
pub const LOG_CLEARANCE: f64 = 220.0;

/// Balizas de registro del espacio profundo (experiencia por defecto): a ~1800 del Sol y a
/// ~3100 entre si, repartidas a 120 grados. La mas cercana al arranque queda detras de la nave.
pub const DEEP_LOG_POSITIONS: [Vec3; 3] = [
    Vec3::new(1560.0, 120.0, -900.0),
    Vec3::new(-1560.0, -150.0, -900.0),
    Vec3::new(0.0, 200.0, 1800.0),
];

pub type SectorKey = (i64, i64, i64);

/// Asteroide estatico que gira sobre si mismo. Chocar pierde O2 (`ASTEROID_OXYGEN_LOSS`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asteroid {
    pub id: u64,
    pub center: Vec3,
    pub radius: f64,
    /// Variante de malla (`asteroid_mesh`).
    pub variant: usize,
    /// Rad/s de giro propio.
    pub spin: f64,
}

impl Asteroid {
    pub fn transform(&self, time: f64) -> Transform {
        let a = (self.spin * time).rem_euclid(std::f64::consts::TAU);
        Transform {
            position: self.center,
            rotation: Vec3::new(a * 0.7, a, 0.0),
            scale: Vec3::splat(self.radius),
        }
    }
}

/// Variantes de malla de asteroide.
pub const ASTEROID_VARIANTS: usize = 3;

/// Roca irregular: esfera de pocos anillos con los vertices desplazados de forma determinista
/// (radio entre 0.7 y 1.0). Las normales siguen siendo radiales (sombreado suave aproximado).
pub fn asteroid_mesh(variant: usize) -> crate::assets::obj::Model {
    let mut model = crate::assets::primitives::uv_sphere(5, 8);
    let seed = 0xA57E_0000 + variant as u64;
    // Los vertices duplicados de la costura comparten direccion: se desplazan igual.
    for v in &mut model.vertices {
        let p = Vec3::from_array(*v);
        let key = ((p.x * 97.0).round() as i64).wrapping_mul(73_856_093)
            ^ ((p.y * 97.0).round() as i64).wrapping_mul(19_349_663)
            ^ ((p.z * 97.0).round() as i64).wrapping_mul(83_492_791);
        let k = 0.7 + 0.3 * unit(seed ^ key as u64, 1);
        *v = [p.x * k, p.y * k * 0.85, p.z * k];
    }
    model.update_bounds();
    model
}

fn near_log(p: Vec3, margin: f64) -> bool {
    DEEP_LOG_POSITIONS
        .iter()
        .any(|log| (p - *log).length() < LOG_CLEARANCE + margin)
}

/// Cinturon del sistema (determinista).
pub fn asteroid_belt() -> Vec<Asteroid> {
    (0..BELT_COUNT)
        .map(|i| {
            let s = 0xBE17_0000 + i as u64;
            let angle = std::f64::consts::TAU * (i as f64 + unit(s, 1) * 0.8) / BELT_COUNT as f64;
            let r = BELT_INNER + (BELT_OUTER - BELT_INNER) * unit(s, 2);
            Asteroid {
                id: s,
                center: Vec3::new(r * angle.cos(), (unit(s, 3) - 0.5) * 40.0, r * angle.sin()),
                radius: 2.5 + 5.5 * unit(s, 4),
                variant: (unit(s, 5) * ASTEROID_VARIANTS as f64) as usize % ASTEROID_VARIANTS,
                spin: 0.1 + 0.6 * unit(s, 6),
            }
        })
        .collect()
}

/// Planeta errante de un sector.
#[derive(Debug)]
pub struct SectorPlanet {
    pub id: u64,
    pub center: Vec3,
    pub radius: f64,
    pub color: [f64; 3],
    /// Rad/s alrededor de su eje Y.
    pub spin: f64,
    /// Props (puede faltar si la distribucion no acepto ninguno).
    pub props: Option<PlanetProps>,
}

impl SectorPlanet {
    pub fn transform(&self, time: f64) -> Transform {
        Transform {
            position: self.center,
            rotation: Vec3::new(
                0.0,
                (self.spin * time).rem_euclid(std::f64::consts::TAU),
                0.0,
            ),
            scale: Vec3::splat(self.radius),
        }
    }
}

/// Tanque de oxigeno en una posicion fija del mundo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OxygenTank {
    pub id: u64,
    pub position: Vec3,
}

#[derive(Debug)]
pub struct Sector {
    pub key: SectorKey,
    pub planets: Vec<SectorPlanet>,
    pub tanks: Vec<OxygenTank>,
    pub asteroids: Vec<Asteroid>,
}

/// Sectores vivos alrededor de la nave y tanques ya recogidos.
#[derive(Debug, Default)]
pub struct SectorField {
    sectors: HashMap<SectorKey, Sector>,
    collected: HashSet<u64>,
    center: Option<SectorKey>,
    /// Cinturon fijo del sistema (vacio con `Default`; `new` lo genera).
    belt: Vec<Asteroid>,
}

impl SectorField {
    pub fn new() -> Self {
        Self {
            belt: asteroid_belt(),
            ..Self::default()
        }
    }

    /// Cinturon del sistema y asteroides de los sectores vivos.
    pub fn asteroids(&self) -> impl Iterator<Item = &Asteroid> {
        self.belt
            .iter()
            .chain(self.sectors.values().flat_map(|s| s.asteroids.iter()))
    }

    /// Como `resolve_collision`, contra los asteroides: saca `end` del primero que atraviese
    /// y devuelve su id.
    pub fn hit_asteroid(&self, start: Vec3, end: &mut Vec3, margin: f64) -> Option<u64> {
        if !(start.is_finite() && end.is_finite()) {
            return None;
        }
        let mut hit = None;
        for a in self.asteroids() {
            if let Some(pushed) = push_out(start, *end, a.center, a.radius + margin) {
                *end = pushed;
                hit = Some(a.id);
            }
        }
        hit
    }

    pub fn key_of(position: Vec3) -> SectorKey {
        let k = |v: f64| (v / SECTOR_SIZE).floor() as i64;
        (k(position.x), k(position.y), k(position.z))
    }

    /// Genera los sectores alrededor de `ship` y descarta los lejanos. Solo trabaja cuando la
    /// nave cambia de sector. Ignora posiciones no finitas.
    pub fn update(&mut self, ship: Vec3) {
        if !ship.is_finite() {
            return;
        }
        let center = Self::key_of(ship);
        if self.center == Some(center) {
            return;
        }
        self.center = Some(center);
        let near = |k: &SectorKey| {
            (k.0 - center.0).abs() <= SECTOR_RANGE
                && (k.1 - center.1).abs() <= SECTOR_RANGE
                && (k.2 - center.2).abs() <= SECTOR_RANGE
        };
        self.sectors.retain(|k, _| near(k));
        for dx in -SECTOR_RANGE..=SECTOR_RANGE {
            for dy in -SECTOR_RANGE..=SECTOR_RANGE {
                for dz in -SECTOR_RANGE..=SECTOR_RANGE {
                    let key = (center.0 + dx, center.1 + dy, center.2 + dz);
                    self.sectors.entry(key).or_insert_with(|| generate(key));
                }
            }
        }
    }

    pub fn planets(&self) -> impl Iterator<Item = &SectorPlanet> {
        self.sectors.values().flat_map(|s| s.planets.iter())
    }

    /// Tanques aun no recogidos.
    pub fn tanks(&self) -> impl Iterator<Item = &OxygenTank> {
        self.sectors
            .values()
            .flat_map(|s| s.tanks.iter())
            .filter(|t| !self.collected.contains(&t.id))
    }

    pub fn sector_count(&self) -> usize {
        self.sectors.len()
    }

    /// Marca recogidos los tanques que el segmento `start -> end` pasa a `radius` o menos.
    /// Devuelve cuantos se recogieron.
    pub fn collect_along(&mut self, start: Vec3, end: Vec3, radius: f64) -> u32 {
        if !(start.is_finite() && end.is_finite() && radius.is_finite()) {
            return 0;
        }
        let hits: Vec<u64> = self
            .tanks()
            .filter(|t| segment_distance(start, end, t.position) <= radius)
            .map(|t| t.id)
            .collect();
        let count = hits.len() as u32;
        self.collected.extend(hits);
        count
    }

    /// Partida nueva: todos los tanques vuelven a estar disponibles.
    pub fn reset_collected(&mut self) {
        self.collected.clear();
    }

    /// Saca la posicion `end` (tras moverse desde `start`) de cualquier planeta que atraviese,
    /// dejandola a `margin` de su superficie por el lado de entrada. Devuelve si hubo contacto.
    pub fn resolve_collision(&self, start: Vec3, end: &mut Vec3, time: f64, margin: f64) -> bool {
        let _ = time; // los planetas no se trasladan; el giro no cambia su esfera
        if !(start.is_finite() && end.is_finite()) {
            return false;
        }
        let mut hit = false;
        for planet in self.planets() {
            if let Some(pushed) = push_out(start, *end, planet.center, planet.radius + margin) {
                *end = pushed;
                hit = true;
            }
        }
        hit
    }
}

/// Si el segmento `start -> end` entra en la esfera `(center, radius)`, devuelve `end` sacado a
/// su superficie por el lado de entrada (o por donde esta, si ya empezaba dentro).
fn push_out(start: Vec3, end: Vec3, center: Vec3, radius: f64) -> Option<Vec3> {
    if (closest_point(start, end, center) - center).length() >= radius {
        return None;
    }
    let delta = end - start;
    let rel = start - center;
    let a = delta.dot(delta);
    let b = rel.dot(delta);
    let c = rel.dot(rel) - radius * radius;
    let entry = if c > 0.0 && a > 1e-12 {
        let disc = (b * b - a * c).max(0.0);
        let t = ((-b - disc.sqrt()) / a).clamp(0.0, 1.0);
        start + delta * t
    } else {
        end
    };
    let normal = (entry - center).normalized();
    let normal = if normal == Vec3::ZERO {
        Vec3::Y
    } else {
        normal
    };
    Some(center + normal * (radius + 1e-6))
}

fn closest_point(start: Vec3, end: Vec3, point: Vec3) -> Vec3 {
    let delta = end - start;
    let length_squared = delta.dot(delta);
    if length_squared <= 0.0 {
        return start;
    }
    let t = ((point - start).dot(delta) / length_squared).clamp(0.0, 1.0);
    start + delta * t
}

fn segment_distance(start: Vec3, end: Vec3, point: Vec3) -> f64 {
    (closest_point(start, end, point) - point).length()
}

/// Valor en `[0, 1)` determinista por `(semilla, flujo)` (finalizador de SplitMix64).
fn unit(seed: u64, stream: u64) -> f64 {
    let mut z = seed
        .wrapping_add(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

fn sector_seed(key: SectorKey) -> u64 {
    let mix = |v: i64, m: u64| (v as u64).wrapping_mul(m);
    mix(key.0, 0x9E37_79B9_7F4A_7C15)
        ^ mix(key.1, 0xC2B2_AE3D_27D4_EB4F)
        ^ mix(key.2, 0x1656_67B1_9E37_79F9)
        ^ 0x0DD5_EC70_25EE_D000
}

/// Paleta de suelos: oxido, hielo, violeta, verde gris, arena.
const PALETTE: [[f64; 3]; 5] = [
    [0.55, 0.32, 0.22],
    [0.62, 0.72, 0.8],
    [0.42, 0.3, 0.55],
    [0.38, 0.46, 0.38],
    [0.7, 0.6, 0.42],
];

/// Contenido determinista de un sector.
pub fn generate(key: SectorKey) -> Sector {
    let seed = sector_seed(key);
    let origin = Vec3::new(key.0 as f64, key.1 as f64, key.2 as f64) * SECTOR_SIZE;
    let inside = |u: f64, margin: f64| margin + u * (SECTOR_SIZE - 2.0 * margin);
    let mut sector = Sector {
        key,
        planets: Vec::new(),
        tanks: Vec::new(),
        asteroids: Vec::new(),
    };
    if unit(seed, 1) < PLANET_CHANCE {
        let radius = 10.0 + 24.0 * unit(seed, 2);
        let margin = radius + 60.0;
        let center = origin
            + Vec3::new(
                inside(unit(seed, 3), margin),
                inside(unit(seed, 4), margin) * 0.6 + SECTOR_SIZE * 0.2,
                inside(unit(seed, 5), margin),
            );
        if center.length() > INNER_EXCLUSION + radius && !near_log(center, radius) {
            let base = PALETTE[(unit(seed, 6) * PALETTE.len() as f64) as usize % PALETTE.len()];
            let shade = 0.8 + 0.4 * unit(seed, 7);
            let color = base.map(|c| (c * shade).clamp(0.0, 1.0));
            let props = build_planet_props(&procedural_layers(seed, color)).ok();
            let id = seed;
            // Tanque en "orbita" fija: a 10 unidades sobre una direccion del planeta.
            let dir = Vec3::new(
                unit(seed, 8) * 2.0 - 1.0,
                0.4 + 0.6 * unit(seed, 9),
                unit(seed, 10) * 2.0 - 1.0,
            )
            .normalized();
            sector.tanks.push(OxygenTank {
                id: id ^ 0x7A4C,
                position: center + dir * (radius + 10.0),
            });
            sector.planets.push(SectorPlanet {
                id,
                center,
                radius,
                color,
                spin: 0.05 + 0.25 * unit(seed, 11),
                props,
            });
        }
    }
    if unit(seed, 12) < FREE_TANK_CHANCE {
        let position = origin
            + Vec3::new(
                inside(unit(seed, 13), 40.0),
                inside(unit(seed, 14), 40.0),
                inside(unit(seed, 15), 40.0),
            );
        let clear = sector
            .planets
            .iter()
            .all(|p| (position - p.center).length() > p.radius + 25.0);
        if position.length() > INNER_EXCLUSION
            && clear
            && !near_log(position, -LOG_CLEARANCE + 60.0)
        {
            sector.tanks.push(OxygenTank {
                id: seed ^ 0xF4EE,
                position,
            });
        }
    }
    add_asteroid_field(&mut sector, seed, origin);
    sector
}

/// Campo de asteroides de un sector: un cumulo de 5 a 14 rocas alrededor de un punto, lejos
/// del Sol, de los planetas del sector, de sus tanques y de las balizas de registro.
fn add_asteroid_field(sector: &mut Sector, seed: u64, origin: Vec3) {
    let s = seed ^ 0xA57E_401D;
    if unit(s, 1) >= ASTEROID_FIELD_CHANCE {
        return;
    }
    let inside = |u: f64| 90.0 + u * (SECTOR_SIZE - 180.0);
    let cluster = origin + Vec3::new(inside(unit(s, 2)), inside(unit(s, 3)), inside(unit(s, 4)));
    let count = 5 + (unit(s, 5) * 10.0) as usize;
    for i in 0..count {
        let r = s.wrapping_add(i as u64 * 0x9E37);
        let offset =
            Vec3::new(unit(r, 6) - 0.5, (unit(r, 7) - 0.5) * 0.6, unit(r, 8) - 0.5) * 170.0;
        let center = cluster + offset;
        let radius = 3.0 + 7.0 * unit(r, 9);
        let clear_planets = sector
            .planets
            .iter()
            .all(|p| (center - p.center).length() > p.radius + radius + 30.0);
        let clear_tanks = sector
            .tanks
            .iter()
            .all(|t| (center - t.position).length() > radius + 20.0);
        if center.length() > INNER_EXCLUSION
            && clear_planets
            && clear_tanks
            && !near_log(center, radius)
        {
            sector.asteroids.push(Asteroid {
                id: r ^ 0xA5,
                center,
                radius,
                variant: (unit(r, 10) * ASTEROID_VARIANTS as f64) as usize % ASTEROID_VARIANTS,
                spin: 0.1 + 0.8 * unit(r, 11),
            });
        }
    }
}
