use umbral::{
    assets::{
        obj::{Model, parse_obj},
        primitives::uv_sphere,
    },
    math::{Transform, Vec2, Vec3},
    renderer::{
        clipping::Frustum,
        framebuffer::Framebuffer,
        pipeline::draw_model,
        rasterizer::{RasterVertex, triangle_shaded},
        shader::{Material, ShaderContext, ShaderKind, to_rgb8},
        stats::RenderStats,
    },
    scene::camera::Camera,
};

const BG: u32 = 0x000000;

fn triangle_model() -> Model {
    parse_obj("v -1 -1 0\nv 1 -1 0\nv 0 1 0\nf 1 2 3\n").unwrap()
}

fn context() -> ShaderContext {
    ShaderContext {
        elapsed_time: 0.0,
        oxygen01: 1.0,
        camera_position: Vec3::new(0.0, 0.0, 10.0),
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(0.0, 0.0, 100.0),
    }
}

fn camera() -> Camera {
    let mut c = Camera::new(60f64.to_radians(), 1.0, 0.1, 1000.0).unwrap();
    c.position = Vec3::new(0.0, 0.0, 10.0);
    c.target = Vec3::ZERO;
    c
}

fn draw(fb: &mut Framebuffer, at: Vec3, color: (f64, f64, f64)) -> RenderStats {
    draw_model(
        fb,
        &triangle_model(),
        &Transform::at(at),
        &Material::solid(color.0, color.1, color.2),
        ShaderKind::Flat,
        &context(),
        &camera(),
    )
}

fn center(fb: &Framebuffer) -> u32 {
    fb.pixels[(fb.height / 2) * fb.width + fb.width / 2]
}

#[test]
fn dos_instancias_en_lugares_distintos_aparecen_a_la_vez() {
    let mut fb = Framebuffer::new(100, 100, BG).unwrap();
    draw(&mut fb, Vec3::new(-3.0, 0.0, 0.0), (1.0, 0.0, 0.0));
    draw(&mut fb, Vec3::new(3.0, 0.0, 0.0), (0.0, 1.0, 0.0));
    let at = |x: usize, y: usize| fb.pixels[y * 100 + x];
    assert_eq!(at(25, 52), to_rgb8(Vec3::new(1.0, 0.0, 0.0)));
    assert_eq!(at(75, 52), to_rgb8(Vec3::new(0.0, 1.0, 0.0)));
}

#[test]
fn el_z_buffer_ignora_el_orden_de_dibujo() {
    let red = to_rgb8(Vec3::new(1.0, 0.0, 0.0));
    for red_first in [true, false] {
        let mut fb = Framebuffer::new(100, 100, BG).unwrap();
        let (near, far) = (Vec3::new(0.0, 0.0, 2.0), Vec3::new(0.0, 0.0, -2.0));
        if red_first {
            draw(&mut fb, near, (1.0, 0.0, 0.0));
            draw(&mut fb, far, (0.0, 0.0, 1.0));
        } else {
            draw(&mut fb, far, (0.0, 0.0, 1.0));
            draw(&mut fb, near, (1.0, 0.0, 0.0));
        }
        assert_eq!(center(&fb), red, "red_first={red_first}");
    }
}

#[test]
fn la_perspectiva_hace_mas_pequeno_lo_lejano() {
    let count = |z: f64| {
        let mut fb = Framebuffer::new(100, 100, BG).unwrap();
        draw(&mut fb, Vec3::new(0.0, 0.0, z), (1.0, 1.0, 1.0)).pixels_written
    };
    assert!(count(0.0) > 3 * count(-10.0));
}

#[test]
fn un_triangulo_que_cruza_el_plano_cercano_se_recorta_sin_romperse() {
    let mut fb = Framebuffer::new(100, 100, BG).unwrap();
    // Camara a z=10; el triangulo va de z=+20 (detras) a z=-20 (delante).
    let model = parse_obj("v -1 -1 20\nv 1 -1 20\nv 0 -1 -20\nf 1 2 3\n").unwrap();
    let stats = draw_model(
        &mut fb,
        &model,
        &Transform::IDENTITY,
        &Material::solid(1.0, 1.0, 1.0),
        ShaderKind::Flat,
        &context(),
        &camera(),
    );
    assert_eq!(stats.triangles_clipped, 1);
    assert!(stats.pixels_written > 0);
}

#[test]
fn lo_que_esta_detras_de_la_camara_se_descarta() {
    let mut fb = Framebuffer::new(100, 100, BG).unwrap();
    let stats = draw(&mut fb, Vec3::new(0.0, 0.0, 30.0), (1.0, 1.0, 1.0));
    assert_eq!((stats.triangles_culled, stats.pixels_written), (1, 0));
    assert!(fb.pixels.iter().all(|&p| p == BG));
}

