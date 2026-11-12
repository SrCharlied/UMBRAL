//! Contenido procedural de los planetas jugables: une la distribucion sobre esfera (UMB-304,
//! `procedural_surface::distribute`) con el anclaje de props (UMB-305, `SurfaceAnchor`).
//!
//! Al construir el mundo, cada planeta recibe sus muestras de `distribute` y un `SurfaceAnchor`
//! **persistente** por prop (normal local, giro, hundimiento y escala). En cada cuadro solo cambia
//! la `Transform` del planeta: `SurfaceAnchor::world_transform` da la pose exacta para
//! `draw_model` (ver `docs/procedural.md`), asi los props siguen orbita y rotacion axial sin
//! recalcular la base.
//!
//! **Sombra del planeta:** el shader `LitPlanet` no sabe que el planeta tapa al Sol, asi que un
//! prop de la cara nocturna se veria iluminado. `prop_light_position` inclina la luz de cada prop
//! hacia su suelo con el mismo terminador que el planeta, sin tocar el renderer.
//!
//! Props (pivote en la base, +Y arriba, alto ~1 en unidades del modelo):
//! - Antena: mastiles con antena parabolica y baliza roja.
//! - Osario: costillas curvas que salen del suelo, en grupos de tres.
//! - Vigilia: racimos de cristales hexagonales.
//! - Los tres: rocas de relleno con el color del suelo.
//!
//! Son decorativos: no colisionan (la navegacion sigue usando la esfera del cuerpo) y su altura
//! queda muy por debajo de las balizas de recursos y registros.

use std::{collections::BTreeMap, f64::consts::TAU};

use crate::{
    assets::obj::{Face, Model},
    math::{Mat4, Vec3},
    scene::solar_system::SolarSystem,
    visual::{
        procedural_surface::{DistributionSettings, distribute},
        surface_placement::{PropPlacement, SurfaceAnchor},
    },
};

/// Tipo de prop: cada uno es una malla base en espacio de modelo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Mast,
    Ribs,
    Crystals,
    Rock,
}

/// Una capa de props sobre un planeta: distribucion + rango de escala.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropLayer {
    pub kind: PropKind,
    pub distribution: DistributionSettings,
    /// Radios del planeta por unidad de modelo, `[min, max]`.
    pub scale: (f64, f64),
    /// Hundimiento del pivote (radios del planeta) para que la base no flote.
    pub elevation: f64,
    /// Tope de props de esta capa (las primeras muestras en orden de intento).
    pub max_count: usize,
    /// Color del material del suelo para las rocas (`Rock`); ignorado en otros tipos.
    pub rock_color: [f64; 3],
}

/// Contenido decidido por planeta, por nombre del cuerpo. El Sol y los cuerpos sin entrada no
/// llevan props.
pub fn layers_for(body_name: &str) -> Vec<PropLayer> {
    let dist = |seed: u64, attempts: u32, noise_scale: f64, threshold: f64, min_spacing: f64| {
        DistributionSettings {
            seed,
            attempts,
            noise_scale,
            threshold,
            min_spacing,
        }
    };
    let layer = |kind, distribution, scale, elevation, max_count, rock_color| PropLayer {
        kind,
        distribution,
        scale,
        elevation,
        max_count,
        rock_color,
    };
    match body_name {
        "Antena" => vec![
            layer(
                PropKind::Mast,
                dist(0x0A17_E0A1, 1200, 2.2, 0.52, 0.24),
                (0.15, 0.21),
                -0.004,
                22,
                [0.0; 3],
            ),
            layer(
                PropKind::Rock,
                dist(0x0A17_E0A2, 900, 3.0, 0.45, 0.14),
                (0.035, 0.07),
                -0.01,
                40,
                [0.42, 0.38, 0.34],
            ),
        ],
        "Osario" => vec![
            layer(
                PropKind::Ribs,
                dist(0x05A1_0B01, 1200, 2.6, 0.5, 0.26),
                (0.2, 0.28),
                -0.01,
                20,
                [0.0; 3],
            ),
            layer(
                PropKind::Rock,
                dist(0x05A1_0B02, 900, 3.4, 0.5, 0.16),
                (0.04, 0.08),
                -0.012,
                28,
                [0.58, 0.52, 0.41],
            ),
        ],
        "Vigilia" => vec![
            layer(
                PropKind::Crystals,
                dist(0x0F16_11A1, 1400, 2.0, 0.5, 0.19),
                (0.11, 0.18),
                -0.006,
                30,
                [0.0; 3],
            ),
            layer(
                PropKind::Rock,
                dist(0x0F16_11A2, 900, 3.2, 0.5, 0.15),
                (0.03, 0.06),
                -0.01,
                30,
                [0.27, 0.32, 0.45],
            ),
        ],
        _ => Vec::new(),
    }
}

