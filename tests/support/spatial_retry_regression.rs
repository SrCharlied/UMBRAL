//! FIX01 (reintento acotado autorizado): regresion rapida APPEND-ONLY con el brazo REAL. Reutiliza la
//! historia y el pico del harness del padre (60 -> 216 grados a 0.05 en el plano (t, t x Y), r = bola minima,
//! refinado 216 -> 223) y conserva variantes adversarias de radio, memoria inicial e historia/plano.
//! Criterio EXACTO del contrato: salto maximo de TODOS los vertices <= 8 * paso de la punta + 1e-5; punta
//! exacta; finitud de cada componente validada ANTES de reducir maximos. No sustituye a la prueba real
//! original (oraculo final) ni a sp10.
use super::*;

/// Salto maximo entre dos mallas; falla si algun componente no es finito.
fn retry_max_jump(a: &[[f64; 3]], b: &[[f64; 3]], what: &str) -> f64 {
    assert_eq!(a.len(), b.len(), "{what}: topologia");
    let mut worst = 0.0_f64;
    for (i, (p, q)) in a.iter().zip(b).enumerate() {
        assert!(
            p.iter().chain(q).all(|c| c.is_finite()),
            "{what}: componente no finito en el vertice {i}"
        );
        let d = (Vec3::from_array(*p) - Vec3::from_array(*q)).length();
        assert!(d.is_finite(), "{what}: distancia no finita en {i}");
        worst = worst.max(d);
    }
    worst
}

/// Estado congelado de una historia: memoria publicada, malla y punta.
type RetryState = (SpatialMemory, Vec<[f64; 3]>, Vec3);

/// Un caso: barrido en el plano (t, w) desde `from` a `to` grados con `per_degree` muestras por grado,
/// partiendo de `start` (memoria y malla de una historia previa) o sin memoria.
struct RetryCase {
    name: String,
    frames: usize,
    violations: Vec<String>,
    worst_gain: f64,
}

#[allow(clippy::too_many_arguments)]
fn retry_sweep(
    rig: &SpatialRig,
    template: &Model,
    name: String,
    plane: Vec3,
    radius: f64,
    from: f64,
    to: f64,
    per_degree: usize,
    start: Option<RetryState>,
) -> (RetryCase, Option<RetryState>) {
    let rest = rig.rest_loop();
    let (cp, t) = (
        Vec3::from_array(rest.center),
        Vec3::from_array(rest.tangent),
    );
    let point = |degrees: f64| {
        let a = degrees.to_radians();
        cp + (t * a.cos() + plane * a.sin()) * radius
    };
    let mut case = RetryCase {
        name,
        frames: 0,
        violations: Vec::new(),
        worst_gain: 0.0,
    };
    let (mut memory, mut previous) = match start {
        Some((m, v, tip)) => (Some(m), Some((v, tip))),
        None => (None, None),
    };
    let samples = ((to - from) * per_degree as f64).round() as usize;
    let first = if previous.is_some() { 1 } else { 0 };
    for i in first..=samples {
        let degrees = from + i as f64 / per_degree as f64;
        let tip = point(degrees);
        let (mut v, mut n) = (template.vertices.clone(), template.normals.clone());
        let pose = ok(
            rig.deform([tip.x, tip.y, tip.z], memory, &mut v, &mut n),
            &case.name,
        );
        assert!(
            pose.axial_tip.iter().all(|c| c.is_finite())
                && (Vec3::from_array(pose.axial_tip) - tip).length() < 1e-9,
            "{} {degrees}: punta exacta",
            case.name
        );
        if let Some((pv, pt)) = &previous {
            let step = (tip - *pt).length();
            assert!(
                step.is_finite() && step > 0.0,
                "{} {degrees}: paso",
                case.name
            );
            let jump = retry_max_jump(pv, &v, &case.name);
            case.worst_gain = case.worst_gain.max(jump / step);
            if jump > 8.0 * step + 1e-5 {
                case.violations
                    .push(format!("{degrees}: salto {jump} paso {step}"));
            }
        }
        case.frames += 1;
        memory = Some(pose.memory);
        previous = Some((v, tip));
    }
    let end = previous.map(|(v, tip)| (memory.unwrap(), v, tip));
    (case, end)
}

