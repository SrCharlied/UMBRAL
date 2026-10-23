use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug)]
pub struct Face {
    pub vertex_indices: [usize; 3],
    pub material: Option<String>,
}

#[derive(Debug)]
pub struct Model {
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Face>,
    pub material_libraries: Vec<String>,
    pub materials: BTreeMap<String, [f64; 3]>,
}

pub fn parse_obj(source: &str) -> Result<Model, String> {
    let mut model = Model {
        vertices: Vec::new(),
        faces: Vec::new(),
        material_libraries: Vec::new(),
        materials: BTreeMap::new(),
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
            Some("f") => {
                if fields.len() != 4 {
                    return Err(error("el modelo debe estar triangulado"));
                }
                let mut vertex_indices = [0; 3];
                for (corner, token) in fields[1..].iter().enumerate() {
                    let obj_index = token
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse::<usize>()
                        .map_err(|_| error("indice invalido"))?;
                    vertex_indices[corner] = obj_index
                        .checked_sub(1)
                        .filter(|&i| i < model.vertices.len())
                        .ok_or_else(|| error("indice fuera de rango"))?;
                }
                if vertex_indices[0] == vertex_indices[1]
                    || vertex_indices[1] == vertex_indices[2]
                    || vertex_indices[0] == vertex_indices[2]
                {
                    return Err(error("la cara repite indices"));
                }
                model.faces.push(Face {
                    vertex_indices,
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