/// Un prop colocado: malla (`PlanetProps::meshes[mesh]`), anclaje persistente y su normal local.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedProp {
    pub mesh: usize,
    pub anchor: SurfaceAnchor,
    /// Normal radial unitaria de la muestra, espacio local del planeta.
    pub normal: Vec3,
}

/// Contenido de un planeta: una malla base por capa y los props que la usan.
#[derive(Debug)]
pub struct PlanetProps {
    /// Nombre del cuerpo dueno; el dibujo busca por nombre, no por indice, para que un sistema
    /// con cuerpos quitados o reordenados no ponga props en otro planeta.
    pub body: &'static str,
    pub meshes: Vec<Model>,
    pub props: Vec<PlacedProp>,
    /// Altura maxima de un prop sobre la superficie, en radios del planeta (esfera envolvente).
    pub max_reach: f64,
}

/// Props por cuerpo del sistema (`None`: cuerpo sin contenido, como el Sol).
pub fn build_system_props(system: &SolarSystem) -> Result<Vec<Option<PlanetProps>>, String> {
    system
        .bodies
        .iter()
        .map(|body| {
            let layers = layers_for(body.name);
            if layers.is_empty() {
                return Ok(None);
            }
            build_planet_props(&layers).map(|mut content| {
                content.body = body.name;
                Some(content)
            })
        })
        .collect()
}

/// Distribuye cada capa y crea un anclaje por muestra, una sola vez.
pub fn build_planet_props(layers: &[PropLayer]) -> Result<PlanetProps, String> {
    let mut meshes = Vec::with_capacity(layers.len());
    let mut props = Vec::new();
    let mut max_reach: f64 = 0.0;
    for (layer_index, layer) in layers.iter().enumerate() {
        let samples = distribute(&layer.distribution)?;
        let mesh = prop_mesh(layer.kind, layer.rock_color, layer_index);
        let tallest = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from_array(*v).length())
            .fold(0.0, f64::max);
        for sample in samples.iter().take(layer.max_count) {
            let seed = layer.distribution.seed ^ u64::from(sample.attempt).wrapping_mul(0x9E37);
            let twist = unit(seed, 1) * TAU;
            let (lo, hi) = layer.scale;
            let scale = lo + (hi - lo) * unit(seed, 2);
            let anchor = SurfaceAnchor::new(
                sample.normal,
                PropPlacement {
                    twist,
                    elevation: layer.elevation,
                    scale,
                },
            )?;
            max_reach = max_reach.max(layer.elevation + tallest * scale);
            props.push(PlacedProp {
                mesh: layer_index,
                anchor,
                normal: sample.normal,
            });
        }
        meshes.push(mesh);
    }
    if props.is_empty() {
        return Err("ningun prop aceptado por la distribucion".into());
    }
    Ok(PlanetProps {
        body: "",
        meshes,
        props,
        max_reach,
    })
}

/// Terminador de los props: el mismo intervalo que el difuso de `LitPlanet` en el suelo.
pub const PROP_TERMINATOR: (f64, f64) = (-0.08, 0.3);

/// Posicion de luz para un prop: la real de dia; de noche se inclina hacia el interior del
/// planeta (`-normal`), asi las caras que miran al Sol se apagan como el suelo que las rodea.
/// `normal_world`: normal radial del prop en el mundo; `base`: su pie en el mundo.
pub fn prop_light_position(
    light: Vec3,
    planet_center: Vec3,
    normal_world: Vec3,
    base: Vec3,
) -> Vec3 {
    let to_light = (light - planet_center).normalized();
    let (e0, e1) = PROP_TERMINATOR;
    let x = ((normal_world.dot(to_light) - e0) / (e1 - e0)).clamp(0.0, 1.0);
    let lit = x * x * (3.0 - 2.0 * x);
    if lit >= 1.0 {
        return light;
    }
    let tilted = to_light * lit - normal_world * (1.0 - lit);
    let direction = if tilted.length() > 1e-9 {
        tilted.normalized()
    } else {
        -normal_world
    };
    base + direction * (light - base).length().max(1.0)
}

