//! Sintesis procedural de todo el sonido del juego (sin archivos de audio): osciladores, ruido
//! y envolventes. Puro y determinista, para poder probarlo sin dispositivo de salida.
//!
//! - `Sfx`: efectos de un disparo (recoger, registro, rugido, agarre, muerte...). Cada uno es
//!   una funcion del tiempo desde su inicio, con duracion fija.
//! - `Ambient`: capas continuas que dependen del estado (zumbido del espacio, motor segun la
//!   velocidad, latido con O2 bajo, susurro de los ojos, retumbar del monstruo).
//! - `Mixer`: suma ambiente y efectos activos y aplica un limitador suave.

use std::f64::consts::TAU;

/// Frecuencia de muestreo de la salida.
pub const SAMPLE_RATE: u32 = 44_100;

/// Efectos de sonido de un disparo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// Tanque de oxigeno recogido: campanita ascendente.
    TankPickup,
    /// Registro dorado recogido: arpegio de campanas sobre un bordon grave.
    LogPickup,
    /// Victoria: acorde mayor que crece con brillo.
    Victory,
    /// El monstruo despierta (empieza la secuencia de muerte): rugido grave.
    MonsterRoar,
    /// El tentaculo atrapa la nave: golpe humedo.
    TentacleGrab,
    /// Arrastre hacia la criatura: retumbar con crujidos.
    Drag,
    /// El monstruo mata: impacto y alarido descendente.
    BeastKill,
    /// Muerte por asfixia sin monstruo: latido que se apaga y linea plana.
    Suffocate,
    /// Alarma de oxigeno (cruce de 50 %, 25 % y 10 %): doble pitido.
    OxygenAlarm,
    /// Los ojos empiezan a abrirse (O2 bajo 65 %): susurro inquietante.
    EyesOpen,
    /// Choque con un planeta: golpe sordo.
    Collision,
    /// Partida nueva: barrido ascendente.
    Restart,
    /// Interfaz (L, F2, silencio): clic corto.
    Blip,
}

impl Sfx {
    pub const ALL: [Sfx; 13] = [
        Sfx::TankPickup,
        Sfx::LogPickup,
        Sfx::Victory,
        Sfx::MonsterRoar,
        Sfx::TentacleGrab,
        Sfx::Drag,
        Sfx::BeastKill,
        Sfx::Suffocate,
        Sfx::OxygenAlarm,
        Sfx::EyesOpen,
        Sfx::Collision,
        Sfx::Restart,
        Sfx::Blip,
    ];

    /// Duracion en segundos.
    pub fn duration(self) -> f64 {
        match self {
            Sfx::TankPickup => 0.9,
            Sfx::LogPickup => 2.2,
            Sfx::Victory => 5.0,
            Sfx::MonsterRoar => 3.0,
            Sfx::TentacleGrab => 0.9,
            Sfx::Drag => 3.6,
            Sfx::BeastKill => 3.5,
            Sfx::Suffocate => 3.5,
            Sfx::OxygenAlarm => 0.6,
            Sfx::EyesOpen => 2.5,
            Sfx::Collision => 0.5,
            Sfx::Restart => 1.0,
            Sfx::Blip => 0.08,
        }
    }

