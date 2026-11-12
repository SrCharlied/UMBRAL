//! Sonido procedural: sintesis acotada y determinista, mezcla y decision de efectos por cuadro.

use umbral::{
    audio::{AmbientParams, AudioDirector, AudioSnapshot, Mixer, SAMPLE_RATE, Sfx, synth::Noise},
    visual::tentacle_auto_capture::AutoEvent,
};

fn render(sfx: Sfx) -> Vec<f64> {
    let mut noise = Noise::new(7);
    let n = (sfx.duration() * f64::from(SAMPLE_RATE)) as usize;
    (0..n)
        .map(|i| sfx.sample(i as f64 / f64::from(SAMPLE_RATE), &mut noise))
        .collect()
}

#[test]
fn cada_efecto_suena_esta_acotado_y_termina() {
    for sfx in Sfx::ALL {
        let s = render(sfx);
        assert!(
            s.iter().all(|v| v.is_finite() && v.abs() <= 1.5),
            "{sfx:?} fuera de rango"
        );
        let energy = s.iter().map(|v| v * v).sum::<f64>() / s.len() as f64;
        assert!(energy > 1e-5, "{sfx:?} sin energia ({energy})");
        let mut noise = Noise::new(7);
        assert_eq!(sfx.sample(sfx.duration() + 0.01, &mut noise), 0.0);
        assert_eq!(sfx.sample(-0.01, &mut noise), 0.0);
    }
}

#[test]
fn los_efectos_son_deterministas() {
    for sfx in Sfx::ALL {
        assert_eq!(render(sfx), render(sfx), "{sfx:?}");
    }
}

#[test]
fn el_mezclador_esta_acotado_y_libera_las_voces() {
    let mut m = Mixer::new();
    m.set_params(AmbientParams {
        oxygen01: 0.05,
        speed01: 1.0,
        boost: true,
        dying: true,
        ..AmbientParams::default()
    });
    for sfx in Sfx::ALL {
        m.play(sfx);
    }
    let mut max: f64 = 0.0;
    for _ in 0..(SAMPLE_RATE as usize * 6) {
        let s = m.next_sample();
        assert!(s.is_finite());
        max = max.max(s.abs());
    }
    assert!(max <= 1.0 && max > 0.05);
    assert_eq!(m.active_voices(), 0, "los efectos terminaron");
    let mut block = [0i16; 512];
    m.fill_i16(&mut block);
}

#[test]
fn silencio_y_parametros_invalidos() {
    let mut m = Mixer::new();
    m.set_params(AmbientParams {
        oxygen01: f64::NAN,
        speed01: f64::INFINITY,
        muted: true,
        ..AmbientParams::default()
    });
    m.play(Sfx::Victory);
    // Tras la transicion del nivel, el silencio es casi total.
    for _ in 0..SAMPLE_RATE {
        m.next_sample();
    }
    let loud = (0..2000).map(|_| m.next_sample().abs()).fold(0.0, f64::max);
    assert!(loud < 0.02, "silenciado deja {loud}");
}

#[test]
fn el_ambiente_cambia_con_el_oxigeno() {
    let rms = |oxygen01: f64| {
        let mut m = Mixer::new();
        m.set_params(AmbientParams {
            oxygen01,
            ..AmbientParams::default()
        });
        for _ in 0..SAMPLE_RATE * 2 {
            m.next_sample();
        }
        let n = SAMPLE_RATE as usize * 2;
        ((0..n).map(|_| m.next_sample().powi(2)).sum::<f64>() / n as f64).sqrt()
    };
    // Con O2 bajo se suman latido y susurro de los ojos.
    assert!(rms(0.05) > rms(1.0) * 1.3, "{} vs {}", rms(0.05), rms(1.0));
}

fn snap() -> AudioSnapshot {
    AudioSnapshot {
        oxygen01: 1.0,
        ..AudioSnapshot::default()
    }
}

#[test]
fn el_director_traduce_cada_cambio_en_su_efecto() {
    let mut d = AudioDirector::new();
    assert!(
        d.frame(snap(), &[]).is_empty(),
        "el primer cuadro solo fija la referencia"
    );
    let mut s = snap();
    s.resources = 1;
    assert_eq!(d.frame(s, &[]), vec![Sfx::TankPickup]);
    s.logs = 1;
    assert_eq!(d.frame(s, &[]), vec![Sfx::LogPickup]);
    s.oxygen01 = 0.6;
    assert_eq!(d.frame(s, &[]), vec![Sfx::EyesOpen]);
    s.oxygen01 = 0.45;
    assert_eq!(d.frame(s, &[]), vec![Sfx::OxygenAlarm]);
    s.speed = 120.0;
    assert!(d.frame(s, &[]).is_empty());
    s.speed = 0.0;
    assert_eq!(d.frame(s, &[]), vec![Sfx::Collision]);
    // Monstruo: rugido, agarre y arrastre, y la muerte por la bestia.
    s.dying = true;
    s.oxygen01 = 0.0;
    assert_eq!(
        d.frame(s, &[AutoEvent::Contact]),
        vec![Sfx::TentacleGrab, Sfx::MonsterRoar]
    );
    assert_eq!(d.frame(s, &[AutoEvent::DragStarted]), vec![Sfx::Drag]);
    s.dying = false;
    s.game_over = true;
    assert_eq!(d.frame(s, &[]), vec![Sfx::BeastKill]);
    // Partida nueva.
    let mut fresh = snap();
    fresh.restart_generation = 1;
    assert_eq!(d.frame(fresh, &[]), vec![Sfx::Restart]);
}

#[test]
fn asfixia_sin_monstruo_y_victoria() {
    let mut d = AudioDirector::new();
    let mut s = snap();
    s.oxygen01 = 0.01;
    d.frame(s, &[]);
    s.game_over = true;
    s.oxygen01 = 0.0;
    assert_eq!(d.frame(s, &[]), vec![Sfx::Suffocate]);
    let mut d = AudioDirector::new();
    let mut s = snap();
    s.logs = 3;
    d.frame(s, &[]);
    s.victory = true;
    assert_eq!(d.frame(s, &[]), vec![Sfx::Victory]);
}

#[test]
fn ganar_con_el_tercer_registro_suena_solo_la_fanfarria() {
    let mut d = AudioDirector::new();
    let mut s = snap();
    s.logs = 2;
    d.frame(s, &[]);
    s.logs = 3;
    s.victory = true;
    assert_eq!(d.frame(s, &[]), vec![Sfx::Victory]);
}

#[test]
fn la_victoria_empieza_con_una_fanfarria_audible() {
    // En los primeros 0.6 s ya hay energia clara (antes solo crecia el acorde).
    let mut noise = Noise::new(3);
    let n = (0.6 * f64::from(SAMPLE_RATE)) as usize;
    let early: f64 = (0..n)
        .map(|i| {
            Sfx::Victory
                .sample(i as f64 / f64::from(SAMPLE_RATE), &mut noise)
                .powi(2)
        })
        .sum::<f64>()
        / n as f64;
    assert!(early.sqrt() > 0.12, "fanfarria debil: {}", early.sqrt());
}
