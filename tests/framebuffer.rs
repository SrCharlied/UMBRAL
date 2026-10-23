use umbral::renderer::framebuffer::Framebuffer;

#[test]
fn pinta_pixeles_con_limites_profundidad_y_limpieza() {
    let mut fb = Framebuffer::new(3, 3, 0x010203).expect("framebuffer valido");
    assert_eq!(fb.pixels, vec![0x010203; 9]);
    fb.set_color(0xff0000);
    assert!(fb.point(1, 1, 4.0));
    fb.set_color(0x00ff00);
    assert!(!fb.point(1, 1, 6.0));
    assert_eq!(fb.pixels[4], 0xff0000);
    assert!(fb.point(1, 1, 2.0));
    assert_eq!(fb.pixels[4], 0x00ff00);
    assert!(!fb.point(-1, 0, 1.0));
    assert!(!fb.point(3, 0, 1.0));
    assert!(!fb.point(0, 0, f64::NAN));
    assert!(!fb.point(0, 0, f64::INFINITY));
    fb.clear();
    assert_eq!(fb.pixels, vec![0x010203; 9]);
    assert!(
        fb.point(1, 1, 20.0),
        "clear tambien reinicia la profundidad"
    );
    assert!(Framebuffer::new(0, 3, 0).is_err());
    assert!(Framebuffer::new(usize::MAX, 2, 0).is_err());
}
