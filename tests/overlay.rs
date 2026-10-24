use umbral::{
    debug::overlay::{OverlayState, draw_overlay, draw_text, overlay_lines},
    renderer::{framebuffer::Framebuffer, stats::RenderStats},
};

#[test]
fn el_texto_dibuja_pixeles_solo_donde_hay_glifos() {
    let mut fb = Framebuffer::new(64, 16, 0).unwrap();
    let width = draw_text(&mut fb, 2, 2, 1, 0xffffff, "I");
    assert_eq!(width, 6);
    // La barra superior de la I ocupa 3 columnas (0x0E) en la primera fila.
    let row = |y: usize| (0..64).filter(|&x| fb.pixels[y * 64 + x] != 0).count();
    assert_eq!(row(2), 3);
    assert_eq!(row(5), 1);
    assert_eq!(row(0), 0);
}

#[test]
fn el_texto_fuera_del_framebuffer_se_ignora_sin_panico() {
    let mut fb = Framebuffer::new(8, 8, 0).unwrap();
    draw_text(&mut fb, 6, 6, 3, 0xffffff, "WWW ???");
    draw_text(&mut fb, 1000, 1000, 2, 0xffffff, "X");
}

#[test]
fn el_overlay_muestra_los_contadores_y_oscurece_el_fondo() {
    let stats = RenderStats {
        triangles_submitted: 20174,
        triangles_culled: 7534,
        fragments_evaluated: 119118,
        ..RenderStats::default()
    };
    let state = OverlayState {
        culling: true,
        lod: false,
        backface: true,
        orbits_frozen: false,
    };
    let lines = overlay_lines(Some(60.0), &stats, &state, 12.0, 0.5, 3.0);
    let all = lines.join("\n");
    for expected in [
        "20174",
        "7534",
        "119118",
        "LOD OFF",
        "CULLING ON",
        "OXIGENO 50%",
    ] {
        assert!(all.contains(expected), "falta {expected}: {all}");
    }
    assert!(!overlay_lines(None, &stats, &state, 0.0, 1.0, 0.0)[0].contains("FPS"));
    let mut fb = Framebuffer::new(1280, 720, 0x808080).unwrap();
    draw_overlay(&mut fb, &lines);
    assert!(fb.pixels[0] < 0x808080);
    assert_eq!(fb.pixels[719 * 1280 + 1279], 0x808080);
}
