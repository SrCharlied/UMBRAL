//! La rasterizacion en paralelo de `draw_model` es identica bit a bit a la secuencial de
//! `draw_model_with` (mismo shader), en color, profundidad efectiva y contadores.

use umbral::{
    assets::{obj::load_model, primitives::uv_sphere, ship_model_path},
    math::{Transform, Vec3},
    renderer::{
        framebuffer::Framebuffer,
        pipeline::{PARALLEL_MIN_BOX_PIXELS, draw_model, draw_model_with, raster_threads},
        shader::{Material, ShaderContext, ShaderKind, shade_fragment},
    },
    scene::{camera::Camera, world::SPACE_COLOR},
};

fn camera() -> Camera {
    let mut camera = Camera::new(1.0, 16.0 / 9.0, 0.3, 6000.0).unwrap();
    camera.position = Vec3::new(0.0, 0.0, 6.0);
    camera.target = Vec3::ZERO;
    camera
}

fn context() -> ShaderContext {
    ShaderContext {
        elapsed_time: 3.25,
        oxygen01: 0.3,
        camera_position: Vec3::new(0.0, 0.0, 6.0),
        ship_position: Vec3::new(0.0, -0.5, 4.0),
        light_position: Vec3::new(-40.0, 20.0, 30.0),
    }
}

#[test]
fn draw_model_paralelo_es_identico_al_secuencial() {
    assert!(raster_threads() >= 1);
    let camera = camera();
    let context = context();
    let sphere = uv_sphere(32, 64);
    let ship = load_model(&ship_model_path()).unwrap();
    // Varias capas solapadas: esfera grande (supera el umbral), otra que la corta y la nave.
    let scenes = [
        (
            &sphere,
            Transform {
                position: Vec3::ZERO,
                rotation: Vec3::new(0.2, 0.7, 0.0),
                scale: Vec3::splat(2.4),
            },
            ShaderKind::LitPlanet,
            Material::solid(0.5, 0.45, 0.4),
        ),
        (
            &sphere,
            Transform {
                position: Vec3::new(1.2, 0.4, 1.0),
                rotation: Vec3::ZERO,
                scale: Vec3::splat(1.3),
            },
            ShaderKind::AnimatedSun,
            Material::solid(1.0, 0.45, 0.08),
        ),
        (
            &sphere,
            Transform {
                position: Vec3::new(-1.0, -0.2, 2.0),
                rotation: Vec3::new(0.0, 1.1, 0.0),
                scale: Vec3::splat(1.1),
            },
            ShaderKind::LucidSurface,
            Material::solid(0.45, 0.40, 0.36),
        ),
        (
            &ship,
            Transform {
                position: Vec3::new(0.0, -0.6, 3.6),
                rotation: Vec3::new(0.1, 3.0, 0.0),
                scale: Vec3::splat(0.6),
            },
            ShaderKind::LitPlanet,
            Material {
                cull_backfaces: true,
                ..Material::from_model()
            },
        ),
    ];
    let mut parallel = Framebuffer::new(1280, 720, SPACE_COLOR).unwrap();
    let mut sequential = Framebuffer::new(1280, 720, SPACE_COLOR).unwrap();
    let mut big = false;
    for (model, transform, shader, material) in scenes {
        let a = draw_model(
            &mut parallel,
            model,
            &transform,
            &material,
            shader,
            &context,
            &camera,
        );
        let b = draw_model_with(
            &mut sequential,
            model,
            &transform,
            &material,
            &camera,
            |f, m| shade_fragment(shader, f, &context, m),
        );
        assert_eq!(a, b, "contadores de {shader:?}");
        big |= a.fragments_evaluated > PARALLEL_MIN_BOX_PIXELS;
        assert!(
            parallel.pixels == sequential.pixels,
            "pixeles tras {shader:?}"
        );
    }
    assert!(big, "al menos un dibujo debe tomar la ruta paralela");
    // Una esfera de sondeo encima: el z-test sobre ambas profundidades da el mismo cuadro.
    let probe = Transform {
        position: Vec3::new(0.0, 0.0, 0.5),
        rotation: Vec3::ZERO,
        scale: Vec3::splat(3.0),
    };
    draw_model(
        &mut parallel,
        &sphere,
        &probe,
        &Material::solid(1.0, 1.0, 1.0),
        ShaderKind::Flat,
        &context,
        &camera,
    );
    draw_model_with(
        &mut sequential,
        &sphere,
        &probe,
        &Material::solid(1.0, 1.0, 1.0),
        &camera,
        |f, m| shade_fragment(ShaderKind::Flat, f, &context, m),
    );
    assert!(parallel.pixels == sequential.pixels);
}
