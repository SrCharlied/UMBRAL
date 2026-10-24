use super::vector::{Vec3, Vec4};

/// Matriz 4x4 de vectores columna, `m[fila][columna]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [[f64; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translation(t: Vec3) -> Self {
        let mut r = Self::IDENTITY;
        r.m[0][3] = t.x;
        r.m[1][3] = t.y;
        r.m[2][3] = t.z;
        r
    }
    pub fn scaling(s: Vec3) -> Self {
        let mut r = Self::IDENTITY;
        r.m[0][0] = s.x;
        r.m[1][1] = s.y;
        r.m[2][2] = s.z;
        r
    }
    pub fn rotation_x(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        let mut r = Self::IDENTITY;
        r.m[1][1] = c;
        r.m[1][2] = -s;
        r.m[2][1] = s;
        r.m[2][2] = c;
        r
    }
    pub fn rotation_y(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        let mut r = Self::IDENTITY;
        r.m[0][0] = c;
        r.m[0][2] = s;
        r.m[2][0] = -s;
        r.m[2][2] = c;
        r
    }
    pub fn rotation_z(a: f64) -> Self {
        let (s, c) = a.sin_cos();
        let mut r = Self::IDENTITY;
        r.m[0][0] = c;
        r.m[0][1] = -s;
        r.m[1][0] = s;
        r.m[1][1] = c;
        r
    }
    /// Rotacion Euler `R = Ry * Rx * Rz` (guinada, cabeceo, alabeo); `e = (x, y, z)`.
    pub fn from_euler(e: Vec3) -> Self {
        Self::rotation_y(e.y) * Self::rotation_x(e.x) * Self::rotation_z(e.z)
    }
    /// Inversa de `from_euler`. En el bloqueo de cardan (cabeceo = +-90 grados) el
    /// alabeo se fija en 0 y la guinada absorbe el resto.
    pub fn to_euler(&self) -> Vec3 {
        let m = &self.m;
        if m[1][2].abs() < 0.999_999 {
            Vec3::new(
                (-m[1][2]).asin(),
                m[0][2].atan2(m[2][2]),
                m[1][0].atan2(m[1][1]),
            )
        } else if m[1][2] < 0.0 {
            Vec3::new(std::f64::consts::FRAC_PI_2, m[0][1].atan2(m[0][0]), 0.0)
        } else {
            Vec3::new(-std::f64::consts::FRAC_PI_2, (-m[0][1]).atan2(m[0][0]), 0.0)
        }
    }
    /// Rotacion alrededor de un eje (formula de Rodrigues).
    pub fn rotation_axis(axis: Vec3, a: f64) -> Self {
        let k = axis.normalized();
        let (s, c) = a.sin_cos();
        let t = 1.0 - c;
        let mut r = Self::IDENTITY;
        r.m[0] = [
            t * k.x * k.x + c,
            t * k.x * k.y - s * k.z,
            t * k.x * k.z + s * k.y,
            0.0,
        ];
        r.m[1] = [
            t * k.x * k.y + s * k.z,
            t * k.y * k.y + c,
            t * k.y * k.z - s * k.x,
            0.0,
        ];
        r.m[2] = [
            t * k.x * k.z - s * k.y,
            t * k.y * k.z + s * k.x,
            t * k.z * k.z + c,
            0.0,
        ];
        r
    }
    /// Matriz de vista: camara en `eye` mirando a `target`, mano derecha, mira hacia -Z.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalized();
        let mut s = f.cross(up).normalized();
        if s == Vec3::ZERO {
            // `up` paralelo a la vista: elegir cualquier eje perpendicular.
            let axis = if f.x.abs() < 0.9 { Vec3::X } else { Vec3::Z };
            s = f.cross(axis).normalized();
        }
        let u = s.cross(f);
        let mut r = Self::IDENTITY;
        r.m[0] = [s.x, s.y, s.z, -s.dot(eye)];
        r.m[1] = [u.x, u.y, u.z, -u.dot(eye)];
        r.m[2] = [-f.x, -f.y, -f.z, f.dot(eye)];
        r
    }
    /// Perspectiva con NDC `[-1, 1]`; `fov_y` en radianes. `Err` si el frustum es invalido.
    pub fn perspective(fov_y: f64, aspect: f64, near: f64, far: f64) -> Result<Self, String> {
        if !(fov_y.is_finite() && fov_y > 0.0 && fov_y < std::f64::consts::PI) {
            return Err("fov_y debe estar en (0, pi)".into());
        }
        if !(aspect.is_finite() && aspect > 0.0) {
            return Err("aspect debe ser positivo".into());
        }
        if !(near.is_finite() && near > 0.0 && far.is_finite() && far > near) {
            return Err("se requiere 0 < near < far".into());
        }
        let f = 1.0 / (fov_y / 2.0).tan();
        let mut r = Self { m: [[0.0; 4]; 4] };
        r.m[0][0] = f / aspect;
        r.m[1][1] = f;
        r.m[2][2] = -(far + near) / (far - near);
        r.m[2][3] = -2.0 * far * near / (far - near);
        r.m[3][2] = -1.0;
        Ok(r)
    }
    pub fn transform(&self, v: Vec4) -> Vec4 {
        let m = &self.m;
        Vec4::new(
            m[0][0] * v.x + m[0][1] * v.y + m[0][2] * v.z + m[0][3] * v.w,
            m[1][0] * v.x + m[1][1] * v.y + m[1][2] * v.z + m[1][3] * v.w,
            m[2][0] * v.x + m[2][1] * v.y + m[2][2] * v.z + m[2][3] * v.w,
            m[3][0] * v.x + m[3][1] * v.y + m[3][2] * v.z + m[3][3] * v.w,
        )
    }
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        self.transform(p.extend(1.0)).xyz()
    }
    /// Aplica solo la parte lineal (sin traslacion).
    pub fn transform_dir(&self, d: Vec3) -> Vec3 {
        self.transform(d.extend(0.0)).xyz()
    }
    pub fn column(&self, c: usize) -> Vec3 {
        Vec3::new(self.m[0][c], self.m[1][c], self.m[2][c])
    }
    /// Reortonormaliza la parte rotacional (Gram-Schmidt) para frenar la deriva numerica.
    pub fn orthonormalized(&self) -> Self {
        let x = self.column(0).normalized();
        let z = x.cross(self.column(1)).normalized();
        let y = z.cross(x);
        let mut r = Self::IDENTITY;
        for (c, v) in [x, y, z].into_iter().enumerate() {
            r.m[0][c] = v.x;
            r.m[1][c] = v.y;
            r.m[2][c] = v.z;
        }
        r
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let mut r = Self { m: [[0.0; 4]; 4] };
        for i in 0..4 {
            for j in 0..4 {
                r.m[i][j] = (0..4).map(|k| self.m[i][k] * o.m[k][j]).sum();
            }
        }
        r
    }
}

/// Transformacion de un objeto: `M = T * R(euler) * S`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: Vec3,
    /// Euler en radianes `(cabeceo x, guinada y, alabeo z)`, ver `Mat4::from_euler`.
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Vec3::ZERO,
        scale: Vec3::ONE,
    };
    pub fn at(position: Vec3) -> Self {
        Self {
            position,
            ..Self::IDENTITY
        }
    }
    pub fn matrix(&self) -> Mat4 {
        Mat4::translation(self.position)
            * Mat4::from_euler(self.rotation)
            * Mat4::scaling(self.scale)
    }
    /// Normal de objeto a mundo: rotacion * (n / escala), equivalente a la inversa
    /// transpuesta cuando la escala es por eje.
    pub fn transform_normal(&self, n: Vec3) -> Vec3 {
        let inv = Vec3::new(1.0 / self.scale.x, 1.0 / self.scale.y, 1.0 / self.scale.z);
        Mat4::from_euler(self.rotation)
            .transform_dir(n.mul_elem(inv))
            .normalized()
    }
}
