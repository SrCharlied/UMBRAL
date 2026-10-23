use std::collections::BTreeSet;
use umbral::{
    assets::{
        obj::{load_model, parse_obj},
        ship_model_path,
    },
    renderer::{
        framebuffer::Framebuffer,
        legacy::{BACKGROUND, render_model},
    },
};

#[test]
fn rasteriza_todas_las_caras_de_la_nave_y_la_centra_sin_recortarla() {
    let model = load_model(&ship_model_path()).unwrap();
    for (width, height, angle) in [(800, 600, 0.0), (480, 800, 0.7)] {
        let mut fb = Framebuffer::new(width, height, BACKGROUND).unwrap();
        let stats = render_model(&mut fb, &model, angle).expect("renderer por software");
        assert_eq!(stats.faces_processed, 4302);
        assert!(stats.pixels_written > 1000);
        let mut bounds = [width, height, 0, 0];
        for (index, &pixel) in fb.pixels.iter().enumerate() {
            if pixel != BACKGROUND {
                let (x, y) = (index % width, index / width);
                bounds = [
                    bounds[0].min(x),
                    bounds[1].min(y),
                    bounds[2].max(x),
                    bounds[3].max(y),
                ];
            }
        }
        assert!(bounds[0] as f64 >= width as f64 * 0.08);
        assert!(bounds[1] as f64 >= height as f64 * 0.08);
        assert!((bounds[2] as f64) < width as f64 * 0.92);
        assert!((bounds[3] as f64) < height as f64 * 0.92);
        assert!(
            ((bounds[0] + bounds[2]) as f64 / 2.0 - width as f64 / 2.0).abs() < width as f64 * 0.03
        );
        assert!(
            ((bounds[1] + bounds[3]) as f64 / 2.0 - height as f64 / 2.0).abs()
                < height as f64 * 0.03
        );
        let extent = ((bounds[2] - bounds[0]) as f64 / width as f64)
            .max((bounds[3] - bounds[1]) as f64 / height as f64);
        assert!(extent > 0.75 && extent < 0.9, "tamano razonable: {extent}");
        assert!(fb.pixels.iter().copied().collect::<BTreeSet<_>>().len() > 10);
    }
}

#[test]
fn rechaza_proyecciones_sin_extension_o_angulos_no_finitos() {
    let model = parse_obj("v 0 0 0\nv 0 0 0\nv 0 0 0\nf 1 2 3").unwrap();
    let mut fb = Framebuffer::new(40, 40, BACKGROUND).unwrap();
    assert!(render_model(&mut fb, &model, 0.0).is_err());
    assert!(render_model(&mut fb, &model, f64::NAN).is_err());
}
