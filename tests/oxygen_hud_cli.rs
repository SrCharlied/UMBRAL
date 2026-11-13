//! HUD de oxigeno por la CLI REAL del juego (`--oxygen-hud`, `--oxygen-reserve`): validacion antes de
//! cualquier archivo, apagado por defecto y autoridad de la reserva real frente al O2 visual.

use std::{fs::File, io::BufReader, path::PathBuf, process::Command};
use umbral::{
    renderer::framebuffer::Framebuffer,
    visual::oxygen_hud::{
        OxygenHudOptions, OxygenReading, draw_oxygen_hud, oxygen_hud_layout, parse_oxygen_reserve,
        validate_oxygen_hud_flags,
    },
};

fn output(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "oxygen_hud_cli_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&p).unwrap();
    p.join("frame.png")
}

fn umbral(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args(args)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

/// Cuadro headless 320x180 del juego normal; devuelve los pixeles 0xRRGGBB.
fn headless(name: &str, extra: &[&str]) -> Vec<u32> {
    let path = output(name);
    let p = path.to_str().unwrap();
    let mut args = vec![
        "--headless",
        "--width",
        "320",
        "--height",
        "180",
        "--output",
        p,
    ];
    args.extend_from_slice(extra);
    let result = umbral(&args);
    assert!(
        result.status.success(),
        "{extra:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut r = png::Decoder::new(BufReader::new(File::open(&path).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut b).unwrap();
    assert_eq!((info.width, info.height), (320, 180));
    b[..info.buffer_size()]
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]))
        .collect()
}

fn with_hud(base: &[u32], reserve: f64) -> Vec<u32> {
    let mut fb = Framebuffer::new(320, 180, 0).unwrap();
    fb.pixels.copy_from_slice(base);
    draw_oxygen_hud(&mut fb, OxygenReading::new(reserve).unwrap());
    fb.pixels
}

#[test]
fn hudcli01_flag_rules_are_checked_without_files() {
    let on = OxygenHudOptions {
        enabled: true,
        initial_reserve: None,
    };
    let reserve = |v: f64| OxygenHudOptions {
        enabled: true,
        initial_reserve: Some(v),
    };
    assert!(validate_oxygen_hud_flags(OxygenHudOptions::default(), true, true).is_ok());
    assert!(validate_oxygen_hud_flags(on, false, false).is_ok());
    assert!(
        validate_oxygen_hud_flags(on, false, true).is_ok(),
        "HUD con O2 visual de depuracion"
    );
    assert!(validate_oxygen_hud_flags(reserve(0.25), false, false).is_ok());
    assert!(
        validate_oxygen_hud_flags(on, true, false).is_err(),
        "con --legacy"
    );
    assert!(
        validate_oxygen_hud_flags(
            OxygenHudOptions {
                enabled: false,
                initial_reserve: Some(0.5)
            },
            false,
            false
        )
        .is_err(),
        "reserva sin HUD"
    );
    assert!(
        validate_oxygen_hud_flags(reserve(0.5), false, true).is_err(),
        "reserva real con --debug-oxygen"
    );
    for bad in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(
            validate_oxygen_hud_flags(reserve(bad), false, false).is_err(),
            "{bad}"
        );
    }
    assert_eq!(parse_oxygen_reserve("0.25"), Ok(0.25));
    assert_eq!(parse_oxygen_reserve("0"), Ok(0.0));
    for bad in ["abc", "NaN", "inf", "-0.1", "1.5", ""] {
        assert!(parse_oxygen_reserve(bad).is_err(), "{bad}");
    }
}

#[test]
fn hudcli02_cli_rejects_bad_combinations_before_writing() {
    for (args, needle) in [
        (
            &["--oxygen-reserve", "0.5"][..],
            "--oxygen-reserve requiere --oxygen-hud",
        ),
        (
            &["--oxygen-hud", "--legacy"][..],
            "--oxygen-hud no se combina con --legacy",
        ),
        (
            &[
                "--oxygen-hud",
                "--debug-oxygen",
                "0.5",
                "--oxygen-reserve",
                "0.3",
            ][..],
            "no se combina con --debug-oxygen",
        ),
        (&["--oxygen-hud", "--oxygen-reserve", "2"][..], "[0, 1]"),
        (
            &["--oxygen-hud", "--oxygen-reserve"][..],
            "Falta el valor despues de --oxygen-reserve",
        ),
    ] {
        let path = output("reject");
        let mut all = vec!["--headless", "--output", path.to_str().unwrap()];
        all.extend_from_slice(args);
        let result = umbral(&all);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "{args:?} fue aceptado");
        assert!(stderr.contains(needle), "{args:?}: {stderr}");
        assert!(!path.exists(), "{args:?} escribio el PNG");
    }
    let help = umbral(&["--help"]);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(
        text.contains("--oxygen-hud") && text.contains("--oxygen-reserve"),
        "{text}"
    );
}

#[test]
fn hudcli03_off_by_default_and_on_draws_the_real_reserve() {
    let off = headless("off", &[]);
    let on = headless("on", &["--oxygen-hud"]);
    assert!(on != off, "--oxygen-hud no dibujo nada");
    assert!(
        on == with_hud(&off, 1.0),
        "HUD distinto del instrumento con la reserva llena"
    );
    let l = oxygen_hud_layout(320, 180).unwrap();
    for (i, (a, b)) in on.iter().zip(&off).enumerate() {
        if !l.bounds.contains(i % 320, i / 320) {
            assert_eq!(a, b, "fuera de la region del HUD");
        }
    }
}

#[test]
fn hudcli04_initial_reserve_is_real_and_debug_oxygen_does_not_move_the_hud() {
    let off = headless("off2", &[]);
    let quarter = headless("quarter", &["--oxygen-hud", "--oxygen-reserve", "0.25"]);
    assert!(
        quarter == with_hud(&off, 0.25),
        "la reserva real inicial no llego al HUD"
    );
    // O2 VISUAL al 25 % con la reserva real llena: el HUD sigue en 100 %.
    let visual = headless("visual", &["--oxygen-hud", "--debug-oxygen", "0.25"]);
    let visual_off = headless("visual_off", &["--debug-oxygen", "0.25"]);
    assert!(
        visual == with_hud(&visual_off, 1.0),
        "el HUD siguio al O2 visual"
    );
}