/// Valor en `[0, 1)` determinista por `(seed, stream)` (finalizador de SplitMix64).
fn unit(seed: u64, stream: u64) -> f64 {
    let mut z = seed
        .wrapping_add(stream.wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Acumula piezas convexas; orienta cada cara hacia fuera de su pieza (CCW visto desde fuera),
/// asi el descarte de caras traseras funciona con cualquier transformacion sin reflexion.
struct MeshBuilder {
    model: Model,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            model: Model {
                vertices: Vec::new(),
                uvs: Vec::new(),
                normals: Vec::new(),
                faces: Vec::new(),
                material_libraries: Vec::new(),
                materials: BTreeMap::new(),
                bounds_center: [0.0; 3],
                bounds_radius: 1.0,
            },
        }
    }

    fn material(&mut self, name: &str, color: [f64; 3]) -> String {
        self.model.materials.insert(name.to_string(), color);
        name.to_string()
    }

    /// Pieza convexa: vertices y triangulos en cualquier orden de giro.
    fn convex(&mut self, vertices: &[Vec3], faces: &[[usize; 3]], material: &str) {
        let offset = self.model.vertices.len();
        let centroid = vertices.iter().fold(Vec3::ZERO, |acc, &v| acc + v) / vertices.len() as f64;
        self.model
            .vertices
            .extend(vertices.iter().map(|v| [v.x, v.y, v.z]));
        for &[a, b, c] in faces {
            let (pa, pb, pc) = (vertices[a], vertices[b], vertices[c]);
            let outward = (pb - pa).cross(pc - pa).dot(pa - centroid) > 0.0;
            let [a, b, c] = if outward { [a, b, c] } else { [a, c, b] };
            self.model.faces.push(Face {
                vertex_indices: [offset + a, offset + b, offset + c],
                uv_indices: None,
                normal_indices: None,
                material: Some(material.to_string()),
            });
        }
    }

    fn finish(self) -> Model {
        self.model
    }
}

/// Tronco de piramide/cono de `n` lados entre dos anillos (`r1 = 0`: punta).
fn frustum(n: usize, r0: f64, r1: f64, y0: f64, y1: f64) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let ring = |r: f64, y: f64| {
        (0..n)
            .map(|i| {
                let a = TAU * i as f64 / n as f64;
                Vec3::new(r * a.cos(), y, r * a.sin())
            })
            .collect::<Vec<_>>()
    };
    let mut vertices = ring(r0, y0);
    let mut faces = Vec::new();
    // Tapa inferior en abanico.
    for i in 1..n - 1 {
        faces.push([0, i, i + 1]);
    }
    if r1 <= 0.0 {
        vertices.push(Vec3::new(0.0, y1, 0.0));
        for i in 0..n {
            faces.push([i, (i + 1) % n, n]);
        }
    } else {
        vertices.extend(ring(r1, y1));
        for i in 0..n {
            let j = (i + 1) % n;
            faces.push([i, j, n + j]);
            faces.push([i, n + j, n + i]);
        }
        for i in 1..n - 1 {
            faces.push([n, n + i, n + i + 1]);
        }
    }
    (vertices, faces)
}

/// Segmento de tubo entre `a` y `b` (radios `ra`, `rb`), `n` lados.
fn tube(a: Vec3, b: Vec3, ra: f64, rb: f64, n: usize) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let axis = (b - a).normalized();
    let helper = if axis.dot(Vec3::Z).abs() < 0.9 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let u = axis.cross(helper).normalized();
    let v = axis.cross(u);
    let mut vertices = Vec::with_capacity(2 * n);
    for (center, r) in [(a, ra), (b, rb)] {
        for i in 0..n {
            let t = TAU * i as f64 / n as f64;
            vertices.push(center + (u * t.cos() + v * t.sin()) * r);
        }
    }
    let mut faces = Vec::new();
    for i in 1..n - 1 {
        faces.push([0, i, i + 1]);
        faces.push([n, n + i, n + i + 1]);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push([i, j, n + j]);
        faces.push([i, n + j, n + i]);
    }
    (vertices, faces)
}

fn transformed(piece: (Vec<Vec3>, Vec<[usize; 3]>), m: &Mat4) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    (
        piece.0.into_iter().map(|v| m.transform_point(v)).collect(),
        piece.1,
    )
}

