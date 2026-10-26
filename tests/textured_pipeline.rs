use std::collections::BTreeMap;
use umbral::{
    assets::{
        obj::{Face, Model, load_model},
        primitives::uv_sphere,
        ship_model_path,
    },
    math::{Transform, Vec2, Vec3},
    renderer::{
        framebuffer::Framebuffer,
        pipeline::{draw_model, draw_model_with},
        shader::{FragmentInput, Material, ShaderContext, ShaderKind, shade_fragment, to_rgb8},
        texture::Texture,
    },
    scene::camera::Camera,
    visual::textured::{INVALID_UV_COLOR, TexturedMode, TexturedShader},
};

fn context(camera: &Camera) -> ShaderContext {
    ShaderContext {
        elapsed_time: 1.3,
        oxygen01: 0.7,
        camera_position: camera.position,
        ship_position: Vec3::ZERO,
        light_position: Vec3::new(4.0, 6.0, 5.0),
    }
}

fn camera(w: usize, h: usize, position: Vec3, target: Vec3) -> Camera {
    let mut camera = Camera::new(60f64.to_radians(), w as f64 / h as f64, 0.1, 100.0).unwrap();
    camera.position = position;
    camera.target = target;
    camera
}

#[test]
fn el_envoltorio_clasico_y_el_callback_dan_los_mismos_pixeles_y_contadores() {
    let (w, h) = (160, 120);
    let ship = load_model(&ship_model_path()).unwrap();
    let sphere = uv_sphere(16, 32);
    let cam = camera(w, h, Vec3::new(0.5, 2.0, 7.0), Vec3::ZERO);
    let ctx = context(&cam);
    // Dos objetos superpuestos: la esfera atraviesa la nave, asi interviene el z-test.
    let scene: [(&Model, Transform); 2] = [
        (&ship, Transform::IDENTITY),
        (
            &sphere,
            Transform {
                position: Vec3::new(0.8, 0.3, 0.5),
                scale: Vec3::splat(1.6),
                ..Transform::IDENTITY
            },
        ),
    ];
    let materials = [
        Material::from_model(),
        Material {
            cull_backfaces: true,
            ..Material::solid(0.3, 0.5, 0.9)
        },
    ];
    for kind in [
        ShaderKind::Flat,
        ShaderKind::LitPlanet,
        ShaderKind::AnimatedSun,
    ] {
        for material in &materials {
            let mut classic = Framebuffer::new(w, h, 0x000000).unwrap();
            let mut extended = Framebuffer::new(w, h, 0x000000).unwrap();
            for (model, transform) in &scene {
                let a = draw_model(&mut classic, model, transform, material, kind, &ctx, &cam);
                let b = draw_model_with(&mut extended, model, transform, material, &cam, |f, m| {
                    shade_fragment(kind, f, &ctx, m)
                });
                assert_eq!(a, b, "contadores {kind:?}");
                assert!(a.pixels_written > 0, "la escena dibuja algo con {kind:?}");
            }
            assert!(
                classic.pixels == extended.pixels,
                "pixeles {kind:?} {material:?}"
            );
        }
    }
}

