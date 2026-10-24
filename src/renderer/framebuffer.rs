/// Maximo de pixeles aceptado por `Framebuffer::new` (equivale a 4096x4096).
pub const MAX_PIXELS: usize = 16_777_216;

pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    depth: Vec<f64>,
    background: u32,
    current_color: u32,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize, background: u32) -> Result<Self, String> {
        let count = width
            .checked_mul(height)
            .filter(|&n| n > 0 && n <= MAX_PIXELS)
            .ok_or("Dimensiones de framebuffer invalidas o demasiado grandes")?;
        Ok(Self {
            width,
            height,
            pixels: vec![background; count],
            depth: vec![f64::INFINITY; count],
            background,
            current_color: 0xffffff,
        })
    }
    pub fn set_color(&mut self, color: u32) {
        self.current_color = color & 0xffffff;
    }
    pub fn clear(&mut self) {
        self.pixels.fill(self.background);
        self.depth.fill(f64::INFINITY);
    }
    pub fn point(&mut self, x: isize, y: isize, depth: f64) -> bool {
        if x < 0
            || y < 0
            || x as usize >= self.width
            || y as usize >= self.height
            || !depth.is_finite()
        {
            return false;
        }
        let index = y as usize * self.width + x as usize;
        if depth < self.depth[index] {
            self.depth[index] = depth;
            self.pixels[index] = self.current_color;
            true
        } else {
            false
        }
    }
    /// Verdadero si un fragmento a esta profundidad ganaria el z-test (sin escribir).
    pub fn depth_passes(&self, x: isize, y: isize, depth: f64) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && depth.is_finite()
            && depth < self.depth[y as usize * self.width + x as usize]
    }
    /// Escribe `color` si el z-test pasa; no usa `set_color`.
    pub fn write(&mut self, x: isize, y: isize, depth: f64, color: u32) -> bool {
        if !self.depth_passes(x, y, depth) {
            return false;
        }
        let index = y as usize * self.width + x as usize;
        self.depth[index] = depth;
        self.pixels[index] = color & 0xffffff;
        true
    }
    /// Escribe un pixel sin z-test (superposiciones 2D como el overlay).
    pub fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = color & 0xffffff;
        }
    }
    /// Oscurece un rectangulo multiplicando cada canal por `1 - amount`.
    pub fn darken_rect(&mut self, x: usize, y: usize, w: usize, h: usize, amount: f64) {
        let keep = ((1.0 - amount.clamp(0.0, 1.0)) * 256.0) as u32;
        for py in y..(y + h).min(self.height) {
            for px in x..(x + w).min(self.width) {
                let p = &mut self.pixels[py * self.width + px];
                let ch = |shift: u32| ((((*p >> shift) & 0xff) * keep) >> 8) << shift;
                *p = ch(16) | ch(8) | ch(0);
            }
        }
    }
}
