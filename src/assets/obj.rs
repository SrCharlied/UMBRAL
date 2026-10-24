use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug)]
pub struct Face {
    pub vertex_indices: [usize; 3],
    /// Indices `vt` por esquina, si la cara los declara.
    pub uv_indices: Option<[usize; 3]>,
    /// Indices `vn` por esquina, si la cara los declara.
    pub normal_indices: Option<[usize; 3]>,
    pub material: Option<String>,
}

#[derive(Debug)]
pub struct Model {
    pub vertices: Vec<[f64; 3]>,
    pub uvs: Vec<[f64; 2]>,
    pub normals: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    pub material_libraries: Vec<String>,
    pub materials: BTreeMap<String, [f64; 3]>,
    /// Esfera envolvente en espacio de objeto (centro de la caja y radio maximo).
    pub bounds_center: [f64; 3],
    pub bounds_radius: f64,
}

impl Model {
    /// Calcula `bounds_center` y `bounds_radius` a partir de los vertices.
    pub fn update_bounds(&mut self) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for v in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(v[axis]);
                max[axis] = max[axis].max(v[axis]);
            }
        }
        if self.vertices.is_empty() {
            return;
        }
        let center = [0, 1, 2].map(|a| (min[a] + max[a]) / 2.0);
        self.bounds_center = center;
        self.bounds_radius = self
            .vertices
            .iter()
            .map(|v| {
                (0..3)
                    .map(|a| (v[a] - center[a]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(0.0, f64::max);
    }
}

pub fn parse_obj(source: &str) -> Result<Model, String> {
    let mut model = Model {
        vertices: Vec::new(),
        uvs: Vec::new(),
        normals: Vec::new(),
        faces: Vec::new(),
        material_libraries: Vec::new(),
        materials: BTreeMap::new(),
        bounds_center: [0.0; 3],
        bounds_radius: 0.0,
    };
    let mut current_material = None;
    for (line_index, line) in source.lines().enumerate() {
        let fields: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let error = |message: &str| format!("OBJ, linea {}: {message}", line_index + 1);
        match fields.first().copied() {
            Some("v") => {
                if fields.len() != 4 {
                    return Err(error("se esperaba v x y z"));
                }
                let mut vertex = [0.0; 3];
                for axis in 0..3 {
                    vertex[axis] = fields[axis + 1]
                        .parse::<f64>()
                        .map_err(|_| error("vertice invalido"))?;
                    if !vertex[axis].is_finite() {
                        return Err(error("el vertice debe ser finito"));
                    }
                }
                model.vertices.push(vertex);
            }
            Some("vt") => {
                if fields.len() < 3 || fields.len() > 4 {
                    return Err(error("se esperaba vt u v"));
                }
                let mut uv = [0.0; 2];
                for axis in 0..2 {
                    uv[axis] = fields[axis + 1]
                        .parse::<f64>()
                        .map_err(|_| error("coordenada UV invalida"))?;
                    if !uv[axis].is_finite() {
                        return Err(error("la coordenada UV debe ser finita"));
                    }
                }
                model.uvs.push(uv);
            }
            Some("vn") => {
                if fields.len() != 4 {
                    return Err(error("se esperaba vn x y z"));
                }
                let mut normal = [0.0; 3];
                for axis in 0..3 {
                    normal[axis] = fields[axis + 1]
                        .parse::<f64>()
                        .map_err(|_| error("normal invalida"))?;
                    if !normal[axis].is_finite() {
                        return Err(error("la normal debe ser finita"));
                    }
                }
                model.normals.push(normal);
            }
            Some("f") => {
                if fields.len() != 4 {
                    return Err(error("el modelo debe estar triangulado"));
                }
                let mut vertex_indices = [0; 3];
                let mut uv_indices = [0; 3];
                let mut normal_indices = [0; 3];
                let (mut has_uv, mut has_normal) = (0, 0);
                for (corner, token) in fields[1..].iter().enumerate() {
                    let mut parts = token.split('/');
                    let index =
                        |part: Option<&str>, count: usize| -> Result<Option<usize>, String> {
                            match part {
                                None | Some("") => Ok(None),
                                Some(text) => text
                                    .parse::<usize>()
                                    .ok()
                                    .and_then(|i| i.checked_sub(1))
                                    .filter(|&i| i < count)
                                    .map(Some)
                                    .ok_or_else(|| error("indice invalido o fuera de rango")),
                            }
                        };
                    vertex_indices[corner] = index(parts.next(), model.vertices.len())?
                        .ok_or_else(|| error("indice invalido o fuera de rango"))?;
                    if let Some(i) = index(parts.next(), model.uvs.len())? {
                        uv_indices[corner] = i;
                        has_uv += 1;
                    }
                    if let Some(i) = index(parts.next(), model.normals.len())? {
                        normal_indices[corner] = i;
                        has_normal += 1;
                    }
                    if parts.next().is_some() {
                        return Err(error("demasiados campos en el indice de la cara"));
                    }
                }
                if !matches!(has_uv, 0 | 3) || !matches!(has_normal, 0 | 3) {
                    return Err(error("la cara mezcla esquinas con y sin vt/vn"));
                }
                if vertex_indices[0] == vertex_indices[1]
                    || vertex_indices[1] == vertex_indices[2]
                    || vertex_indices[0] == vertex_indices[2]
                {
                    return Err(error("la cara repite indices"));
                }
                model.faces.push(Face {
                    vertex_indices,
                    uv_indices: (has_uv == 3).then_some(uv_indices),
                    normal_indices: (has_normal == 3).then_some(normal_indices),
                    material: current_material.clone(),
                });
            }
            Some("mtllib") => {
                if fields.len() != 2 {
                    return Err(error("se esperaba un nombre de archivo MTL sin espacios"));
                }
                model.material_libraries.push(fields[1].to_owned());
            }
            Some("usemtl") => {
                if fields.len() < 2 {
                    return Err(error("usemtl sin nombre"));
                }
                current_material = Some(fields[1..].join(" "));
            }
            _ => {}
        }
    }
    if model.vertices.is_empty() || model.faces.is_empty() {
        return Err("OBJ sin vertices o caras".into());
    }
    model.update_bounds();
    Ok(model)
}

pub fn load_model(path: &Path) -> Result<Model, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let mut model = parse_obj(&source)?;
    for filename in &model.material_libraries {
        let relative = Path::new(filename);
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err("El MTL debe estar dentro de la carpeta del modelo".into());
        }
        let material_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(relative);
        let source = std::fs::read_to_string(&material_path)
            .map_err(|e| format!("No se pudo leer {}: {e}", material_path.display()))?;
        model.materials.extend(parse_mtl(&source)?);
    }
    for face in &model.faces {
        if let Some(name) = &face.material
            && !model.materials.contains_key(name)
        {
            return Err(format!("Material no definido: {name}"));
        }
    }
    Ok(model)
}

