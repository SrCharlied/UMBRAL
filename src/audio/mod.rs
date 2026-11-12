//! Sonido del juego, sin dependencias nuevas: sintesis procedural (`synth`) y salida directa a
//! Windows con la API `waveOut` de winmm (FFI). En otros sistemas el sonido queda en silencio.
//!
//! - `Audio::start` abre el dispositivo y lanza un hilo que mezcla en bloques de ~23 ms con
//!   cuatro buferes en cola (~90 ms de latencia). El juego solo publica parametros (`set_params`)
//!   y dispara efectos (`play`); nada del hilo de audio toca el mundo.
//! - `AudioDirector` compara el estado del juego cuadro a cuadro y decide que efectos suenan
//!   (puro y probado en `tests/audio.rs`).
//! - Sin ventana (`--headless`), con `--no-sound` o sin dispositivo, no se crea nada.

pub mod synth;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub use synth::{AmbientParams, Mixer, SAMPLE_RATE, Sfx};

use crate::{scene::world::World, visual::tentacle_auto_capture::AutoEvent};

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Activa o desactiva el sonido para las siguientes partidas (`--no-sound`).
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

struct Shared {
    mixer: Mutex<Mixer>,
    stop: AtomicBool,
}

/// Salida de audio en marcha; al soltarla se detiene el hilo y se cierra el dispositivo.
pub struct Audio {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Audio {
    /// Abre la salida de audio, o `None` si el sonido esta desactivado o no hay dispositivo.
    pub fn start() -> Option<Self> {
        if !enabled() {
            return None;
        }
        let shared = Arc::new(Shared {
            mixer: Mutex::new(Mixer::new()),
            stop: AtomicBool::new(false),
        });
        let worker = Arc::clone(&shared);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("umbral-audio".into())
            .spawn(move || backend::run(&worker, ready_tx))
            .ok()?;
        match ready_rx.recv() {
            Ok(true) => Some(Self {
                shared,
                thread: Some(thread),
            }),
            _ => {
                let _ = thread.join();
                None
            }
        }
    }

    pub fn play(&self, sfx: Sfx) {
        if let Ok(mut m) = self.shared.mixer.lock() {
            m.play(sfx);
        }
    }

