//! Shader texturizado del preview de la criatura (CPU). La escena del juego no lo usa.
//! Ver `docs/textured-preview.md`.

use crate::{
    math::Vec3,
    renderer::{
        shader::{Color, FragmentInput},
        texture::Texture,
    },
};

/// Color de diagnostico para fragmentos con UV NaN.
pub const INVALID_UV_COLOR: Color = Vec3::new(1.0, 0.0, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexturedMode {
    Unlit,
    Lit,
}

pub struct TexturedShader<'a> {
    texture: &'a Texture,
    mode: TexturedMode,
    light_position: Vec3,
    ambient: f64,
}

impl<'a> TexturedShader<'a> {
    /// `light_position` en mundo (finita); `ambient` en `0..=1` (fraccion de luz en sombra).
    /// La textura se captura aqui una vez; `shade` no carga ni reserva nada.
    pub fn new(
        texture: &'a Texture,
        mode: TexturedMode,
        light_position: Vec3,
        ambient: f64,
    ) -> Result<Self, String> {
        if !light_position.is_finite() {
            return Err(format!(
                "light_position debe ser finita: {light_position:?}"
            ));
        }
        if !(0.0..=1.0).contains(&ambient) {
            return Err(format!("ambient debe estar en [0, 1]: {ambient}"));
        }
        Ok(Self {
            texture,
            mode,
            light_position,
            ambient,
        })
    }
    /// `Unlit`: albedo de la textura. `Lit`: albedo * (ambiente + (1 - ambiente) * difuso),
    /// con la normal interpolada renormalizada. El `Kd`/`base_color` del material se ignora
    /// (tinte blanco). UV NaN da `INVALID_UV_COLOR`.
    pub fn shade(&self, fragment: &FragmentInput) -> Color {
        let Some(albedo) = self.texture.sample(fragment.uv) else {
            return INVALID_UV_COLOR;
        };
        match self.mode {
            TexturedMode::Unlit => albedo,
            TexturedMode::Lit => {
                let n = fragment.normal.normalized();
                let l = (self.light_position - fragment.world_position).normalized();
                let diffuse = n.dot(l).max(0.0);
                albedo * (self.ambient + (1.0 - self.ambient) * diffuse)
            }
        }
    }
}
