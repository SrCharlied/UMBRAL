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

pub type SectorKey = (i64, i64, i64);

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
}

/// Sectores vivos alrededor de la nave y tanques ya recogidos.
#[derive(Debug, Default)]
pub struct SectorField {
    sectors: HashMap<SectorKey, Sector>,
    collected: HashSet<u64>,
    center: Option<SectorKey>,
}

impl SectorField {
    pub fn new() -> Self {
        Self::default()
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
            let radius = planet.radius + margin;
            let closest = closest_point(start, *end, planet.center);
            if (closest - planet.center).length() >= radius {
                continue;
            }
            // Punto de entrada del segmento en la esfera (o el inicio si ya estaba dentro).
            let delta = *end - start;
            let rel = start - planet.center;
            let a = delta.dot(delta);
            let b = rel.dot(delta);
            let c = rel.dot(rel) - radius * radius;
            let entry = if c > 0.0 && a > 1e-12 {
                let disc = (b * b - a * c).max(0.0);
                let t = ((-b - disc.sqrt()) / a).clamp(0.0, 1.0);
                start + delta * t
            } else {
                *end
            };
            let normal = (entry - planet.center).normalized();
            let normal = if normal == Vec3::ZERO {
                Vec3::Y
            } else {
                normal
            };
            *end = planet.center + normal * (radius + 1e-6);
            hit = true;
        }
        hit
    }
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
        if center.length() > INNER_EXCLUSION + radius {
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
        if position.length() > INNER_EXCLUSION && clear {
            sector.tanks.push(OxygenTank {
                id: seed ^ 0xF4EE,
                position,
            });
        }
    }
    sector
}
