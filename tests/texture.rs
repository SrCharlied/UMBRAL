use umbral::math::{Vec2, Vec3};
use umbral::renderer::texture::{MAX_TEXTURE_SIDE, Texture, srgb_to_linear};

// Fixture sintetico 2x2, filas PNG de arriba hacia abajo:
//   fila 0 (arriba): rojo,  verde
//   fila 1 (abajo):  azul,  blanco
fn checker() -> Texture {
    Texture::from_rgb8(2, 2, vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]).unwrap()
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-12
}

#[test]
fn la_lut_srgb_es_la_curva_estandar() {
    assert_eq!(srgb_to_linear(0), 0.0);
    assert_eq!(srgb_to_linear(255), 1.0);
    // Formula estandar IEC 61966-2-1 en sus dos tramos.
    for v in [1u8, 10, 11, 64, 128, 200] {
        let c = f64::from(v) / 255.0;
        let expected = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
        assert!((srgb_to_linear(v) - expected).abs() < 1e-12, "{v}");
    }
    assert!((1..=255u8).all(|v| srgb_to_linear(v) > srgb_to_linear(v - 1)));
}

#[test]
fn las_esquinas_respetan_v_invertida_de_obj() {
    let t = checker();
    // v = 0 es la fila de abajo del PNG; v = 1 la de arriba.
    assert!(close(
        t.sample(Vec2::new(0.0, 0.0)).unwrap(),
        Vec3::new(0.0, 0.0, 1.0)
    ));
    assert!(close(t.sample(Vec2::new(1.0, 0.0)).unwrap(), Vec3::ONE));
    assert!(close(
        t.sample(Vec2::new(0.0, 1.0)).unwrap(),
        Vec3::new(1.0, 0.0, 0.0)
    ));
    assert!(close(
        t.sample(Vec2::new(1.0, 1.0)).unwrap(),
        Vec3::new(0.0, 1.0, 0.0)
    ));
}

#[test]
fn bilineal_en_espacio_lineal_no_en_srgb() {
    let t = checker();
    // Centro: promedio lineal de los cuatro texeles.
    let center = t.sample(Vec2::new(0.5, 0.5)).unwrap();
    assert!(close(center, Vec3::new(0.5, 0.5, 0.5)), "{center:?}");
    // Mitad del borde inferior: entre azul y blanco.
    let bottom = t.sample(Vec2::new(0.5, 0.0)).unwrap();
    assert!(close(bottom, Vec3::new(0.5, 0.5, 1.0)), "{bottom:?}");
    // Gris sRGB: interpolar despues de linealizar da 0.5, no srgb_to_linear(127.5).
    let gray = Texture::from_rgb8(2, 1, vec![0, 0, 0, 255, 255, 255]).unwrap();
    let mid = gray.sample(Vec2::new(0.5, 0.5)).unwrap();
    assert!((mid.x - 0.5).abs() < 1e-12, "{mid:?}");
}

#[test]
fn fuera_de_rango_recorta_al_borde_y_nan_no_tiene_color() {
    let t = checker();
    for (uv, texel) in [
        (Vec2::new(-3.0, -0.5), Vec3::new(0.0, 0.0, 1.0)),
        (Vec2::new(7.0, 2.0), Vec3::new(0.0, 1.0, 0.0)),
        (
            Vec2::new(f64::NEG_INFINITY, f64::INFINITY),
            Vec3::new(1.0, 0.0, 0.0),
        ),
        (Vec2::new(f64::INFINITY, f64::NEG_INFINITY), Vec3::ONE),
    ] {
        assert!(close(t.sample(uv).unwrap(), texel), "{uv:?}");
    }
    assert!(t.sample(Vec2::new(f64::NAN, 0.5)).is_none());
    assert!(t.sample(Vec2::new(0.5, f64::NAN)).is_none());
}

#[test]
fn una_textura_de_un_texel_es_constante() {
    let t = Texture::from_rgb8(1, 1, vec![128, 64, 255]).unwrap();
    let expected = Vec3::new(srgb_to_linear(128), srgb_to_linear(64), 1.0);
    for uv in [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.3, 0.9),
        Vec2::new(1.0, 1.0),
    ] {
        assert!(close(t.sample(uv).unwrap(), expected));
    }
}