    pub fn set_params(&self, params: AmbientParams) {
        if let Ok(mut m) = self.shared.mixer.lock() {
            m.set_params(params);
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Lo que el director necesita del estado del juego en un cuadro.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AudioSnapshot {
    pub resources: u32,
    pub logs: u32,
    pub game_over: bool,
    pub victory: bool,
    pub dying: bool,
    pub restart_generation: u64,
    /// O2 mostrado `[0, 1]`.
    pub oxygen01: f64,
    pub speed: f64,
    /// Choques con asteroides acumulados (`World::asteroid_hits`).
    pub asteroid_hits: u32,
}

impl AudioSnapshot {
    pub fn of(world: &World) -> Self {
        Self {
            resources: world.gameplay.collected_resources,
            logs: world.gameplay.collected_logs,
            game_over: world.gameplay.game_over,
            victory: world.gameplay.objective_complete,
            dying: world.dying,
            restart_generation: world.restart_generation,
            oxygen01: if world.dying { 0.0 } else { world.oxygen01() },
            speed: world.ship.speed,
            asteroid_hits: world.asteroid_hits,
        }
    }
}

/// Umbrales de O2 que disparan la alarma al cruzarlos hacia abajo.
pub const ALARM_LEVELS: [f64; 3] = [0.5, 0.25, 0.1];
/// Umbral de los ojos (`watching_stars::OBSERVER_ONSET_OXYGEN`).
pub const EYES_LEVEL: f64 = 0.65;

/// Decide los efectos de cada cuadro comparando con el anterior.
#[derive(Clone, Debug, Default)]
pub struct AudioDirector {
    prev: Option<AudioSnapshot>,
}

impl AudioDirector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Efectos de este cuadro. `events`: eventos de la captura automatica del cuadro.
    pub fn frame(&mut self, now: AudioSnapshot, events: &[AutoEvent]) -> Vec<Sfx> {
        let mut out = Vec::new();
        for e in events {
            match e {
                AutoEvent::Contact => out.push(Sfx::TentacleGrab),
                AutoEvent::DragStarted => out.push(Sfx::Drag),
                _ => {}
            }
        }
        let Some(prev) = self.prev.replace(now) else {
            return out;
        };
        if now.restart_generation != prev.restart_generation {
            out.push(Sfx::Restart);
            return out;
        }
        if now.resources > prev.resources {
            out.push(Sfx::TankPickup);
        }
        // Si el registro da la victoria en este mismo cuadro, suena solo la fanfarria.
        if now.logs > prev.logs && !(now.victory && !prev.victory) {
            out.push(Sfx::LogPickup);
        }
        if now.dying && !prev.dying {
            out.push(Sfx::MonsterRoar);
        }
        if now.game_over && !prev.game_over {
            out.push(if prev.dying {
                Sfx::BeastKill
            } else {
                Sfx::Suffocate
            });
        }
        if now.victory && !prev.victory {
            out.push(Sfx::Victory);
        }
        if !now.game_over && !now.victory && !now.dying {
            if ALARM_LEVELS
                .iter()
                .any(|&l| prev.oxygen01 >= l && now.oxygen01 < l)
            {
                out.push(Sfx::OxygenAlarm);
            }
            if prev.oxygen01 >= EYES_LEVEL && now.oxygen01 < EYES_LEVEL {
                out.push(Sfx::EyesOpen);
            }
            // Asteroide (quita O2) o choque con un planeta: la nave iba rapido y se detuvo de golpe.
            if now.asteroid_hits > prev.asteroid_hits {
                out.push(Sfx::AsteroidHit);
            } else if prev.speed > 25.0 && now.speed < 1.0 {
                out.push(Sfx::Collision);
            }
        }
        out
    }
}

/// Parametros del ambiente a partir del mundo.
pub fn ambient_params(world: &World, muted: bool) -> AmbientParams {
    let s = &world.ship.settings;
    let top = (s.max_speed * s.boost_multiplier).max(1e-9);
    AmbientParams {
        oxygen01: if world.dying { 0.0 } else { world.oxygen01() },
        speed01: (world.ship.speed.abs() / top).clamp(0.0, 1.0),
        boost: world.ship.speed > s.max_speed * 1.05,
        dying: world.dying,
        terminal: world.gameplay.game_over || world.gameplay.objective_complete,
        muted,
    }
}

#[cfg(windows)]
mod backend {
    //! Salida `waveOut` de winmm: PCM 16 bits mono a `SAMPLE_RATE`, cuatro buferes en cola.
    use super::{SAMPLE_RATE, Shared};
    use std::sync::{atomic::Ordering, mpsc::Sender};

    #[repr(C)]
    struct WaveFormatEx {
        format_tag: u16,
        channels: u16,
        samples_per_sec: u32,
        avg_bytes_per_sec: u32,
        block_align: u16,
        bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct WaveHdr {
        data: *mut i8,
        buffer_length: u32,
        bytes_recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: *mut WaveHdr,
        reserved: usize,
    }

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn waveOutOpen(
            handle: *mut isize,
            device: u32,
            format: *const WaveFormatEx,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        fn waveOutPrepareHeader(handle: isize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutUnprepareHeader(handle: isize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutWrite(handle: isize, header: *mut WaveHdr, size: u32) -> u32;
        fn waveOutReset(handle: isize) -> u32;
        fn waveOutClose(handle: isize) -> u32;
    }

    const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
    const WAVE_FORMAT_PCM: u16 = 1;
    const WHDR_DONE: u32 = 0x1;
    const BLOCK: usize = 1024;
    const BUFFERS: usize = 4;

    pub(super) fn run(shared: &Shared, ready: Sender<bool>) {
        let format = WaveFormatEx {
            format_tag: WAVE_FORMAT_PCM,
            channels: 1,
            samples_per_sec: SAMPLE_RATE,
            avg_bytes_per_sec: SAMPLE_RATE * 2,
            block_align: 2,
            bits_per_sample: 16,
            cb_size: 0,
        };
        let mut handle: isize = 0;
        // SAFETY: `format` vive durante la llamada; sin callback (CALLBACK_NULL).
        let opened = unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, 0, 0, 0) } == 0;
        let _ = ready.send(opened);
        if !opened {
            return;
        }
        let size = std::mem::size_of::<WaveHdr>() as u32;
        // Datos y cabeceras en el monton: direcciones estables mientras el driver las usa.
        let mut data: Vec<Box<[i16; BLOCK]>> =
            (0..BUFFERS).map(|_| Box::new([0i16; BLOCK])).collect();
        let mut headers: Vec<Box<WaveHdr>> = data
            .iter_mut()
            .map(|d| {
                Box::new(WaveHdr {
                    data: d.as_mut_ptr().cast(),
                    buffer_length: (BLOCK * 2) as u32,
                    bytes_recorded: 0,
                    user: 0,
                    flags: 0,
                    loops: 0,
                    next: std::ptr::null_mut(),
                    reserved: 0,
                })
            })
            .collect();
        let fill = |block: &mut [i16; BLOCK]| {
            if let Ok(mut m) = shared.mixer.lock() {
                m.fill_i16(block);
            } else {
                block.fill(0);
            }
        };
        for (h, d) in headers.iter_mut().zip(data.iter_mut()) {
            fill(d);
            // SAFETY: cabecera y datos en cajas que viven hasta el cierre del dispositivo.
            unsafe {
                waveOutPrepareHeader(handle, &mut **h, size);
                waveOutWrite(handle, &mut **h, size);
            }
        }
        let mut next = 0;
        while !shared.stop.load(Ordering::Relaxed) {
            // El driver marca WHDR_DONE al terminar de reproducir un bufer.
            // SAFETY: lectura volatil del campo que escribe el driver.
            let done = unsafe { std::ptr::read_volatile(&headers[next].flags) } & WHDR_DONE != 0;
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            }
            fill(&mut data[next]);
            // SAFETY: el bufer termino; se reescribe la misma cabecera preparada.
            unsafe {
                headers[next].flags &= !WHDR_DONE;
                waveOutWrite(handle, &mut *headers[next], size);
            }
            next = (next + 1) % BUFFERS;
        }
        // SAFETY: reset devuelve todos los buferes; despues se liberan y se cierra.
        unsafe {
            waveOutReset(handle);
            for h in &mut headers {
                waveOutUnprepareHeader(handle, &mut **h, size);
            }
            waveOutClose(handle);
        }
    }
}

#[cfg(not(windows))]
mod backend {
    use super::Shared;
    use std::sync::mpsc::Sender;

    /// Sin backend fuera de Windows: el juego sigue en silencio.
    pub(super) fn run(_shared: &Shared, ready: Sender<bool>) {
        let _ = ready.send(false);
    }
}
