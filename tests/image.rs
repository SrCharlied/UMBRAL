use std::{fs::File, io::BufReader};
use umbral::renderer::{framebuffer::Framebuffer, image::save_png};

#[test]
fn el_png_contiene_los_pixeles_del_framebuffer_sin_invertir_canales() {
    let path = std::env::temp_dir().join(format!("umbral_{}_png.png", std::process::id()));
    let mut fb = Framebuffer::new(2, 2, 0x010203).unwrap();
    fb.set_color(0xff0000);
    fb.point(0, 0, 0.0);
    fb.set_color(0x0000ff);
    fb.point(1, 1, 0.0);
    save_png(&path, &fb).expect("se debe escribir el PNG real");
    let decoder = png::Decoder::new(BufReader::new(File::open(&path).unwrap()));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (2, 2));
    assert_eq!(
        &bytes[..info.buffer_size()],
        &[255, 0, 0, 1, 2, 3, 1, 2, 3, 0, 0, 255]
    );
    std::fs::remove_file(path).unwrap();
}