#[test]
fn rechaza_dimensiones_y_buffers_incoherentes() {
    let side = MAX_TEXTURE_SIDE;
    for (w, h, len, reason) in [
        (0, 2, 0, "dimension"),
        (2, 0, 0, "dimension"),
        (side + 1, 1, (side as usize + 1) * 3, "dimension"),
        (1, side + 1, (side as usize + 1) * 3, "dimension"),
        (2, 2, 11, "bytes"),
        (2, 2, 13, "bytes"),
    ] {
        match Texture::from_rgb8(w, h, vec![0; len]) {
            Err(error) => assert!(error.contains(reason), "{w}x{h}/{len}: {error}"),
            Ok(_) => panic!("{w}x{h} con {len} bytes aceptado"),
        }
    }
    let max = Texture::from_rgb8(side, 1, vec![7; side as usize * 3]).unwrap();
    assert_eq!((max.width(), max.height()), (side, 1));
}

fn encode(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if color == png::ColorType::Indexed {
            encoder.set_palette(vec![255, 0, 0, 0, 255, 0]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    out
}

fn decode(bytes: Vec<u8>) -> Result<Texture, String> {
    Texture::decode_png(std::io::Cursor::new(bytes))
}

fn expect_err(result: Result<Texture, String>, reason: &str, case: &str) {
    match result {
        Err(error) => assert!(error.contains(reason), "{case}: {error}"),
        Ok(t) => panic!("{case}: aceptado {}x{}", t.width(), t.height()),
    }
}

#[test]
fn decodifica_png_rgb8_con_las_mismas_filas_que_el_fixture() {
    let bytes = encode(
        2,
        2,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
    );
    let t = decode(bytes).unwrap();
    assert_eq!((t.width(), t.height()), (2, 2));
    let reference = checker();
    for uv in [
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.25, 0.8),
        Vec2::new(0.5, 0.5),
    ] {
        assert!(
            close(t.sample(uv).unwrap(), reference.sample(uv).unwrap()),
            "{uv:?}"
        );
    }
}

#[test]
fn rechaza_formatos_distintos_de_rgb8() {
    use png::{BitDepth::*, ColorType::*};
    for (color, depth, data, case) in [
        (Rgba, Eight, vec![0u8; 16], "RGBA8"),
        (Grayscale, Eight, vec![0u8; 4], "gris 8"),
        (GrayscaleAlpha, Eight, vec![0u8; 8], "gris+alfa 8"),
        (Indexed, Eight, vec![0u8, 1, 1, 0], "paleta 8"),
        (Rgb, Sixteen, vec![0u8; 24], "RGB16"),
    ] {
        expect_err(decode(encode(2, 2, color, depth, &data)), "formato", case);
    }
}

/// CRC-32 de PNG (polinomio 0xEDB88320), solo para fabricar cabeceras de prueba.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out
}

#[test]
fn rechaza_dimensiones_excesivas_antes_de_reservar_la_imagen() {
    let wide = MAX_TEXTURE_SIDE + 1;
    let real = encode(
        wide,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &vec![0; wide as usize * 3],
    );
    expect_err(decode(real), "dimension", "4097x1 real");
    // Cabecera que declara 100000x100000 RGB8 (~28 GiB) sin datos de imagen: se rechaza
    // leyendo solo el IHDR.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&100_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut fake = b"\x89PNG\r\n\x1a\n".to_vec();
    fake.extend(chunk(b"IHDR", &ihdr));
    fake.extend(chunk(b"IEND", &[]));
    expect_err(decode(fake), "dimension", "IHDR 100000x100000");
}

#[test]
fn rechaza_png_con_crc_final_corrupto_o_sin_iend() {
    let good = encode(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[255; 3]);
    let len = good.len();
    assert_eq!(&good[len - 8..len - 4], b"IEND");
    assert!(decode(good.clone()).is_ok());
    expect_err(decode(good[..len - 12].to_vec()), "PNG", "IEND ausente");
    let mut flipped = good;
    flipped[len - 1] ^= 1;
    expect_err(decode(flipped), "PNG", "CRC de IEND alterado");
}

#[test]
fn rechaza_png_corruptos_truncados_o_ausentes() {
    let good = encode(
        2,
        2,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
    );
    let mut flipped = good.clone();
    let idat = flipped.windows(4).position(|w| w == b"IDAT").unwrap();
    flipped[idat + 6] ^= 0xFF;
    expect_err(decode(flipped), "PNG", "byte de IDAT alterado");
    expect_err(decode(good[..good.len() / 2].to_vec()), "PNG", "truncado");
    expect_err(decode(b"no es un png".to_vec()), "PNG", "basura");
    let missing = std::path::Path::new("no-existe/textura.png");
    expect_err(
        Texture::load_png(missing),
        "textura.png",
        "ruta inexistente",
    );
}