/// Malla base de cada tipo. `layer` distingue los nombres de material entre capas.
pub fn prop_mesh(kind: PropKind, rock_color: [f64; 3], layer: usize) -> Model {
    let mut b = MeshBuilder::new();
    match kind {
        PropKind::Mast => {
            let metal = b.material("mastil", [0.58, 0.6, 0.64]);
            let dish = b.material("antena", [0.82, 0.82, 0.78]);
            let beacon = b.material("baliza", [0.95, 0.16, 0.08]);
            let (v, f) = frustum(4, 0.09, 0.035, 0.0, 1.0);
            b.convex(&v, &f, &metal);
            // Travesano.
            let (v, f) = tube(
                Vec3::new(-0.28, 0.68, 0.0),
                Vec3::new(0.28, 0.68, 0.0),
                0.025,
                0.025,
                4,
            );
            b.convex(&v, &f, &metal);
            // Parabola inclinada hacia el cielo.
            let m = Mat4::translation(Vec3::new(0.0, 0.82, 0.06)) * Mat4::rotation_x(0.6);
            let (v, f) = transformed(frustum(8, 0.03, 0.26, 0.0, 0.09), &m);
            b.convex(&v, &f, &dish);
            let (v, f) = frustum(4, 0.045, 0.0, 1.0, 1.13);
            b.convex(&v, &f, &beacon);
        }
        PropKind::Ribs => {
            let bone = b.material("hueso", [0.88, 0.82, 0.68]);
            for (z, size) in [(-0.3, 0.8), (0.0, 1.0), (0.3, 0.85)] {
                let segments = 6;
                let radius = 0.85 * size;
                let point = |s: f64| {
                    let theta = s * 1.25;
                    Vec3::new(radius * (1.0 - theta.cos()) - 0.3, radius * theta.sin(), z)
                };
                for k in 0..segments {
                    let (s0, s1) = (k as f64 / segments as f64, (k + 1) as f64 / segments as f64);
                    let r = |s: f64| (0.075 * (1.0 - s) + 0.012) * size;
                    let (v, f) = tube(point(s0), point(s1), r(s0), r(s1), 4);
                    b.convex(&v, &f, &bone);
                }
            }
        }
        PropKind::Crystals => {
            let crystal = b.material("cristal", [0.5, 0.78, 0.98]);
            let pale = b.material("cristal_palido", [0.72, 0.86, 0.98]);
            let spire = |r: f64, h: f64| {
                let (mut v, mut f) = frustum(6, r, r * 0.85, 0.0, h);
                // Punta: un vertice sobre la tapa superior (sigue siendo convexo).
                let apex = v.len();
                v.push(Vec3::new(0.0, h + r * 1.8, 0.0));
                // Sustituye la tapa superior por la piramide.
                f.retain(|t| !(t[0] >= 6 && t[1] >= 6 && t[2] >= 6));
                for i in 0..6 {
                    f.push([6 + i, 6 + (i + 1) % 6, apex]);
                }
                (v, f)
            };
            let (v, f) = spire(0.12, 0.62);
            b.convex(&v, &f, &crystal);
            for (angle, yaw, r, h, mat) in [
                (0.45, 0.4, 0.08, 0.4, &pale),
                (-0.5, 2.3, 0.07, 0.34, &crystal),
                (0.35, 4.2, 0.06, 0.26, &pale),
            ] {
                let m = Mat4::rotation_y(yaw)
                    * Mat4::translation(Vec3::new(0.1, -0.02, 0.0))
                    * Mat4::rotation_z(angle);
                let (v, f) = transformed(spire(r, h), &m);
                b.convex(&v, &f, mat);
            }
        }
        PropKind::Rock => {
            let name = format!("roca{layer}");
            let rock = b.material(&name, rock_color);
            let (mut v, f) = frustum(6, 0.5, 0.3, -0.15, 0.42);
            // Achatada y sesgada para que no parezca un prisma perfecto; una transformacion
            // afin conserva la convexidad que necesita la orientacion de caras.
            for p in &mut v {
                p.x = p.x * 1.2 + p.y * 0.25;
                p.z *= 0.8;
            }
            b.convex(&v, &f, &rock);
        }
    }
    let mut model = b.finish();
    model.update_bounds();
    model
}