    /// Muestra en `t` segundos desde el inicio, aproximadamente en `[-1, 1]`. `noise` es un
    /// generador de ruido blanco propio de la voz.
    pub fn sample(self, t: f64, noise: &mut Noise) -> f64 {
        if !(0.0..self.duration()).contains(&t) {
            return 0.0;
        }
        let d = self.duration();
        match self {
            Sfx::TankPickup => {
                let note = if t < 0.12 { 660.0 } else { 990.0 };
                let local = if t < 0.12 { t } else { t - 0.12 };
                let bell =
                    sine(note, t) * 0.6 + sine(note * 2.01, t) * 0.25 + sine(note * 3.0, t) * 0.1;
                bell * attack_decay(local, 0.005, 3.5) * 0.55
                    + noise.white() * 0.04 * decay(t, 18.0)
            }
            Sfx::LogPickup => {
                let notes = [523.25, 659.25, 783.99, 1046.5];
                let mut s = 0.0;
                for (i, &f) in notes.iter().enumerate() {
                    let start = i as f64 * 0.13;
                    if t >= start {
                        let lt = t - start;
                        s += (sine(f, lt) * 0.55 + sine(f * 2.76, lt) * 0.18)
                            * attack_decay(lt, 0.004, 2.2);
                    }
                }
                let drone = sine(130.8, t) * 0.25 * envelope(t, 0.3, d - 0.8, d);
                (s * 0.4 + drone) * 0.8
            }
            Sfx::Victory => {
                // Fanfarria: do-mi-sol-do agudo, brillante y marcada (tipo metal suave).
                let fanfare_notes = [523.25, 659.25, 783.99, 1046.5];
                let mut fanfare = 0.0;
                for (i, &f) in fanfare_notes.iter().enumerate() {
                    let start = i as f64 * 0.14;
                    let lt = t - start;
                    if lt >= 0.0 {
                        // La ultima nota se sostiene mas.
                        let rate = if i == 3 { 1.6 } else { 5.0 };
                        let brass =
                            sine(f, lt) * 0.6 + sine(f * 2.0, lt) * 0.25 + sine(f * 3.0, lt) * 0.12;
                        fanfare += brass * attack_decay(lt, 0.01, rate);
                    }
                }
                // Despues, el acorde mayor que crece con brillo.
                let swell_t = t - 0.55;
                let env = envelope(swell_t, 0.9, 3.2, d - 0.55);
                let chord = [261.63, 329.63, 392.0, 523.25]
                    .iter()
                    .map(|&f| sine(f, t) + 0.3 * sine(f * 2.0 + 0.7 * (t * 3.0).sin(), t))
                    .sum::<f64>()
                    / 4.0;
                let shimmer = sine(1567.98, t) * 0.15 * (0.5 + 0.5 * (t * 7.0).sin());
                fanfare * 0.5 + (chord * 0.55 + shimmer) * env
            }
            Sfx::MonsterRoar => {
                let env = envelope(t, 0.4, 2.0, d);
                let f = 62.0 - 22.0 * (t / d);
                let growl = saw(f, t) * 0.5 + saw(f * 1.5 + 3.0, t) * 0.25;
                let rasp = noise.lowpassed(0.08) * (0.6 + 0.4 * (t * 31.0).sin());
                ((growl + rasp * 1.2) * env * 0.75).tanh()
            }
            Sfx::TentacleGrab => {
                let thump = sine(70.0 * (1.0 + 2.0 * decay(t, 12.0)), t) * decay(t, 7.0);
                let squelch =
                    noise.lowpassed(0.25) * attack_decay(t, 0.01, 9.0) * (1.0 + (t * 90.0).sin());
                (thump * 0.9 + squelch * 0.6).clamp(-1.0, 1.0) * 0.8
            }
            Sfx::Drag => {
                let env = envelope(t, 0.3, d - 0.6, d);
                let rumble = noise.lowpassed(0.03) * 2.5;
                let creak = sine(180.0 + 40.0 * (t * 2.3).sin(), t)
                    * 0.15
                    * (0.5 + 0.5 * (t * 5.0).sin()).powi(4);
                (rumble + creak + sine(41.0, t) * 0.3) * env * 0.6
            }
            Sfx::BeastKill => {
                let boom = sine(48.0 * (1.0 + 3.0 * decay(t, 6.0)), t) * decay(t, 2.5);
                let shriek_f = 900.0 * (-t * 0.7).exp() + 120.0;
                let shriek = (saw(shriek_f, t) * 0.4 + noise.lowpassed(0.4) * 0.3)
                    * envelope(t, 0.05, 1.6, 2.6);
                let tail = noise.lowpassed(0.02) * 2.0 * envelope(t, 0.5, 2.0, d);
                ((boom * 1.1 + shriek + tail) * 0.75).tanh()
            }
            Sfx::Suffocate => {
                // Dos latidos que se apagan y una linea plana.
                let beat = |start: f64, amp: f64| {
                    if t >= start {
                        let lt = t - start;
                        sine(55.0, lt) * attack_decay(lt, 0.005, 14.0) * amp
                    } else {
                        0.0
                    }
                };
                let heart = beat(0.0, 0.9) + beat(0.22, 0.6) + beat(1.1, 0.5) + beat(1.32, 0.3);
                let flat = sine(1000.0, t)
                    * 0.12
                    * envelope(t - 1.9, 0.05, 1.3, d - 1.9)
                    * f64::from(u8::from(t > 1.9));
                heart + flat
            }
            Sfx::OxygenAlarm => {
                let beep = |start: f64| {
                    let lt = t - start;
                    if (0.0..0.18).contains(&lt) {
                        square(880.0, lt) * envelope(lt, 0.005, 0.15, 0.18)
                    } else {
                        0.0
                    }
                };
                (beep(0.0) + beep(0.28)) * 0.22
            }
            Sfx::EyesOpen => {
                let env = envelope(t, 0.8, 1.6, d);
                let whisper = noise.highpassed() * 0.35 * (0.6 + 0.4 * (t * 11.0).sin());
                let tones =
                    sine(1480.0, t) * 0.08 + sine(1487.0, t) * 0.08 + sine(2217.0, t) * 0.05;
                (whisper + tones) * env
            }
            Sfx::Collision => {
                let thud = sine(65.0 * (1.0 + decay(t, 15.0)), t) * decay(t, 9.0);
                (thud + noise.lowpassed(0.15) * decay(t, 20.0) * 0.8) * 0.8
            }
            Sfx::Restart => {
                let f = 200.0 + 900.0 * (t / d).powi(2);
                (sine(f, t) * 0.25 + noise.highpassed() * 0.15) * envelope(t, 0.2, 0.6, d)
            }
            Sfx::Blip => sine(1320.0, t) * envelope(t, 0.003, 0.05, d) * 0.25,
        }
    }
}

