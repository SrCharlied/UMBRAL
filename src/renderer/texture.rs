//! Textura RGB8 compacta y muestreo bilineal para el preview texturizado.
//! Contrato y limites en `docs/textured-preview.md`.

use crate::math::{Vec2, Vec3};
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek},
    path::Path,
    sync::OnceLock,
};

/// Lado maximo admitido (ancho y alto).
pub const MAX_TEXTURE_SIDE: u32 = 4096;
/// Bytes que el decodificador `png` puede reservar para sus buffers internos (filas,
/// fragmentos auxiliares). La imagen (hasta 48 MiB) la reserva `decode_png` aparte.
pub const PNG_DECODER_LIMIT: usize = 64 * 1024 * 1024;

/// Texeles RGB de 8 bits en orden de filas PNG (la fila 0 es la de arriba), 3 bytes por
/// texel. Se guarda compacta: 4096x4096 ocupa 48 MiB, no un `Vec3<f64>` por texel.
pub struct Texture {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

fn lut() -> &'static [f64; 256] {
    static LUT: OnceLock<[f64; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        std::array::from_fn(|i| {
            let c = i as f64 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

/// sRGB de 8 bits a lineal `0..=1` (IEC 61966-2-1), por tabla de 256 entradas.
pub fn srgb_to_linear(value: u8) -> f64 {
    lut()[value as usize]
}

impl Texture {
    /// `rgb` debe tener exactamente `width * height * 3` bytes; cada lado en
    /// `1..=MAX_TEXTURE_SIDE`.
    pub fn from_rgb8(width: u32, height: u32, rgb: Vec<u8>) -> Result<Self, String> {
        if !(1..=MAX_TEXTURE_SIDE).contains(&width) || !(1..=MAX_TEXTURE_SIDE).contains(&height) {
            return Err(format!(
                "dimension de textura fuera de 1..={MAX_TEXTURE_SIDE}: {width}x{height}"
            ));
        }
        let expected = width as usize * height as usize * 3;
        if rgb.len() != expected {
            return Err(format!(
                "se esperaban {expected} bytes RGB8 para {width}x{height}, hay {}",
                rgb.len()
            ));
        }
        Ok(Self { width, height, rgb })
    }
    /// Decodifica un PNG RGB de 8 bits (unico formato admitido; no hay ruta de alfa).
    /// Dimensiones y formato se validan con el IHDR, antes de preparar la lectura y de
    /// reservar el buffer de la imagen.
    pub fn decode_png<R: BufRead + Seek>(reader: R) -> Result<Self, String> {
        let png_error = |e: png::DecodingError| format!("PNG invalido: {e}");
        let mut decoder = png::Decoder::new_with_limits(
            reader,
            png::Limits {
                bytes: PNG_DECODER_LIMIT,
            },
        );
        decoder.set_transformations(png::Transformations::IDENTITY);
        let info = decoder.read_header_info().map_err(png_error)?;
        let (width, height) = (info.width, info.height);
        if !(1..=MAX_TEXTURE_SIDE).contains(&width) || !(1..=MAX_TEXTURE_SIDE).contains(&height) {
            return Err(format!(
                "dimension de textura fuera de 1..={MAX_TEXTURE_SIDE}: {width}x{height}"
            ));
        }
        if (info.color_type, info.bit_depth) != (png::ColorType::Rgb, png::BitDepth::Eight) {
            return Err(format!(
                "formato PNG no admitido: {:?} {:?} (solo RGB de 8 bits)",
                info.color_type, info.bit_depth
            ));
        }
        let mut reader = decoder.read_info().map_err(png_error)?;
        let mut rgb = vec![0; width as usize * height as usize * 3];
        let frame = reader.next_frame(&mut rgb).map_err(png_error)?;
        if (frame.width, frame.height, frame.buffer_size()) != (width, height, rgb.len()) {
            return Err("PNG invalido: el cuadro decodificado no coincide con el IHDR".into());
        }
        // next_frame termina los pixeles; finish consume hasta IEND y valida su CRC.
        reader.finish().map_err(png_error)?;
        Self::from_rgb8(width, height, rgb)
    }
    pub fn load_png(path: &Path) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
        Self::decode_png(BufReader::new(file))
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Color lineal en `uv` con convencion OBJ (`v = 0` abajo, la fila PNG 0 arriba):
    /// bilineal entre centros de texel, con recorte al borde (`+-inf` incluidos). Cada
    /// texel se linealiza antes de interpolar. `None` si alguna coordenada es NaN.
    pub fn sample(&self, uv: Vec2) -> Option<Vec3> {
        if uv.x.is_nan() || uv.y.is_nan() {
            return None;
        }
        let (w, h) = (self.width as usize, self.height as usize);
        let x = (uv.x.clamp(0.0, 1.0) * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
        let y = ((1.0 - uv.y.clamp(0.0, 1.0)) * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let lut = lut();
        let texel = |tx: usize, ty: usize| {
            let i = (ty * w + tx) * 3;
            Vec3::new(
                lut[self.rgb[i] as usize],
                lut[self.rgb[i + 1] as usize],
                lut[self.rgb[i + 2] as usize],
            )
        };
        let top = texel(x0, y0).lerp(texel(x1, y0), fx);
        let bottom = texel(x0, y1).lerp(texel(x1, y1), fx);
        Some(top.lerp(bottom, fy))
    }
}
