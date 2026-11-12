use std::{fs::File, io::BufReader, path::PathBuf, process::Command};
use umbral::{
    assets::{obj::load_model, ship_model_path},
    renderer::framebuffer::Framebuffer,
    scene::world::World,
};
fn output(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "correction_{}_{}_{}",
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
fn pixels(p: &std::path::Path) -> Vec<u8> {
    let mut r = png::Decoder::new(BufReader::new(File::open(p).unwrap()))
        .read_info()
        .unwrap();
    let mut b = vec![0; r.output_buffer_size().unwrap()];
    let n = r.next_frame(&mut b).unwrap().buffer_size();
    b.truncate(n);
    b
}
fn rgb(f: &Framebuffer) -> Vec<u8> {
    f.pixels
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect()
}
#[test]
fn explicit_feedback_flag_matches_existing_layer_and_zero_post_is_black() {
    use umbral::app::draw_gameplay_feedback;
    for zero in [false, true] {
        let path = output("optin");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_umbral"));
        cmd.args([
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--gameplay-feedback",
            "--output",
        ])
        .arg(&path);
        if zero {
            cmd.args(["--lucid-perception", "--debug-oxygen", "0"]);
        }
        let result = cmd.output().unwrap();
        assert!(
            result.status.success(),
            "feedback flag rejected: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual = pixels(&path);
        if zero {
            assert!(actual.iter().all(|&x| x == 0));
        } else {
            let w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
            let mut fb = Framebuffer::new(320, 180, umbral::scene::world::SPACE_COLOR).unwrap();
            w.render(&mut fb);
            draw_gameplay_feedback(&mut fb, &w);
            assert!(
                actual == rgb(&fb),
                "optin feedback differs or scene order changed"
            );
        }
    }
}

#[test]
fn explicit_debug_one_retains_origin_but_default_tracks_real() {
    for explicit in [false, true] {
        let path = output("origin");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_umbral"));
        cmd.args([
            "--headless",
            "--width",
            "640",
            "--height",
            "360",
            "--gameplay-feedback",
            "--output",
        ])
        .arg(&path);
        if explicit {
            cmd.args(["--debug-oxygen", "1"]);
        }
        let result = cmd.output().unwrap();
        assert!(result.status.success());
        let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
        if explicit {
            w.set_debug_oxygen(1.0);
        }
        let mut fb = Framebuffer::new(640, 360, umbral::scene::world::SPACE_COLOR).unwrap();
        w.render(&mut fb);
        umbral::app::draw_gameplay_feedback(&mut fb, &w);
        assert!(
            pixels(&path) == rgb(&fb),
            "CLI lost explicit debug-one origin: explicit={explicit}"
        );
    }
}

#[test]
fn terminal_feedback_names_only_the_explicit_full_restart_gesture() {
    let mut w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    w.gameplay.game_over = true;
    let lines = umbral::app::gameplay_feedback_lines(&w).join(" ");
    assert!(
        lines.contains("SHIFT+R: REINICIAR"),
        "feedback still promises R fullrestart: {lines}"
    );
}

#[test]
fn default_frame_is_integrated_beacons_without_technical_feedback() {
    let path = output("default");
    let result = Command::new(env!("CARGO_BIN_EXE_umbral"))
        .args([
            "--headless",
            "--width",
            "320",
            "--height",
            "180",
            "--output",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    assert_eq!(w.gameplay.resources.len() + w.gameplay.logs.len(), 6);
    let mut fb = Framebuffer::new(320, 180, umbral::scene::world::SPACE_COLOR).unwrap();
    w.render(&mut fb);
    assert!(
        pixels(&path) == rgb(&fb),
        "default CLI added technical feedback over integrated scene"
    );
}

/// CORRECTION03: la ventana, `P` y el headless pasan por el MISMO compositor (`compose_presentation`);
/// la franja tecnica sigue siendo opcional ahi y la percepcion es el ultimo paso de color. Esto cubre
/// el codigo compartido; no certifica la interaccion fisica con la ventana.
#[test]
fn correction03_shared_composer_keeps_feedback_opt_in_and_perception_last() {
    use umbral::{
        app::{PresentationLayers, compose_presentation, draw_gameplay_feedback},
        visual::creature_distant_turn::CreaturePreset,
    };
    let layers = |gameplay_feedback: bool, perception: bool| PresentationLayers {
        overlay: None,
        comparison: false,
        distant_turn: false,
        horizontal: false,
        preset: CreaturePreset::Far,
        gameplay_feedback,
        perception,
        ending: None,
    };
    let scene = |w: &World| {
        let mut fb = Framebuffer::new(320, 180, umbral::scene::world::SPACE_COLOR).unwrap();
        w.render(&mut fb);
        fb
    };
    let w = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    let base = scene(&w).pixels;
    let mut off = scene(&w);
    compose_presentation(&mut off, &w, &layers(false, false)).unwrap();
    assert!(
        off.pixels == base,
        "el compositor dibujo la franja sin pedirla"
    );
    let mut on = scene(&w);
    compose_presentation(&mut on, &w, &layers(true, false)).unwrap();
    let mut expected = scene(&w);
    draw_gameplay_feedback(&mut expected, &w);
    assert!(
        on.pixels == expected.pixels,
        "franja distinta de la capa existente"
    );
    assert!(
        on.pixels != base,
        "precondicion: la franja cambia el cuadro"
    );
    let mut zero = World::new(load_model(&ship_model_path()).unwrap(), 16.0 / 9.0).unwrap();
    zero.set_debug_oxygen(0.0);
    let mut last = scene(&zero);
    compose_presentation(&mut last, &zero, &layers(true, true)).unwrap();
    assert!(
        last.pixels.iter().all(|&p| p & 0x00ff_ffff == 0),
        "la percepcion no fue el ultimo paso (queda franja brillante con O2 0)"
    );
}
