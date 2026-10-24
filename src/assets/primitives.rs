//! Mallas generadas por codigo (sin archivo OBJ).

use super::obj::{Face, Model};
use std::{collections::BTreeMap, f64::consts::PI};

/// Esfera UV de radio 1 con normales y UV. `rings` divisiones en latitud y `segments`
/// en longitud. El polo norte es +Y; `u` crece con la longitud y `v` baja del polo norte.
pub fn uv_sphere(rings: usize, segments: usize) -> Model {
    let (rings, segments) = (rings.max(2), segments.max(3));
    let mut model = Model {
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        faces: Vec::new(),
        material_libraries: Vec::new(),
        materials: BTreeMap::new(),
        bounds_center: [0.0; 3],
        bounds_radius: 1.0,
    };
    for ring in 0..=rings {
        let v = ring as f64 / rings as f64;
        let (sin_t, cos_t) = (v * PI).sin_cos();
        for seg in 0..=segments {
            let u = seg as f64 / segments as f64;
            let (sin_p, cos_p) = (u * 2.0 * PI).sin_cos();
            let p = [sin_t * cos_p, cos_t, sin_t * sin_p];
            model.vertices.push(p);
            model.normals.push(p);
            model.uvs.push([u, v]);
        }
    }
    let stride = segments + 1;
    let mut push = |a: usize, b: usize, c: usize| {
        model.faces.push(Face {
            vertex_indices: [a, b, c],
            uv_indices: Some([a, b, c]),
            normal_indices: Some([a, b, c]),
            material: None,
        });
    };
    for ring in 0..rings {
        for seg in 0..segments {
            let a = ring * stride + seg;
            let (b, c, d) = (a + 1, a + stride, a + stride + 1);
            // Antihorario visto desde fuera.
            if ring != 0 {
                push(a, b, c);
            }
            if ring != rings - 1 {
                push(b, d, c);
            }
        }
    }
    model
}
