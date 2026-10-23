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
}
