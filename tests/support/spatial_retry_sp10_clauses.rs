//! FIX01: guarda APPEND-ONLY que conserva las clausulas VALIOSAS de sp10 (sp10 original queda intacto y
//! RED por su plazo interno de alineacion antes de 60 grados, ver TEST_CONTRACT_DECISION.json):
//! inicializacion sin memoria = lado de la punta, giro real <= tope declarado (8 * paso / r), lado
//! perpendicular a la tangente, punta exacta y mismo estado -> misma memoria y misma malla. Mismo escenario
//! que sp10 (detras, lado +Y, la punta cruza a -Y en pasos de 0.5 grados).
use super::*;

#[test]
fn retry_sp10_valuable_clauses_initialization_rate_perpendicularity_and_same_state() {
    let (d, r) = rig();
    let cp = v3(r.rest_loop().center);
    // Inicializacion: sin memoria y fuera del eje, el lado es el de la punta y no hay giro.
    let tip = cp + Vec3::new(-0.3, 0.8, 0.2).normalized();
    let (mut v, mut n) = buffers(&d);
    let pose = ok(
        r.deform([tip.x, tip.y, tip.z], None, &mut v, &mut n),
        "sin memoria",
    );
    let expect = Vec3::new(0.0, 0.8, 0.2).normalized();
    assert!((v3(pose.memory.side) - expect).length() < 1e-12 && pose.side_rotation == 0.0);
    let mut memory = Some(mem([0.0, 1.0, 0.0], [cp.x - 1.0, cp.y, cp.z]));
    let mut previous = cp - Vec3::X;
    let mut turned = 0.0;
    for i in 1..=200 {
        let q = (180.0 + 0.5 * i as f64).to_radians();
        let tip = cp + Vec3::new(q.cos(), q.sin(), 0.0);
        let (mut v, mut n) = buffers(&d);
        let pose = ok(
            r.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
            "cruce",
        );
        let cap = SPATIAL_SIDE_RATE * (tip - previous).length() / (tip - cp).length();
        let before = v3(memory.unwrap().side);
        let real = before.dot(v3(pose.memory.side)).clamp(-1.0, 1.0).acos();
        assert!(pose.side_rotation.abs() <= cap + 1e-12, "{i}: declarado");
        assert!(real <= cap + 1e-9, "{i}: giro real {real} > tope {cap}");
        assert!(
            (real - pose.side_rotation.abs()).abs() < 1e-9,
            "{i}: declarado = real"
        );
        assert!(
            v3(pose.memory.side).dot(Vec3::X).abs() < 1e-12,
            "{i}: perpendicular a t"
        );
        assert!(
            (v3(pose.memory.side).length() - 1.0).abs() < 1e-12,
            "{i}: unitario"
        );
        assert!(
            (v3(pose.axial_tip) - tip).length() < 1e-9,
            "{i}: punta exacta"
        );
        // Mismo estado (misma punta, memoria publicada): misma memoria y misma malla, bit a bit.
        let (mut v2, mut n2) = buffers(&d);
        let again = ok(
            r.deform([tip.x, tip.y, tip.z], Some(pose.memory), &mut v2, &mut n2),
            "mismo estado",
        );
        assert_eq!(
            again.memory, pose.memory,
            "{i}: memoria con el mismo estado"
        );
        assert_eq!((v2, n2), (v, n), "{i}: malla con el mismo estado");
        turned += real;
        memory = Some(pose.memory);
        previous = tip;
    }
    // La memoria SI se mueve hacia el lado de la punta (no se congela), sin exigir plazo.
    assert!(turned > 0.0, "la memoria gira en el cruce");
    println!("giro acumulado de la memoria en el cruce: {turned} rad");
}
