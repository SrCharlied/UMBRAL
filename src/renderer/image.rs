use super::framebuffer::Framebuffer;
use std::path::Path;

pub fn save_png(path: &Path, framebuffer: &Framebuffer) -> Result<(), String> {
    use std::{
        fs::File,
        io::{BufWriter, Write},
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear la carpeta de salida: {e}"))?;
    }
    let file =
        File::create(path).map_err(|e| format!("No se pudo crear {}: {e}", path.display()))?;
    let mut output = BufWriter::new(file);
    let mut encoder = png::Encoder::new(
        &mut output,
        framebuffer.width as u32,
        framebuffer.height as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let rgb: Vec<u8> = framebuffer
        .pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}
