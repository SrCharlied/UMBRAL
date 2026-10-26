//! Orientacion y anclaje de props sobre la esfera de un planeta (UMB-305).
//! Contrato, unidades y limites en `docs/procedural.md`.

use crate::math::{Mat4, Transform, Vec3};

/// Limites de `PropPlacement::elevation`, en radios del planeta.
pub const MIN_ELEVATION: f64 = -0.25;
pub const MAX_ELEVATION: f64 = 1.0;
/// Limite de `PropPlacement::scale`, en radios del planeta por unidad del modelo.
pub const MAX_PROP_SCALE: f64 = 1.0;
/// Rango de `radio * scale` (escala del prop en el mundo). `Transform::transform_normal`
/// normaliza `n / s`, y `(1 / s)^2` tiene que ser un f64 normal: `[1e-300, 1e300]`.
pub const MIN_WORLD_SCALE: f64 = 1e-150;
pub const MAX_WORLD_SCALE: f64 = 1e150;

/// Como se coloca un prop modelado con el pivote en su base y +Y hacia arriba.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropPlacement {
    /// Giro en radianes alrededor de la normal (Y local), finito.
    pub twist: f64,
    /// Desplazamiento del pivote a lo largo de la normal, en radios del planeta;
    /// negativo lo hunde. `[MIN_ELEVATION, MAX_ELEVATION]`.
    pub elevation: f64,
    /// Radios del planeta por unidad del modelo, `(0, MAX_PROP_SCALE]`.
    pub scale: f64,
}

/// Anclaje local persistente: pivote, base y escala en el espacio del planeta (radio 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceAnchor {
    origin: Vec3,
    rotation: Mat4,
    scale: f64,
}

/// Debajo de esta longitud de `Y x n` la normal se considera polar y la base usa +Z.
const POLE_EPSILON: f64 = 1e-6;

/// Base tangente determinista con `Y' = n`, `Z' = normalizar(Y x n)` (horizontal) y
/// `X' = n x Z'` (hacia el norte). En los polos usa `Z x n`: una base continua en toda la
/// esfera no existe, asi que hay un salto en ese casquete diminuto.
fn tangent_frame(n: Vec3) -> Mat4 {
    let east = Vec3::Y.cross(n);
    let z = if east.length() >= POLE_EPSILON {
        east.normalized()
    } else {
        Vec3::Z.cross(n).normalized()
    };
    let x = n.cross(z);
    let mut r = Mat4::IDENTITY;
    for (c, v) in [x, n, z].into_iter().enumerate() {
        r.m[0][c] = v.x;
        r.m[1][c] = v.y;
        r.m[2][c] = v.z;
    }
    r
}

/// Diferencia maxima admitida entre `|normal|` y 1.
const NORMAL_TOLERANCE: f64 = 1e-9;

impl PropPlacement {
    fn validate(&self) -> Result<(), String> {
        if !self.twist.is_finite() {
            return Err(format!("twist debe ser finito: {}", self.twist));
        }
        if !(self.elevation.is_finite()
            && (MIN_ELEVATION..=MAX_ELEVATION).contains(&self.elevation))
        {
            return Err(format!(
                "elevation debe ser finito y estar en [{MIN_ELEVATION}, {MAX_ELEVATION}]: {}",
                self.elevation
            ));
        }
        // `is_normal` excluye cero, subnormales, infinitos y NaN: `1 / scale` es finito.
        if !(self.scale.is_normal() && self.scale > 0.0 && self.scale <= MAX_PROP_SCALE) {
            return Err(format!(
                "scale debe ser un f64 normal en [f64::MIN_POSITIVE, {MAX_PROP_SCALE}]: {:e}",
                self.scale
            ));
        }
        Ok(())
    }
}

/// Radio del planeta: escala uniforme, positiva y finita (sin reflexion ni elipsoides).
fn planet_radius(planet: &Transform) -> Result<f64, String> {
    if !planet.position.is_finite() {
        return Err(format!(
            "planet.position debe ser finita: {:?}",
            planet.position
        ));
    }
    if !planet.rotation.is_finite() {
        return Err(format!(
            "planet.rotation debe ser finita: {:?}",
            planet.rotation
        ));
    }
    let s = planet.scale;
    if ![s.x, s.y, s.z].iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err(format!("planet.scale debe ser finita y positiva: {s:?}"));
    }
    if !(s.x == s.y && s.y == s.z) {
        return Err(format!("planet.scale debe ser uniforme: {s:?}"));
    }
    Ok(s.x)
}

