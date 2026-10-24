//! `draw_model`: etapa de vertices (modelo-vista-proyeccion), recorte, viewport y
//! rasterizacion con shader por fragmento (UMB-101/102/104/105). Propietario: Persona 1.

use super::{
    clipping::{NEAR_BIT, VertexOut, clip_near, outcode},
    framebuffer::Framebuffer,
    rasterizer::{RasterVertex, triangle_shaded},
    shader::{Material, ShaderContext, ShaderKind, shade_fragment},
    stats::RenderStats,
};
use crate::{
    assets::obj::Model,
    math::{Transform, Vec2, Vec3},
    scene::camera::Camera,
};

/// Dibuja `model` en el framebuffer sin limpiarlo ni autoencuadrarlo. La posicion final
/// sale de `proyeccion * vista * transform`; la profundidad usa el z-buffer del framebuffer.
pub fn draw_model(
    framebuffer: &mut Framebuffer,
    model: &Model,
    transform: &Transform,
    material: &Material,
    shader: ShaderKind,
    context: &ShaderContext,
    camera: &Camera,
) -> RenderStats {
    let mut stats = RenderStats {
        objects_submitted: 1,
        vertices_processed: model.vertices.len(),
        ..RenderStats::default()
    };
    let model_matrix = transform.matrix();
    let mvp = camera.view_projection() * model_matrix;
    // Etapa de vertices una sola vez por vertice, no por esquina de cara.
    let clip: Vec<_> = model
        .vertices
        .iter()
        .map(|&v| mvp.transform(Vec3::from_array(v).extend(1.0)))
        .collect();
    let world: Vec<_> = model
        .vertices
        .iter()
        .map(|&v| model_matrix.transform_point(Vec3::from_array(v)))
        .collect();
    let normals: Vec<_> = model
        .normals
        .iter()
        .map(|&n| transform.transform_normal(Vec3::from_array(n)))
        .collect();
    let (width, height) = (framebuffer.width as f64, framebuffer.height as f64);

    for face in &model.faces {
        stats.triangles_submitted += 1;
        let [i0, i1, i2] = face.vertex_indices;
        let codes = [outcode(clip[i0]), outcode(clip[i1]), outcode(clip[i2])];
        if codes[0] & codes[1] & codes[2] != 0 {
            stats.triangles_culled += 1;
            continue;
        }
        let flat_normal = || {
            (world[i1] - world[i0])
                .cross(world[i2] - world[i0])
                .normalized()
        };
        let flat = flat_normal();
        let corner = |slot: usize, vertex: usize| VertexOut {
            clip_position: clip[vertex],
            world_position: world[vertex],
            world_normal: face.normal_indices.map_or(flat, |n| normals[n[slot]]),
            uv: face.uv_indices.map_or(Vec2::default(), |t| {
                let uv = model.uvs[t[slot]];
                Vec2::new(uv[0], uv[1])
            }),
        };
        let tri = [corner(0, i0), corner(1, i1), corner(2, i2)];
        let polygon = if (codes[0] | codes[1] | codes[2]) & NEAR_BIT != 0 {
            stats.triangles_clipped += 1;
            clip_near(tri)
        } else {
            tri.to_vec()
        };
        if polygon.len() < 3 {
            stats.triangles_culled += 1;
            continue;
        }

        let face_material = Material {
            base_color: match (&face.material, material.use_model_colors) {
                (Some(name), true) => model
                    .materials
                    .get(name)
                    .map_or(material.base_color, |&kd| Vec3::from_array(kd)),
                _ => material.base_color,
            },
            ..*material
        };
        let screen: Vec<RasterVertex> = polygon
            .iter()
            .map(|v| {
                let c = v.clip_position;
                let inverse_w = 1.0 / c.w;
                RasterVertex {
                    x: (c.x * inverse_w * 0.5 + 0.5) * width,
                    y: (0.5 - c.y * inverse_w * 0.5) * height,
                    z: c.z * inverse_w,
                    inverse_w,
                    world_position: v.world_position,
                    normal: v.world_normal,
                    uv: v.uv,
                }
            })
            .collect();
        if material.cull_backfaces {
            // En pantalla (Y hacia abajo) un triangulo antihorario de frente tiene area negativa.
            let (a, b, c) = (screen[0], screen[1], screen[2]);
            let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if area >= 0.0 {
                stats.triangles_backface += 1;
                continue;
            }
        }
        let mut shade = |fragment: &_| shade_fragment(shader, fragment, context, &face_material);
        // Abanico: un poligono de n vertices produce n - 2 triangulos.
        for k in 1..screen.len() - 1 {
            stats.triangles_rasterized += 1;
            let (fragments, written) = triangle_shaded(
                framebuffer,
                [screen[0], screen[k], screen[k + 1]],
                &mut shade,
            );
            stats.fragments_evaluated += fragments;
            stats.pixels_written += written;
        }
    }
    stats
}
