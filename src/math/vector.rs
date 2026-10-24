use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    pub const fn splat(v: f64) -> Self {
        Self::new(v, v, v)
    }
    pub fn from_array(a: [f64; 3]) -> Self {
        Self::new(a[0], a[1], a[2])
    }
    pub fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }
    /// Vector unitario; devuelve cero si la longitud es cero o no es finita.
    pub fn normalized(self) -> Self {
        let l = self.length();
        if l > 0.0 && l.is_finite() {
            self / l
        } else {
            Self::ZERO
        }
    }
    pub fn lerp(self, o: Self, t: f64) -> Self {
        self + (o - self) * t
    }
    pub fn mul_elem(self, o: Self) -> Self {
        Self::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn extend(self, w: f64) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, w)
    }
}

impl Vec4 {
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }
    pub fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }
    pub fn lerp(self, o: Self, t: f64) -> Self {
        self + (o - self) * t
    }
}

macro_rules! impl_ops {
    ($t:ident { $($f:ident),+ }) => {
        impl Add for $t {
            type Output = Self;
            fn add(self, o: Self) -> Self {
                Self { $($f: self.$f + o.$f),+ }
            }
        }
        impl Sub for $t {
            type Output = Self;
            fn sub(self, o: Self) -> Self {
                Self { $($f: self.$f - o.$f),+ }
            }
        }
        impl Mul<f64> for $t {
            type Output = Self;
            fn mul(self, s: f64) -> Self {
                Self { $($f: self.$f * s),+ }
            }
        }
        impl Div<f64> for $t {
            type Output = Self;
            fn div(self, s: f64) -> Self {
                Self { $($f: self.$f / s),+ }
            }
        }
        impl Neg for $t {
            type Output = Self;
            fn neg(self) -> Self {
                Self { $($f: -self.$f),+ }
            }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, o: Self) {
                $(self.$f += o.$f;)+
            }
        }
    };
}
impl_ops!(Vec2 { x, y });
impl_ops!(Vec3 { x, y, z });
impl_ops!(Vec4 { x, y, z, w });
