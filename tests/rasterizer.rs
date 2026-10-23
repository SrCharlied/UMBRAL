use umbral::renderer::framebuffer::Framebuffer;
use umbral::renderer::rasterizer::{ScreenVertex, triangle};

fn v(x: f64, y: f64, z: f64) -> ScreenVertex {
    ScreenVertex { x, y, z }
}

#[test]
fn rellena_el_interior_recorta_y_acepta_ambos_ordenes_de_vertices() {
    let vertices = [v(1.0, 1.0, 1.0), v(6.0, 1.0, 1.0), v(1.0, 6.0, 1.0)];
    let mut fb = Framebuffer::new(8, 8, 0).unwrap();
    fb.set_color(0xffcc00);
    assert!(triangle(&mut fb, vertices) > 5);
    assert_eq!(fb.pixels[2 * 8 + 2], 0xffcc00);
    assert_eq!(fb.pixels[6 * 8 + 6], 0);
    let expected = fb.pixels.clone();
    fb.clear();
    triangle(&mut fb, [vertices[2], vertices[1], vertices[0]]);
    assert_eq!(fb.pixels, expected);
    fb.clear();
    assert!(
        triangle(
            &mut fb,
            [
                v(-100.0, -100.0, 0.0),
                v(100.0, 0.0, 0.0),
                v(0.0, 100.0, 0.0)
            ]
        ) > 0
    );
    assert_eq!(
        triangle(
            &mut fb,
            [v(0.0, 0.0, 0.0), v(1.0, 1.0, 0.0), v(2.0, 2.0, 0.0)]
        ),
        0
    );
    assert_eq!(
        triangle(&mut fb, [v(f64::NAN, 0.0, 0.0), vertices[1], vertices[2]]),
        0
    );
}

#[test]
fn interpola_la_profundidad_antes_de_pintar_cada_pixel() {
    let mut fb = Framebuffer::new(8, 8, 0).unwrap();
    fb.set_color(0xff0000);
    triangle(
        &mut fb,
        [v(0.0, 0.0, 5.0), v(7.0, 0.0, 5.0), v(0.0, 7.0, 5.0)],
    );
    fb.set_color(0x00ff00);
    triangle(
        &mut fb,
        [v(0.0, 0.0, 0.0), v(7.0, 0.0, 10.0), v(0.0, 7.0, 10.0)],
    );
    assert_eq!(fb.pixels[0], 0x00ff00);
    assert_eq!(fb.pixels[8 + 4], 0xff0000);
}
