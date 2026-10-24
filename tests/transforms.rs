use std::f64::consts::{FRAC_PI_2, PI};
use umbral::{
    math::{Mat4, Transform, Vec3},
    scene::camera::Camera,
};

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).length() < 1e-9
}

#[test]
fn transform_aplica_escala_rotacion_y_traslacion_en_ese_orden() {
    let t = Transform {
        position: Vec3::new(10.0, 0.0, 0.0),
        rotation: Vec3::new(0.0, FRAC_PI_2, 0.0),
        scale: Vec3::new(2.0, 1.0, 1.0),
    };
    // (1,0,0) -> escala (2,0,0) -> gira 90 grados en Y (x pasa a -z) -> traslada.
    let p = t.matrix().transform_point(Vec3::X);
    assert!(close(p, Vec3::new(10.0, 0.0, -2.0)), "{p:?}");
}

#[test]
fn euler_ida_y_vuelta() {
    for e in [
        Vec3::new(0.3, -1.2, 2.0),
        Vec3::new(-1.0, 3.0, -0.5),
        Vec3::new(FRAC_PI_2, 0.7, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
    ] {
        let m = Mat4::from_euler(e);
        let back = Mat4::from_euler(m.to_euler());
        for r in 0..3 {
            for c in 0..3 {
                assert!((m.m[r][c] - back.m[r][c]).abs() < 1e-9, "{e:?}");
            }
        }
    }
}

#[test]
fn la_camara_mira_hacia_menos_z_en_el_espacio_de_vista() {
    let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
    assert!(close(
        view.transform_point(Vec3::ZERO),
        Vec3::new(0.0, 0.0, -5.0)
    ));
    // Vista con el objetivo en vertical: no debe producir NaN.
    let up = Mat4::look_at(Vec3::ZERO, Vec3::Y, Vec3::Y);
    assert!(up.transform_point(Vec3::X).is_finite());
}

#[test]
fn la_perspectiva_mapea_cercano_y_lejano_a_menos_uno_y_uno() {
    let p = Mat4::perspective(PI / 2.0, 1.0, 1.0, 100.0).unwrap();
    let ndc = |z: f64| {
        let c = p.transform(Vec3::new(0.0, 0.0, z).extend(1.0));
        c.z / c.w
    };
    assert!((ndc(-1.0) + 1.0).abs() < 1e-12);
    assert!((ndc(-100.0) - 1.0).abs() < 1e-12);
    // Con 90 grados de campo, (x = -z) cae en el borde derecho.
    let c = p.transform(Vec3::new(5.0, 0.0, -5.0).extend(1.0));
    assert!((c.x / c.w - 1.0).abs() < 1e-12);
    // Detras de la camara w es negativo.
    assert!(p.transform(Vec3::new(0.0, 0.0, 2.0).extend(1.0)).w < 0.0);
}

#[test]
fn la_perspectiva_rechaza_parametros_invalidos() {
    for (fov, aspect, near, far) in [
        (0.0, 1.0, 1.0, 10.0),
        (PI, 1.0, 1.0, 10.0),
        (1.0, 0.0, 1.0, 10.0),
        (1.0, 1.0, 0.0, 10.0),
        (1.0, 1.0, 5.0, 5.0),
        (f64::NAN, 1.0, 1.0, 10.0),
    ] {
        assert!(Mat4::perspective(fov, aspect, near, far).is_err());
    }
    assert!(Camera::new(1.0, 1.0, -1.0, 10.0).is_err());
}

#[test]
fn mover_la_camara_no_modifica_la_matriz_del_objeto() {
    let t = Transform::at(Vec3::new(1.0, 2.0, 3.0));
    let before = t.matrix();
    let mut camera = Camera::new(1.0, 1.5, 0.1, 100.0).unwrap();
    let vp = camera.view_projection();
    camera.position = Vec3::new(50.0, 0.0, 0.0);
    assert_eq!(before, t.matrix());
    assert_ne!(vp, camera.view_projection());
}

#[test]
fn la_normal_usa_la_inversa_transpuesta_con_escala_no_uniforme() {
    let t = Transform {
        scale: Vec3::new(4.0, 1.0, 1.0),
        ..Transform::IDENTITY
    };
    // Plano a 45 grados: al estirar X su normal se inclina hacia Y.
    let n = t.transform_normal(Vec3::new(1.0, 1.0, 0.0).normalized());
    assert!(n.y > n.x && (n.length() - 1.0).abs() < 1e-12);
}