/// Estado continuo que modula el ambiente (lo publica el juego cada cuadro).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientParams {
    /// O2 que ve el jugador `[0, 1]`.
    pub oxygen01: f64,
    /// Velocidad de la nave normalizada `[0, 1]`.
    pub speed01: f64,
    pub boost: bool,
    /// Secuencia de muerte por el monstruo en curso.
    pub dying: bool,
    /// Partida terminada (pantalla final): el ambiente baja.
    pub terminal: bool,
    /// Silencio total (tecla U).
    pub muted: bool,
}

impl Default for AmbientParams {
    fn default() -> Self {
        Self {
            oxygen01: 1.0,
            speed01: 0.0,
            boost: false,
            dying: false,
            terminal: false,
            muted: false,
        }
    }
}

/// Ruido blanco determinista (xorshift) con filtros de un polo.
#[derive(Clone, Copy, Debug)]
pub struct Noise {
    state: u64,
    low: f64,
    high_prev: f64,
}

impl Noise {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
            low: 0.0,
            high_prev: 0.0,
        }
    }

    pub fn white(&mut self) -> f64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }

    /// Paso bajo de un polo (`alpha` en `(0, 1]`; menor = mas grave).
    pub fn lowpassed(&mut self, alpha: f64) -> f64 {
        let n = self.white();
        self.low += (n - self.low) * alpha;
        self.low
    }

    /// Paso alto simple (diferencia): siseo.
    pub fn highpassed(&mut self) -> f64 {
        let n = self.white();
        let out = (n - self.high_prev) * 0.5;
        self.high_prev = n;
        out
    }
}

fn sine(f: f64, t: f64) -> f64 {
    (TAU * f * t).sin()
}

fn saw(f: f64, t: f64) -> f64 {
    2.0 * (f * t).fract() - 1.0
}

fn square(f: f64, t: f64) -> f64 {
    if (f * t).fract() < 0.5 { 1.0 } else { -1.0 }
}

fn decay(t: f64, rate: f64) -> f64 {
    (-t * rate).exp()
}

fn attack_decay(t: f64, attack: f64, rate: f64) -> f64 {
    if t < 0.0 {
        0.0
    } else if t < attack {
        t / attack
    } else {
        decay(t - attack, rate)
    }
}

/// Subida lineal hasta `attack`, sostenido hasta `release_at` y caida lineal hasta `end`.
fn envelope(t: f64, attack: f64, release_at: f64, end: f64) -> f64 {
    if t < 0.0 || t >= end {
        0.0
    } else if t < attack {
        t / attack
    } else if t < release_at {
        1.0
    } else {
        ((end - t) / (end - release_at).max(1e-9)).clamp(0.0, 1.0)
    }
}

/// Voz activa de un efecto.
#[derive(Clone, Copy, Debug)]
struct Voice {
    sfx: Sfx,
    t: f64,
    noise: Noise,
}

/// Mezclador: ambiente continuo + efectos; avanza muestra a muestra.
#[derive(Clone, Debug)]
pub struct Mixer {
    voices: Vec<Voice>,
    params: AmbientParams,
    /// Parametros suavizados (evitan clics al cambiar de golpe).
    smooth_oxygen: f64,
    smooth_speed: f64,
    smooth_level: f64,
    time: f64,
    noise: Noise,
    engine_noise: Noise,
    seed: u64,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub const MAX_VOICES: usize = 24;

    pub fn new() -> Self {
        Self {
            voices: Vec::new(),
            params: AmbientParams::default(),
            smooth_oxygen: 1.0,
            smooth_speed: 0.0,
            smooth_level: 1.0,
            time: 0.0,
            noise: Noise::new(0x5EED_A1B1),
            engine_noise: Noise::new(0xE161_9E00),
            seed: 1,
        }
    }

