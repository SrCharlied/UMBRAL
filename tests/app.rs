use std::{fs::File, io::BufReader, process::Command};

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
    assert_eq!((info.width, info.height), (1280, 960));
    assert!(
        bytes[..info.buffer_size()]
            .chunks_exact(3)
            .any(|rgb| rgb != [16, 25, 31])
    );
    std::fs::remove_file(path).unwrap();
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
