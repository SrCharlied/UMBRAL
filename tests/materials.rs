use umbral::assets::{obj::load_model, ship_model_path};

#[test]
fn carga_la_nave_real_y_asigna_los_colores_planos_de_su_mtl() {
    let model = load_model(&ship_model_path()).expect("se deben cargar el OBJ real y su MTL");
    assert_eq!(model.vertices.len(), 2153);
    assert_eq!(model.faces.len(), 4302);
    assert_eq!(model.materials.len(), 14);
    assert_eq!(model.material_libraries, ["Umbral_v4_Desgastada.mtl"]);
    assert_eq!(model.materials["01_Petroleo"], [0.048, 0.137, 0.126]);
    assert!(model.faces.iter().all(|face| {
        face.material
            .as_ref()
            .is_some_and(|name| model.materials.contains_key(name))
    }));
}