fn parse_mtl(source: &str) -> Result<BTreeMap<String, [f64; 3]>, String> {
    let mut materials = BTreeMap::new();
    let mut current = None;
    for (line_index, line) in source.lines().enumerate() {
        let fields: Vec<_> = line
            .split('#')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let error = |message: &str| format!("MTL, linea {}: {message}", line_index + 1);
        match fields.first().copied() {
            Some("newmtl") => {
                if fields.len() < 2 {
                    return Err(error("newmtl sin nombre"));
                }
                let name = fields[1..].join(" ");
                materials.insert(name.clone(), [0.7; 3]);
                current = Some(name);
            }
            Some("Kd") => {
                if fields.len() != 4 {
                    return Err(error("se esperaba Kd r g b"));
                }
                let name = current.as_ref().ok_or_else(|| error("Kd sin newmtl"))?;
                let mut color = [0.0; 3];
                for axis in 0..3 {
                    color[axis] = fields[axis + 1]
                        .parse::<f64>()
                        .map_err(|_| error("color invalido"))?;
                    if !color[axis].is_finite() || !(0.0..=1.0).contains(&color[axis]) {
                        return Err(error("Kd debe estar entre 0 y 1"));
                    }
                }
                materials.insert(name.clone(), color);
            }
            _ => {}
        }
    }
    Ok(materials)
}
