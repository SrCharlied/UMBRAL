use std::{fs::File, io::BufReader, process::Command};
use umbral::renderer::framebuffer::MAX_PIXELS;

#[test]
fn el_programa_headless_carga_el_obj_y_exporta_un_png_real() {
    let path = std::env::temp_dir().join(format!("umbral_app_{}.png", std::process::id()));
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--headless")
        .arg("--output")
        .arg(&path)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("2153 vertices") && stdout.contains("4302 caras"));
    let decoder = png::Decoder::new(BufReader::new(File::open(&path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (1280, 720));
    assert!(
        bytes[..info.buffer_size()]
            .chunks_exact(3)
            .any(|rgb| rgb != [3, 4, 10])
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn rechaza_dimensiones_invalidas_sin_generar_png() {
    let too_many = (MAX_PIXELS + 1).to_string();
    let overflow = usize::MAX.to_string();
    let cases: [(&[&str], &str); 9] = [
        (&["--width"], "Falta la cantidad despues de --width"),
        (&["--height"], "Falta la cantidad despues de --height"),
        (&["--width", "ancho"], "--width necesita un entero positivo"),
        (
            &["--height", "-720"],
            "--height necesita un entero positivo",
        ),
        (
            &["--width", "99999999999999999999999"],
            "--width necesita un entero positivo",
        ),
        (&["--width", "0"], "--width debe ser mayor que cero"),
        (&["--height", "0"], "--height debe ser mayor que cero"),
        (
            &["--width", &too_many, "--height", "1"],
            "demasiado grandes",
        ),
        (
            &["--width", &overflow, "--height", "2"],
            "demasiado grandes",
        ),
    ];
    let path = std::env::temp_dir().join(format!("umbral_invalido_{}.png", std::process::id()));
    for (args, expected) in cases {
        let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
            .arg("--headless")
            .arg("--output")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(result.status.code(), Some(1), "{args:?}: {stderr}");
        assert!(stderr.contains(expected), "{args:?}: {stderr}");
        assert!(!path.exists(), "{args:?} genero un PNG");
    }
}

#[test]
fn la_linea_de_comandos_explica_los_argumentos_invalidos() {
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .arg("--opcion-inexistente")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Argumento desconocido"));
}
