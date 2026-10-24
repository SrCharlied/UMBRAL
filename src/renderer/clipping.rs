//! Recorte contra el plano cercano y descarte por frustum.

use crate::math::{Mat4, Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
pub struct VertexOut {
    pub clip_position: Vec4,
    pub world_position: Vec3,
    pub world_normal: Vec3,
    pub uv: Vec2,
}

impl VertexOut {
    fn lerp(self, o: Self, t: f64) -> Self {
        Self {
            clip_position: self.clip_position.lerp(o.clip_position, t),
            world_position: self.world_position.lerp(o.world_position, t),
            world_normal: self.world_normal.lerp(o.world_normal, t),
            uv: self.uv + (o.uv - self.uv) * t,
        }
    }
}

/// Bits de los planos del frustum que dejan fuera al punto de clip (bit 4 = cercano).
pub fn outcode(c: Vec4) -> u8 {
    (u8::from(c.x < -c.w))
        | (u8::from(c.x > c.w) << 1)
        | (u8::from(c.y < -c.w) << 2)
        | (u8::from(c.y > c.w) << 3)
        | (u8::from(c.z < -c.w) << 4)
        | (u8::from(c.z > c.w) << 5)
}

pub const NEAR_BIT: u8 = 1 << 4;

/// Sutherland-Hodgman contra `z >= -w`. Devuelve de 0 a 4 vertices en orden.
pub fn clip_near(tri: [VertexOut; 3]) -> Vec<VertexOut> {
    let dist = |v: &VertexOut| v.clip_position.z + v.clip_position.w;
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, b) = (tri[i], tri[(i + 1) % 3]);
        let (da, db) = (dist(&a), dist(&b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a.lerp(b, da / (da - db)));
        }
    }
    out
}

/// Seis planos `(normal, d)` con normal hacia el interior: `n . p + d >= 0` es visible.
#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [(Vec3, f64); 6],
}

impl Frustum {
    pub fn from_view_projection(vp: &Mat4) -> Self {
        let row = |r: usize| Vec4::new(vp.m[r][0], vp.m[r][1], vp.m[r][2], vp.m[r][3]);
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let make = |p: Vec4| {
            let n = p.xyz();
            let len = n.length();
            (n / len, p.w / len)
        };
        Self {
            planes: [
                make(r3 + r0),
                make(r3 - r0),
                make(r3 + r1),
                make(r3 - r1),
                make(r3 + r2),
                make(r3 - r2),
            ],
        }
    }
    /// Falso solo si la esfera esta completamente fuera de algun plano.
    pub fn sphere_visible(&self, center: Vec3, radius: f64) -> bool {
        self.planes
            .iter()
            .all(|(n, d)| n.dot(center) + d >= -radius)
    }
}