#[test]
fn cambiar_el_shader_cambia_el_color_pero_no_la_geometria() {
    let sphere = uv_sphere(16, 32);
    let render = |kind| {
        let mut fb = Framebuffer::new(100, 100, BG).unwrap();
        let stats = draw_model(
            &mut fb,
            &sphere,
            &Transform::IDENTITY,
            &Material::solid(0.8, 0.6, 0.4),
            kind,
            &context(),
            &camera(),
        );
        (fb.pixels, stats)
    };
    let (flat, a) = render(ShaderKind::Flat);
    let (lit, b) = render(ShaderKind::LitPlanet);
    assert_eq!(a.triangles_rasterized, b.triangles_rasterized);
    assert_eq!(a.fragments_evaluated, b.fragments_evaluated);
    let covered = |px: &[u32]| px.iter().map(|&p| p != BG).collect::<Vec<_>>();
    assert_eq!(covered(&flat), covered(&lit));
    assert_ne!(flat, lit);
}

#[test]
fn los_atributos_se_interpolan_con_correccion_de_perspectiva() {
    // Borde con w = 1 (u = 0) y w = 3 (u = 1): en el punto medio de la pantalla,
    // u vale (0.5*0/1 + 0.5*1/3) / (0.5/1 + 0.5/3) = 0.25, no 0.5.
    let v = |x: f64, y: f64, w: f64, u: f64| RasterVertex {
        x,
        y,
        z: 0.0,
        inverse_w: 1.0 / w,
        world_position: Vec3::ZERO,
        normal: Vec3::Z,
        uv: Vec2::new(u, 0.0),
    };
    let mut fb = Framebuffer::new(64, 64, BG).unwrap();
    let mut seen = None;
    triangle_shaded(
        &mut fb,
        [
            v(0.0, 0.0, 1.0, 0.0),
            v(64.0, 0.0, 3.0, 1.0),
            v(0.0, 64.0, 1.0, 0.0),
        ],
        &mut |f| {
            if (f.screen_position.x - 32.5).abs() < 0.1 && (f.screen_position.y - 0.5).abs() < 0.1 {
                seen = Some(f.uv.x);
            }
            Vec3::ONE
        },
    );
    let u = seen.expect("el pixel central del borde debe ser un fragmento");
    assert!((u - 0.25).abs() < 0.01, "u = {u}");
}

#[test]
fn el_frustum_descarta_esferas_fuera_y_conserva_las_que_tocan_el_borde() {
    let f = Frustum::from_view_projection(&camera().view_projection());
    assert!(f.sphere_visible(Vec3::ZERO, 1.0));
    assert!(!f.sphere_visible(Vec3::new(0.0, 0.0, 30.0), 1.0)); // detras
    assert!(!f.sphere_visible(Vec3::new(500.0, 0.0, 0.0), 1.0)); // lateral
    assert!(!f.sphere_visible(Vec3::new(0.0, 0.0, -5000.0), 1.0)); // mas alla del plano lejano
    assert!(f.sphere_visible(Vec3::new(500.0, 0.0, 0.0), 600.0)); // radio grande la alcanza
}

#[test]
fn la_esfera_generada_apunta_hacia_afuera_y_tiene_normales_unitarias() {
    let s = uv_sphere(12, 24);
    for face in &s.faces {
        let [a, b, c] = face.vertex_indices.map(|i| Vec3::from_array(s.vertices[i]));
        let normal = (b - a).cross(c - a);
        assert!(normal.dot((a + b + c) / 3.0) > 0.0);
    }
    assert!(
        s.normals
            .iter()
            .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-12)
    );
}

#[test]
fn un_suelo_enorme_que_cruza_el_plano_cercano_se_rellena_sin_huecos() {
    // Triangulo de 2000 unidades bajo la camara: sin recorte bien hecho dejaria huecos
    // o coordenadas de pantalla absurdas; debe cubrir toda la mitad inferior de la vista.
    let floor = parse_obj("v -1000 0 -1000\nv 1000 0 -1000\nv 0 0 1000\nf 1 3 2\n").unwrap();
    let mut cam = Camera::new(60f64.to_radians(), 1.0, 0.3, 6000.0).unwrap();
    cam.position = Vec3::new(0.0, 2.0, 0.0);
    cam.target = Vec3::new(0.0, 0.0, -10.0);
    let mut fb = Framebuffer::new(200, 200, BG).unwrap();
    let stats = draw_model(
        &mut fb,
        &floor,
        &Transform::IDENTITY,
        &Material::solid(1.0, 1.0, 1.0),
        ShaderKind::Flat,
        &context(),
        &cam,
    );
    assert_eq!(stats.triangles_clipped, 1);
    let lower = fb.pixels[100 * 200..].iter().filter(|&&p| p != BG).count();
    assert_eq!(lower, 100 * 200);
    // El horizonte queda por encima del centro porque la camara mira hacia abajo.
    assert!(fb.pixels[..100 * 200].iter().any(|&p| p != BG));
}
