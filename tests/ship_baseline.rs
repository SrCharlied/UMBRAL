use std::{fs::File, io::BufReader, path::PathBuf, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::{
        framebuffer::Framebuffer,
        legacy::{BACKGROUND, render_model},
    },
};

// Generado por `umbral --headless` a 1280x960 (hoy: `--width 1280 --height 960`) y
// verificado identico a renders/nave.png de carga-modelos-24531 @ 51fbaed (SHA-256
// 5906177...0bfa). Solo se reemplaza con una decision explicita del equipo, nunca
// para "arreglar" esta prueba.
fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/legacy_ship_1280x960.png")
}

#[test]
fn el_render_legado_de_la_nave_coincide_pixel_a_pixel_con_el_baseline() {
    let file = File::open(baseline_path()).expect("falta el PNG baseline de la nave");
    let mut reader = png::Decoder::new(BufReader::new(file)).read_info().unwrap();
    let mut expected = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut expected).unwrap();
    assert_eq!((info.width, info.height), (1280, 960));
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgb, png::BitDepth::Eight)
    );
    let expected = &expected[..info.buffer_size()];

    let model = load_model(&ship_model_path()).unwrap();
    let mut fb = Framebuffer::new(1280, 960, BACKGROUND).unwrap();
    let stats = render_model(&mut fb, &model, 0.0).unwrap();
    assert_eq!(stats.faces_processed, 4302);
    assert_eq!(stats.pixels_written, 477_847);

    let actual: Vec<u8> = fb
        .pixels
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    let differing: Vec<usize> = actual
        .chunks_exact(3)
        .zip(expected.chunks_exact(3))
        .enumerate()
        .filter(|(_, (a, e))| a != e)
        .map(|(index, _)| index)
        .collect();
    assert!(
        differing.is_empty(),
        "{} pixeles distintos; primero en (x={}, y={})",
        differing.len(),
        differing[0] % 1280,
        differing[0] / 1280
    );
}

#[test]
fn el_modo_headless_reproduce_el_png_baseline_byte_a_byte() {
    let path = std::env::temp_dir().join(format!("umbral_baseline_{}.png", std::process::id()));
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--legacy",
            "--headless",
            "--width",
            "1280",
            "--height",
            "960",
        ])
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let actual = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let expected = std::fs::read(baseline_path()).expect("falta el PNG baseline de la nave");
    assert!(
        actual == expected,
        "el PNG headless difiere del baseline ({} vs {} bytes)",
        actual.len(),
        expected.len()
    );
}