#[test]
#[ignore = "assets privados: UMBRAL_CAPTURE_PACKAGE_DIR, UMBRAL_GROWTH_PACKAGE, UMBRAL_ARM_CORRESPONDENCE, UMBRAL_CAPTURE_CREATURE_OBJ, UMBRAL_CREATURE_TEXTURE"]
fn retry_real_rear_crossing_burst_respects_bound8_with_adversarial_history() {
    let (_w, _g, p) = real_game(0.05);
    let rig = p.rig().unwrap();
    let template = p.debug().package().data().template();
    let rest = rig.rest_loop();
    let (cp, t) = (
        Vec3::from_array(rest.center),
        Vec3::from_array(rest.tangent),
    );
    let s = t.cross(Vec3::Y).normalized();
    let u = s.cross(t).normalized();
    let r_min = rig.min_radius() * (1.0 + 1e-9);
    let mut cases = Vec::new();
    // A) Historia y pico del harness del padre: 60 -> 216 a 0.05 y refinado 216 -> 223 desde el MISMO
    //    estado congelado con 1, 4, 20 y 160 muestras por grado.
    let (history, frozen) = retry_sweep(
        rig,
        template,
        "A historia padre 60-216".into(),
        s,
        r_min,
        60.0,
        216.0,
        20,
        None,
    );
    cases.push(history);
    for per_degree in [1_usize, 4, 20, 160] {
        let (case, _) = retry_sweep(
            rig,
            template,
            format!("A pico 216-223 x{per_degree}"),
            s,
            r_min,
            216.0,
            223.0,
            per_degree,
            frozen.clone(),
        );
        cases.push(case);
    }
    // B) Radio adversario: historia continua 60 -> 300 en la bola minima y algo mas lejos.
    for radius in [r_min, 0.75, 1.0] {
        let (case, _) = retry_sweep(
            rig,
            template,
            format!("B radio {radius:.4} 60-300"),
            s,
            radius,
            60.0,
            300.0,
            20,
            None,
        );
        cases.push(case);
    }
    // C) Memoria inicial adversaria (opuesta y perpendicular al lado de la punta) desde 150 grados.
    for (label, side) in [("opuesta", -s), ("perpendicular", u)] {
        let tip = cp + (t * 150f64.to_radians().cos() + s * 150f64.to_radians().sin()) * r_min;
        let memory = SpatialMemory {
            side: [side.x, side.y, side.z],
            tip_obj: [tip.x, tip.y, tip.z],
        };
        let (mut v, mut n) = (template.vertices.clone(), template.normals.clone());
        let pose = ok(
            rig.deform([tip.x, tip.y, tip.z], Some(memory), &mut v, &mut n),
            "C inicio",
        );
        let (case, _) = retry_sweep(
            rig,
            template,
            format!("C memoria {label} 150-300"),
            s,
            r_min,
            150.0,
            300.0,
            20,
            Some((pose.memory, v, tip)),
        );
        cases.push(case);
    }
    // D) Historia en otros planos por el polo trasero (vertical y diagonal) en la bola minima.
    for (label, plane) in [("vertical", u), ("diagonal", (s + u).normalized())] {
        let (case, _) = retry_sweep(
            rig,
            template,
            format!("D plano {label} 60-300"),
            plane,
            r_min,
            60.0,
            300.0,
            20,
            None,
        );
        cases.push(case);
    }
    let mut total_frames = 0;
    let mut total_violations = 0;
    for c in &cases {
        println!(
            "RETRY {}: cuadros {}, violaciones {}, ganancia maxima {}, primeras {:?}",
            c.name,
            c.frames,
            c.violations.len(),
            c.worst_gain,
            c.violations.iter().take(3).collect::<Vec<_>>()
        );
        total_frames += c.frames;
        total_violations += c.violations.len();
    }
    println!(
        "RETRY TOTAL: casos {}, cuadros {total_frames}, violaciones {total_violations}",
        cases.len()
    );
    assert_eq!(cases.len(), 12, "casos ejecutados");
    assert!(total_frames > 20_000, "cuadros evaluados: {total_frames}");
    assert_eq!(
        total_violations, 0,
        "LP-R02 real (8*paso+1e-5): {total_violations} violaciones"
    );
}
