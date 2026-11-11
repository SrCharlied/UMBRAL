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

/// Creates a new PNG exclusively and encodes through that same handle. On an
/// encoding error the newly created file may be partial; existing files are never opened.
pub fn save_png_exclusive(path: &Path, framebuffer: &Framebuffer) -> Result<(), String> {
    save_png_exclusive_after_reserve(path, framebuffer, || Ok(()))
}

fn save_png_exclusive_after_reserve(
    path: &Path,
    framebuffer: &Framebuffer,
    after_reserve: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    use std::io::{BufWriter, Write};
    let width = u32::try_from(framebuffer.width).map_err(|e| e.to_string())?;
    let height = u32::try_from(framebuffer.height).map_err(|e| e.to_string())?;
    if width == 0
        || height == 0
        || framebuffer.width.checked_mul(framebuffer.height) != Some(framebuffer.pixels.len())
    {
        return Err("PNG: dimensiones o numero de pixeles invalidos".into());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("No se pudo crear la carpeta de salida: {e}"))?;
    }
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("No se pudo crear PNG exclusivo {}: {e}", path.display()))?;
    after_reserve()?;
    let mut output = BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut output, width, height);
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

#[cfg(test)]
mod exclusive_writer_tests {
    use super::*;
    fn fresh() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gt3_writer_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        dir
    }
    #[test]
    fn exclusive_png_matches_legacy_and_repeated_press_is_nondestructive() {
        let d = fresh();
        let fb = Framebuffer::new(3, 2, 0x123456).unwrap();
        let path = d.join("p.png");
        let legacy = d.join("legacy.png");
        save_png(&legacy, &fb).unwrap();
        save_png_exclusive(&path, &fb).unwrap();
        let first = std::fs::read(&path).unwrap();
        assert_eq!(first, std::fs::read(&legacy).unwrap());
        assert!(
            save_png_exclusive(&path, &fb)
                .unwrap_err()
                .contains("PNG exclusivo")
        );
        assert_eq!(first, std::fs::read(&path).unwrap());
        let sentinel = d.join("sentinel.png");
        std::fs::write(&sentinel, b"ParentSentinel").unwrap();
        let alias = d.join("owned_hardlink.png");
        std::fs::hard_link(&sentinel, &alias).unwrap();
        assert!(save_png_exclusive(&alias, &fb).is_err());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"ParentSentinel");
        println!(
            "WRITER positive_exact=true repeated_collision_intact=true owned_hardlink_intact=true"
        );
    }
    #[test]
    fn reservation_swap_barrier_keeps_encoding_on_original_handle() {
        use std::sync::{Arc, Barrier};
        let d = fresh();
        let path = d.join("race.png");
        let reserved = d.join("reserved.png");
        let a = Arc::new(Barrier::new(2));
        let b = Arc::new(Barrier::new(2));
        let worker = {
            let (a, b, path) = (a.clone(), b.clone(), path.clone());
            std::thread::spawn(move || {
                let fb = Framebuffer::new(3, 2, 0x123456).unwrap();
                save_png_exclusive_after_reserve(&path, &fb, || {
                    a.wait();
                    b.wait();
                    Ok(())
                })
            })
        };
        a.wait();
        let swapped = std::fs::rename(&path, &reserved);
        if swapped.is_ok() {
            std::fs::write(&path, b"ParentSentinel").unwrap();
        }
        b.wait();
        worker.join().unwrap().unwrap();
        swapped.expect("owned ordinary-file rename must exercise swap, not skip");
        assert_eq!(std::fs::read(&path).unwrap(), b"ParentSentinel");
        let legacy = d.join("legacy.png");
        save_png(&legacy, &Framebuffer::new(3, 2, 0x123456).unwrap()).unwrap();
        assert_eq!(
            std::fs::read(&reserved).unwrap(),
            std::fs::read(legacy).unwrap()
        );
        println!("WRITER barrier_swap_exercised=true sentinel_intact=true reserved_png_exact=true");
    }
    #[test]
    fn invalid_public_framebuffer_rejected_before_directory_creation() {
        let d = fresh();
        let path = d.join("must_not_exist").join("bad.png");
        let mut fb = Framebuffer::new(2, 2, 0).unwrap();
        fb.pixels.pop();
        assert!(
            save_png_exclusive(&path, &fb)
                .unwrap_err()
                .contains("pixeles invalidos")
        );
        assert!(!path.parent().unwrap().exists());
    }
}