    pub fn set_params(&mut self, params: AmbientParams) {
        let clean = |v: f64, d: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        self.params = AmbientParams {
            oxygen01: clean(params.oxygen01, 1.0),
            speed01: clean(params.speed01, 0.0),
            ..params
        };
    }

    pub fn play(&mut self, sfx: Sfx) {
        if self.voices.len() >= Self::MAX_VOICES {
            self.voices.remove(0);
        }
        self.seed = self
            .seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(1);
        self.voices.push(Voice {
            sfx,
            t: 0.0,
            noise: Noise::new(self.seed),
        });
    }

    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }

    /// Siguiente muestra en `[-1, 1]`.
    pub fn next_sample(&mut self) -> f64 {
        let dt = 1.0 / f64::from(SAMPLE_RATE);
        let p = self.params;
        let k = 1.0 - (-dt * 3.0).exp();
        self.smooth_oxygen += (p.oxygen01 - self.smooth_oxygen) * k;
        self.smooth_speed += (p.speed01 - self.smooth_speed) * k;
        let target_level = if p.muted {
            0.0
        } else if p.terminal {
            0.35
        } else {
            1.0
        };
        self.smooth_level += (target_level - self.smooth_level) * (1.0 - (-dt * 4.0).exp());
        let ambient = self.ambient(dt);
        let mut fx = 0.0;
        for v in &mut self.voices {
            fx += v.sfx.sample(v.t, &mut v.noise);
            v.t += dt;
        }
        self.voices.retain(|v| v.t < v.sfx.duration());
        self.time += dt;
        let fx_level = if p.muted { 0.0 } else { 1.0 };
        ((ambient * self.smooth_level + fx * fx_level) * 0.9).tanh()
    }

    fn ambient(&mut self, _dt: f64) -> f64 {
        let t = self.time;
        let o2 = self.smooth_oxygen;
        let p = self.params;
        // Zumbido del espacio: bordon grave con batido lento y viento filtrado.
        let swell = 0.7 + 0.3 * (t * 0.13).sin();
        let drone = (sine(55.0, t) * 0.5 + sine(82.6, t) * 0.3 + sine(110.3, t) * 0.12) * swell;
        let wind = self.noise.lowpassed(0.015) * 3.0 * (0.6 + 0.4 * (t * 0.07).sin());
        let mut out = drone * 0.09 + wind * 0.06;
        // Motor: armonicos segun la velocidad y siseo con impulso.
        let speed = self.smooth_speed;
        if speed > 1e-3 {
            let f = 38.0 + 140.0 * speed;
            let engine = sine(f, t) * 0.5 + sine(f * 2.0, t) * 0.25 + saw(f * 0.5, t) * 0.15;
            let hiss = if p.boost {
                self.engine_noise.lowpassed(0.3) * 0.5
            } else {
                0.0
            };
            out += (engine * 0.07 + hiss * 0.1) * speed.sqrt();
        }
        // Ojos (O2 bajo 65 %): cumulo agudo con tremolo, mas fuerte cuanto menos O2.
        let eerie = ((0.65 - o2) / 0.65).clamp(0.0, 1.0);
        if eerie > 0.0 {
            let trem = 0.5 + 0.5 * (t * 5.3).sin();
            out +=
                (sine(1318.5, t) + sine(1324.0, t) + 0.6 * sine(1975.5, t)) * 0.012 * eerie * trem;
        }
        // Latido bajo 30 % (o durante el monstruo): mas rapido y fuerte con menos O2.
        let danger = if p.dying {
            1.0
        } else {
            ((0.3 - o2) / 0.3).clamp(0.0, 1.0)
        };
        if danger > 0.0 {
            let bpm = 60.0 + 70.0 * danger;
            let phase = (t * bpm / 60.0).fract() * 60.0 / bpm;
            let beat = |start: f64, amp: f64| {
                let lt = phase - start;
                if lt >= 0.0 {
                    sine(52.0, lt) * attack_decay(lt, 0.004, 16.0) * amp
                } else {
                    0.0
                }
            };
            out += (beat(0.0, 1.0) + beat(0.2, 0.6)) * 0.45 * danger;
        }
        // Monstruo: retumbar grave continuo.
        if p.dying {
            out += (self.noise.lowpassed(0.01) * 4.0 + sine(36.0, t) * 0.4) * 0.12;
        }
        out
    }

    /// Llena `out` con muestras i16 (mono).
    pub fn fill_i16(&mut self, out: &mut [i16]) {
        for s in out {
            *s = (self.next_sample() * 30_000.0).round() as i16;
        }
    }
}
