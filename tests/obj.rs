use umbral::assets::obj::parse_obj;

#[test]
fn recorre_los_indices_de_una_cara_para_obtener_sus_tres_vertices() {
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 1\nf 3//1 1//1 2//1\n";
    let model = parse_obj(source).expect("el OBJ triangulado debe cargarse");
    assert_eq!(model.vertices.len(), 3);
    assert_eq!(model.faces.len(), 1);
    assert_eq!(model.faces[0].vertex_indices, [2, 0, 1]);
    let [a, b, c] = model.faces[0].vertex_indices;
    assert_eq!(model.vertices[a], [0.0, 1.0, 0.0]);
    assert_eq!(model.vertices[b], [0.0, 0.0, 0.0]);
    assert_eq!(model.vertices[c], [1.0, 0.0, 0.0]);
}

#[test]
fn rechaza_datos_obj_que_no_pueden_formar_caras_validas() {
    let vertices = "v 0 0 0\nv 1 0 0\nv 0 1 0\n";
    let cases = [
        String::new(),
        "v NaN 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3".into(),
        "v inf 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3".into(),
        format!("{vertices}f 0 2 3"),
        format!("{vertices}f 1 2 4"),
        format!("{vertices}f 1 2 3 1"),
        format!("{vertices}f 1 1 2"),
    ];
    for source in cases {
        assert!(
            parse_obj(&source).is_err(),
            "se acepto OBJ invalido: {source:?}"
        );
    }
}

#[test]
fn lee_uv_y_normales_por_esquina() {
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nvn 0 0 1\nvn 0 1 0\nf 1/1/2 2/2/1 3/3/1\n";
    let model = parse_obj(source).unwrap();
    assert_eq!(model.uvs, vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(model.normals.len(), 2);
    let face = &model.faces[0];
    assert_eq!(face.uv_indices, Some([0, 1, 2]));
    assert_eq!(face.normal_indices, Some([1, 0, 0]));
}

#[test]
fn la_nave_real_conserva_normales_y_calcula_su_esfera_envolvente() {
    let model = umbral::assets::obj::load_model(&umbral::assets::ship_model_path()).unwrap();
    assert!(!model.normals.is_empty());
    assert!(model.faces.iter().all(|f| f.normal_indices.is_some()));
    assert!(model.bounds_radius > 4.0 && model.bounds_radius < 10.0);
}

#[test]
fn valida_rangos_de_uv_y_normales_y_esquinas_mixtas() {
    let v = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\n";
    for face in [
        "f 1/2 2/1 3/1",
        "f 1//2 2//1 3//1",
        "f 1/1 2 3",
        "f 1/1/1/1 2/1/1 3/1/1",
        "f 1/0 2/1 3/1",
    ] {
        assert!(parse_obj(&format!("{v}{face}\n")).is_err(), "{face}");
    }
}