#[test]
fn el_callback_recibe_uv_con_correccion_de_perspectiva() {
    // Suelo y = -1 de x en [-4, 4] y z en [-2, -30]; la camara en el origen mira a -Z,
    // asi la vista es la identidad y la UV esperada sale de intersectar el rayo del pixel.
    let (w, h) = (160, 120);
    let floor = Model {
        vertices: vec![
            [-4.0, -1.0, -2.0],
            [4.0, -1.0, -2.0],
            [4.0, -1.0, -30.0],
            [-4.0, -1.0, -30.0],
        ],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        normals: Vec::new(),
        faces: [[0, 1, 2], [0, 2, 3]]
            .into_iter()
            .map(|i| Face {
                vertex_indices: i,
                uv_indices: Some(i),
                normal_indices: None,
                material: None,
            })
            .collect(),
        material_libraries: Vec::new(),
        materials: BTreeMap::new(),
        bounds_center: [0.0, -1.0, -16.0],
        bounds_radius: 15.0,
    };
    let cam = camera(w, h, Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    let tan = (cam.fov_y / 2.0).tan();
    let mut fragments: Vec<FragmentInput> = Vec::new();
    let mut fb = Framebuffer::new(w, h, 0).unwrap();
    let stats = draw_model_with(
        &mut fb,
        &floor,
        &Transform::IDENTITY,
        &Material::solid(1.0, 1.0, 1.0),
        &cam,
        |f, _| {
            fragments.push(*f);
            Vec3::ONE
        },
    );
    assert_eq!(stats.fragments_evaluated, fragments.len());
    assert!(fragments.len() > 2000, "{}", fragments.len());
    let (mut v_min, mut v_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for f in &fragments {
        let ndc_x = f.screen_position.x / w as f64 * 2.0 - 1.0;
        let ndc_y = 1.0 - f.screen_position.y / h as f64 * 2.0;
        let dir = Vec3::new(ndc_x * tan * cam.aspect, ndc_y * tan, -1.0);
        let hit = dir * (-1.0 / dir.y);
        let (u, v) = ((hit.x + 4.0) / 8.0, (-hit.z - 2.0) / 28.0);
        assert!(
            (f.uv.x - u).abs() < 1e-9 && (f.uv.y - v).abs() < 1e-9,
            "{:?} vs ({u}, {v})",
            f.uv
        );
        (v_min, v_max) = (v_min.min(f.uv.y), v_max.max(f.uv.y));
    }
    assert!(
        v_min < 0.05 && v_max > 0.5,
        "rango de profundidad cubierto: {v_min}..{v_max}"
    );
}

fn fragment(uv: Vec2, normal: Vec3) -> FragmentInput {
    FragmentInput {
        screen_position: Vec3::ZERO,
        world_position: Vec3::ZERO,
        normal,
        uv,
    }
}

// Fixture sintetico 2x2 (filas PNG de arriba hacia abajo): rojo, verde / azul, blanco.
fn checker() -> Texture {
    Texture::from_rgb8(2, 2, vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]).unwrap()
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-12
}

#[test]
fn sin_iluminacion_devuelve_el_albedo_muestreado() {
    let tex = checker();
    let shader =
        TexturedShader::new(&tex, TexturedMode::Unlit, Vec3::new(0.0, 10.0, 0.0), 0.2).unwrap();
    for uv in [
        Vec2::new(0.25, 0.75),
        Vec2::new(0.6, 0.1),
        Vec2::new(1.0, 0.0),
    ] {
        // La normal no influye en el modo sin iluminacion.
        for n in [Vec3::Y, -Vec3::Y, Vec3::ZERO] {
            assert!(
                close(shader.shade(&fragment(uv, n)), tex.sample(uv).unwrap()),
                "{uv:?}"
            );
        }
    }
}

#[test]
fn iluminado_es_difuso_mas_ambiente_con_la_normal_interpolada() {
    let tex = Texture::from_rgb8(1, 1, vec![255, 128, 0]).unwrap();
    let albedo = tex.sample(Vec2::new(0.5, 0.5)).unwrap();
    let ambient = 0.2;
    let at = |light: Vec3, normal: Vec3| {
        TexturedShader::new(&tex, TexturedMode::Lit, light, ambient)
            .unwrap()
            .shade(&fragment(Vec2::new(0.5, 0.5), normal))
    };
    assert!(
        close(at(Vec3::new(0.0, 10.0, 0.0), Vec3::Y), albedo),
        "luz de frente"
    );
    assert!(
        close(at(Vec3::new(10.0, 0.0, 0.0), Vec3::Y), albedo * ambient),
        "rasante"
    );
    assert!(
        close(at(Vec3::new(0.0, -10.0, 0.0), Vec3::Y), albedo * ambient),
        "por detras"
    );
    let diagonal = albedo * (ambient + (1.0 - ambient) * std::f64::consts::FRAC_1_SQRT_2);
    assert!(
        close(at(Vec3::new(5.0, 5.0, 0.0), Vec3::Y), diagonal),
        "45 grados"
    );
    // La normal interpolada puede no ser unitaria: se renormaliza.
    assert!(close(at(Vec3::new(5.0, 5.0, 0.0), Vec3::Y * 3.0), diagonal));
}

#[test]
fn uv_nan_da_el_color_de_diagnostico_en_ambos_modos() {
    let tex = checker();
    for mode in [TexturedMode::Unlit, TexturedMode::Lit] {
        let shader = TexturedShader::new(&tex, mode, Vec3::new(0.0, 10.0, 0.0), 0.2).unwrap();
        let color = shader.shade(&fragment(Vec2::new(f64::NAN, 0.5), Vec3::Y));
        assert_eq!(color, INVALID_UV_COLOR, "{mode:?}");
    }
}

#[test]
fn rechaza_ambiente_o_luz_invalidos() {
    let tex = checker();
    for (light, ambient) in [
        (Vec3::Y, f64::NAN),
        (Vec3::Y, -0.1),
        (Vec3::Y, 1.1),
        (Vec3::new(f64::NAN, 0.0, 0.0), 0.2),
        (Vec3::new(0.0, f64::INFINITY, 0.0), 0.2),
    ] {
        assert!(
            TexturedShader::new(&tex, TexturedMode::Lit, light, ambient).is_err(),
            "{light:?} {ambient}"
        );
    }
}

/// Cuadrado de lado 2 en z = -3 frente a una camara en el origen; UV (0, 0) abajo a la izquierda.
fn quad() -> Model {
    Model {
        vertices: vec![
            [-1.0, -1.0, -3.0],
            [1.0, -1.0, -3.0],
            [1.0, 1.0, -3.0],
            [-1.0, 1.0, -3.0],
        ],
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        normals: Vec::new(),
        faces: [[0, 1, 2], [0, 2, 3]]
            .into_iter()
            .map(|i| Face {
                vertex_indices: i,
                uv_indices: Some(i),
                normal_indices: None,
                material: None,
            })
            .collect(),
        material_libraries: Vec::new(),
        materials: BTreeMap::new(),
        bounds_center: [0.0, 0.0, -3.0],
        bounds_radius: 1.5,
    }
}

fn render_quad(tex: &Texture, material: &Material) -> (Framebuffer, usize, usize) {
    let (w, h) = (120, 120);
    let cam = camera(w, h, Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    let shader = TexturedShader::new(tex, TexturedMode::Unlit, Vec3::ZERO, 0.2).unwrap();
    let mut fb = Framebuffer::new(w, h, 0x000000).unwrap();
    draw_model_with(
        &mut fb,
        &quad(),
        &Transform::IDENTITY,
        material,
        &cam,
        |f, _| shader.shade(f),
    );
    (fb, w, h)
}

#[test]
fn en_pantalla_la_fila_superior_del_png_queda_arriba() {
    let (fb, w, h) = render_quad(&checker(), &Material::solid(1.0, 1.0, 1.0));
    // Centros de los cuadrantes del cuadrado (proyectado sobre el centro de la imagen).
    let px = |x: usize, y: usize| fb.pixels[y * w + x];
    let dominant = |c: u32| {
        let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
        (r > 200, g > 200, b > 200)
    };
    let (q1, q3) = (w / 2 - w / 10, w / 2 + w / 10);
    let (t1, t3) = (h / 2 - h / 10, h / 2 + h / 10);
    assert_eq!(
        dominant(px(q1, t1)),
        (true, false, false),
        "arriba izquierda: rojo"
    );
    assert_eq!(
        dominant(px(q3, t1)),
        (false, true, false),
        "arriba derecha: verde"
    );
    assert_eq!(
        dominant(px(q1, t3)),
        (false, false, true),
        "abajo izquierda: azul"
    );
    assert_eq!(
        dominant(px(q3, t3)),
        (true, true, true),
        "abajo derecha: blanco"
    );
}

#[test]
fn el_kd_por_defecto_del_material_no_oscurece_la_textura() {
    // Material::from_model usa Kd 0.7 por defecto; el shader texturizado lo ignora.
    let white = Texture::from_rgb8(1, 1, vec![255, 255, 255]).unwrap();
    let (fb, _, _) = render_quad(&white, &Material::from_model());
    let drawn: Vec<_> = fb.pixels.iter().filter(|&&p| p != 0).collect();
    assert!(drawn.len() > 1000);
    assert!(
        drawn.iter().all(|&&p| p == to_rgb8(Vec3::ONE)),
        "todo blanco puro"
    );
}