/// Angulos `(x, y, z)` para `Mat4::from_euler` (`Ry(y) * Rx(x) * Rz(z)`) que reproducen la
/// rotacion `r` con error de redondeo en toda la esfera, incluido el bloqueo de cardan
/// (metodo de M. Day: se retira la guinada y el cabeceo y el alabeo se leen del resto,
/// que esta bien condicionado). Adaptador local para `world_transform`: `Mat4::to_euler`
/// fija el cabeceo en +-90 grados en `|m12| >= 0.999999` y no se modifica aqui.
fn euler_yxz(r: &Mat4) -> Vec3 {
    let m = &r.m;
    let yaw = m[0][2].atan2(m[2][2]);
    let (s, c) = yaw.sin_cos();
    // Filas 0 y 2 de `Ry(-yaw) * r`; la fila 1 no cambia. El resto es `Rx(x) * Rz(z)`.
    let r00 = c * m[0][0] - s * m[2][0];
    let r01 = c * m[0][1] - s * m[2][1];
    let r22 = s * m[0][2] + c * m[2][2];
    Vec3::new((-m[1][2]).atan2(r22), yaw, (-r01).atan2(r00))
}

fn check_finite(m: &Mat4) -> Result<(), String> {
    if (0..3).all(|i| m.m[i].iter().all(|v| v.is_finite())) {
        Ok(())
    } else {
        Err("la pose resultante no es finita (desbordamiento)".into())
    }
}

impl SurfaceAnchor {
    /// `normal`: direccion unitaria en el espacio local del planeta (tolerancia 1e-9), por
    /// ejemplo `SurfaceSample::normal`. El anclaje es local y persistente: se crea una vez
    /// y solo cambia la `Transform` del planeta.
    pub fn new(normal: Vec3, placement: PropPlacement) -> Result<Self, String> {
        if !normal.is_finite() || (normal.length() - 1.0).abs() > NORMAL_TOLERANCE {
            return Err(format!(
                "normal debe ser un vector unitario finito (tolerancia {NORMAL_TOLERANCE}): {normal:?}"
            ));
        }
        placement.validate()?;
        let n = normal.normalized();
        Ok(Self {
            origin: n * (1.0 + placement.elevation),
            rotation: tangent_frame(n) * Mat4::rotation_y(placement.twist),
            scale: placement.scale,
        })
    }
    /// `Tanchor * Ralign * Rtwist * Sprop` en el espacio local del planeta (radio 1).
    pub fn local_matrix(&self) -> Mat4 {
        Mat4::translation(self.origin) * self.rotation * Mat4::scaling(Vec3::splat(self.scale))
    }
    /// Pose exacta en el mundo: `planet.matrix() * local_matrix()`.
    /// Escala del prop en el mundo (`radio * scale`). Un producto que subdesborda es
    /// finito pero degenerado; fuera de `[MIN_WORLD_SCALE, MAX_WORLD_SCALE]` el pipeline
    /// no puede transformar normales.
    fn world_scale(&self, planet: &Transform) -> Result<f64, String> {
        let scale = planet_radius(planet)? * self.scale;
        if (MIN_WORLD_SCALE..=MAX_WORLD_SCALE).contains(&scale) {
            Ok(scale)
        } else {
            Err(format!(
                "la escala en el mundo (radio * scale) debe estar en [{MIN_WORLD_SCALE:e}, {MAX_WORLD_SCALE:e}]: {scale:e}"
            ))
        }
    }
    pub fn world_matrix(&self, planet: &Transform) -> Result<Mat4, String> {
        self.world_scale(planet)?;
        let m = planet.matrix() * self.local_matrix();
        check_finite(&m)?;
        Ok(m)
    }
    /// La misma pose como `Transform` para `draw_model`. Con escala de planeta uniforme
    /// `T * R * S(r) * Tanchor * Ralign * Rtwist * S(s)` se reduce a `T' * R' * S(r * s)`;
    /// los angulos salen de `euler_yxz`, exacta tambien cerca del bloqueo de cardan.
    pub fn world_transform(&self, planet: &Transform) -> Result<Transform, String> {
        let scale = self.world_scale(planet)?;
        let t = Transform {
            position: planet.matrix().transform_point(self.origin),
            rotation: euler_yxz(&(Mat4::from_euler(planet.rotation) * self.rotation)),
            scale: Vec3::splat(scale),
        };
        check_finite(&t.matrix())?;
        Ok(t)
    }
}
