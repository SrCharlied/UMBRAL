use crate::math::{Mat4, Vec3};
use crate::renderer::clipping::Frustum;

/// Camara en perspectiva. Propietario: Persona 1; Persona 2 la usa por su API publica.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    projection: Mat4,
    pub fov_y: f64,
    pub aspect: f64,
    pub near: f64,
    pub far: f64,
}

impl Camera {
    pub fn new(fov_y: f64, aspect: f64, near: f64, far: f64) -> Result<Self, String> {
        Ok(Self {
            position: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            projection: Mat4::perspective(fov_y, aspect, near, far)?,
            fov_y,
            aspect,
            near,
            far,
        })
    }
    /// Cambia el lente; si los parametros son invalidos conserva el anterior.
    pub fn set_lens(&mut self, fov_y: f64, aspect: f64, near: f64, far: f64) -> Result<(), String> {
        self.projection = Mat4::perspective(fov_y, aspect, near, far)?;
        (self.fov_y, self.aspect, self.near, self.far) = (fov_y, aspect, near, far);
        Ok(())
    }
    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.position, self.target, self.up)
    }
    pub fn projection(&self) -> Mat4 {
        self.projection
    }
    pub fn view_projection(&self) -> Mat4 {
        self.projection * self.view()
    }
    pub fn frustum(&self) -> Frustum {
        Frustum::from_view_projection(&self.view_projection())
    }
    pub fn forward(&self) -> Vec3 {
        (self.target - self.position).normalized()
    }
}
