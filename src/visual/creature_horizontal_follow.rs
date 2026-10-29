//! Seguimiento corporal horizontal (candidata 02, opt-in): la criatura entera gira alrededor de
//! +Y del mundo, sobre su centro fijo, hacia la proyeccion XZ del punto elegido con `G`, cerca y
//! lejos, en todo el azimut, lenta y con rapidez limitada. Sin giro vertical. Separada del giro
//! global de la candidata 01 (`creature_distant_turn`), que se conserva. Aqui vive el nucleo
//! puro. Ver `docs/static-creature.md`.

use crate::{
    math::{Mat4, Transform, Vec3},
    visual::{creature_distant_turn::CreaturePreset, static_creature::StaticCreature},
};
use std::f64::consts::{PI, TAU};

/// Combinaciones de `--creature-horizontal-follow`, sin tocar archivos. Se valida despues de
/// `validate_eye_compare_flags`; no se apila con el giro global de la candidata 01.
pub fn validate_horizontal_follow_flags(
    horizontal: bool,
    compare: bool,
    distant_turn: bool,
    legacy: bool,
) -> Result<(), String> {
    if !horizontal {
        return Ok(());
    }
    if legacy {
        return Err("--legacy no admite --creature-horizontal-follow".into());
    }
    if distant_turn {
        return Err(
            "--creature-horizontal-follow y --creature-distant-turn son incompatibles: elige uno"
                .into(),
        );
    }
    if !compare {
        return Err("--creature-horizontal-follow necesita --creature-eye-compare".into());
    }
    Ok(())
}

/// Angulo equivalente en `(-pi, pi]` (`-pi` da `pi`; en rango, el mismo valor exacto); no finito
/// da 0. Acota el estado tras
/// cualquier numero de vueltas.
pub fn wrap_angle(angle: f64) -> f64 {
    if !angle.is_finite() {
        return 0.0;
    }
    // Ya en rango: exacto, sin redondeo nuevo.
    if angle > -PI && angle <= PI {
        return angle;
    }
    let wrapped = (angle + PI).rem_euclid(TAU) - PI;
    if wrapped <= -PI { PI } else { wrapped }
}

/// Azimut (radianes, `(-pi, pi]`) de la proyeccion XZ del objetivo vista desde el centro, en el
/// marco de reposo de la criatura: 0 es el frente (+Z del OBJ) y positivo hacia +X del OBJ (la
/// convencion de la candidata 01). La altura no cuenta. `None` sin direccion XZ (objetivo sobre
/// o bajo el centro, o coincidente) o con datos no finitos; antes se divide por la componente
/// mayor para no desbordar.
pub fn target_azimuth(center: Vec3, creature_yaw: f64, target: Vec3) -> Option<f64> {
    if !(center.is_finite() && creature_yaw.is_finite() && target.is_finite()) {
        return None;
    }
    let delta = target - center;
    let largest = delta.x.abs().max(delta.z.abs());
    if !(largest.is_finite() && largest > 0.0) {
        return None;
    }
    let d = Mat4::rotation_y(-creature_yaw).transform_dir(Vec3::new(
        delta.x / largest,
        0.0,
        delta.z / largest,
    ));
    if d.x.hypot(d.z) <= 1e-12 {
        return None;
    }
    Some(wrap_angle(d.x.atan2(d.z)))
}

const DEGREE: f64 = PI / 180.0;
/// Candidata unica (tecnica, sin aprobacion artistica). Persecucion exponencial con t95 de 4 s
/// para cambios chicos y rapidez angular maxima de 18 grados/s para los grandes: la tasa es
/// `min(k * |error|, MAX_SPEED)` con `k = ln 20 / t95`, asi que el tope manda por encima de
/// `MAX_SPEED / k` (unos 24 grados) y por debajo queda la exponencial. Con 18 grados/s una nave
/// que pasa a crucero (60 u/s) a 2.5 radios (~15.5 grados/s) se puede seguir; con 12 no.
pub const HORIZONTAL_FOLLOW_T95_SECONDS: f64 = 4.0;
pub const HORIZONTAL_FOLLOW_MAX_SPEED: f64 = 18.0 * DEGREE;
/// Error (radianes) por debajo del cual el estado se fija exactamente en el objetivo.
pub const HORIZONTAL_FOLLOW_SNAP_RADIANS: f64 = 1e-6;

/// Estado del seguimiento horizontal: encendido/apagado (`H`), giro actual (radianes en
/// `(-pi, pi]`, relativo al frente de reposo), ultimo objetivo valido, sentido del ultimo giro
/// (desempate antipodal) y rapidez real del ultimo paso. Ligero y sin geometria; se avanza con
/// segundos **reales** (F5 no lo congela).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalFollow {
    active: bool,
    yaw: f64,
    goal: Option<f64>,
    turn_sign: f64,
    speed: f64,
}

impl Default for HorizontalFollow {
    fn default() -> Self {
        Self::new()
    }
}

impl HorizontalFollow {
    /// Encendido, de frente y sin objetivo.
    pub fn new() -> Self {
        Self {
            active: true,
            yaw: 0.0,
            goal: None,
            turn_sign: 1.0,
            speed: 0.0,
        }
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    /// `H`. Apagado el objetivo efectivo es el frente de reposo y vuelve con la misma dinamica
    /// (no es una pausa); encendido retoma el ultimo objetivo valido. Nunca salta.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    /// Giro actual relativo al frente de reposo (radianes, `(-pi, pi]`).
    pub fn yaw(&self) -> f64 {
        self.yaw
    }
    /// Ultimo azimut valido pedido (se conserva aunque el objetivo pierda direccion).
    pub fn goal(&self) -> Option<f64> {
        self.goal
    }
    /// Lo que persigue ahora: el ultimo objetivo valido encendido, el frente apagado.
    pub fn effective_goal(&self) -> Option<f64> {
        if self.active { self.goal } else { Some(0.0) }
    }
    /// Error por el arco corto del giro actual al objetivo efectivo, en `(-pi, pi]`; en el empate
    /// antipodal exacto, con el sentido del ultimo giro.
    pub fn error(&self) -> Option<f64> {
        let goal = self.effective_goal()?;
        let error = wrap_angle(goal - self.yaw);
        Some(if error == PI {
            self.turn_sign * PI
        } else {
            error
        })
    }
    /// Rapidez angular real del ultimo paso (radianes/s).
    pub fn speed(&self) -> f64 {
        self.speed
    }
    /// Avanza `elapsed` segundos reales. `azimuth` valido actualiza el objetivo; `None` (sin
    /// direccion XZ) conserva el ultimo. Solucion exacta de `|e|' = -min(k |e|, vmax)` con el
    /// objetivo fijo durante el paso: independiente del troceo, sin pasarse y con rapidez media
    /// del paso nunca mayor que el tope. `elapsed` cero, negativo o no finito no avanza.
    pub fn update(&mut self, azimuth: Option<f64>, elapsed: f64) {
        if !(elapsed.is_finite() && elapsed > 0.0) {
            return;
        }
        if let Some(a) = azimuth.filter(|a| a.is_finite()) {
            self.goal = Some(wrap_angle(a));
        }
        let (Some(goal), Some(error)) = (self.effective_goal(), self.error()) else {
            self.speed = 0.0;
            return;
        };
        let k = 20.0_f64.ln() / HORIZONTAL_FOLLOW_T95_SECONDS;
        let knee = HORIZONTAL_FOLLOW_MAX_SPEED / k;
        let magnitude = error.abs();
        let linear = (magnitude - knee).max(0.0) / HORIZONTAL_FOLLOW_MAX_SPEED;
        let mut rest = if elapsed <= linear {
            magnitude - HORIZONTAL_FOLLOW_MAX_SPEED * elapsed
        } else {
            magnitude.min(knee) * (-k * (elapsed - linear)).exp()
        };
        if rest <= HORIZONTAL_FOLLOW_SNAP_RADIANS
            && magnitude <= HORIZONTAL_FOLLOW_MAX_SPEED * elapsed
        {
            rest = 0.0;
        }
        if magnitude == rest {
            self.speed = 0.0;
            return;
        }
        let sign = if error < 0.0 { -1.0 } else { 1.0 };
        let mut yaw = if rest == 0.0 {
            goal
        } else {
            wrap_angle(goal - sign * rest)
        };
        let mut distance = wrap_angle(yaw - self.yaw).abs();
        // Tiny timesteps amplify cancellation/rounding in goal - rest. Bound the pose,
        // not just the reported speed; keep the usual solution bit-exact when admissible.
        if distance / elapsed > HORIZONTAL_FOLLOW_MAX_SPEED * (1.0 + 1e-12) {
            // Retrocede el giro ya envuelto (lo que se guarda y se mide) ULP a ULP hasta que
            // el desplazamiento medido quepa sin pasarse. Cerca del corte +-pi el candidato
            // redondea <= 2 ULP y la medida <= 3 ULP, mas 1 por la division del tope: bastan
            // 6 pasos; 8 deja margen. Si antes cruza el punto de partida, no hay paso
            // representable con ese presupuesto y la pose se conserva.
            const BACKOFF_ULPS: u32 = 8;
            let step = (magnitude - rest).min(HORIZONTAL_FOLLOW_MAX_SPEED * elapsed);
            yaw = wrap_angle(self.yaw + sign * step);
            let mut admissible = false;
            for _ in 0..=BACKOFF_ULPS {
                let moved = wrap_angle(yaw - self.yaw);
                distance = moved.abs();
                if moved * sign < 0.0 {
                    break;
                }
                if distance / elapsed <= HORIZONTAL_FOLLOW_MAX_SPEED && distance <= magnitude {
                    admissible = true;
                    break;
                }
                yaw = wrap_angle(if sign > 0.0 {
                    yaw.next_down()
                } else {
                    yaw.next_up()
                });
            }
            if !admissible {
                self.speed = 0.0;
                return;
            }
        }
        self.speed = distance / elapsed;
        if distance > 0.0 {
            self.turn_sign = sign;
        }
        self.yaw = yaw;
    }
    /// Frente de reposo exacto y sin objetivo previo (presets); conserva `H`.
    pub fn reset(&mut self) {
        self.yaw = 0.0;
        self.goal = None;
        self.speed = 0.0;
    }
}

/// Transform de la criatura girada `yaw` (radianes, +Y del mundo) alrededor del centro fijo de
/// su colocacion: es exactamente el de una colocacion con yaw `placement.yaw() + yaw` en el mismo
/// centro (misma formula que `CreaturePlacement::transform_for`, Euler `(0, yaw total, 0)`):
/// misma escala, sin pitch ni alabeo, y la traslacion que deja el centro envolvente del modelo en
/// `placement.center()`. No copia ni toca vertices, normales ni UV. Con `yaw == 0` devuelve el
/// transform original exacto.
pub fn horizontal_transform(creature: &StaticCreature, yaw: f64) -> Transform {
    let base = creature.transform();
    if yaw == 0.0 {
        return base;
    }
    let placement = creature.placement();
    let total = placement.yaw() + yaw;
    let model_center = Vec3::from_array(creature.model().bounds_center);
    Transform {
        position: placement.center()
            - Mat4::rotation_y(total).transform_dir(model_center * base.scale.x),
        rotation: Vec3::new(0.0, total, 0.0),
        scale: base.scale,
    }
}

/// Punto del marco de reposo equivalente a `point` con el cuerpo girado `yaw` alrededor de +Y
/// por `center`: `center + Ry(-yaw) (point - center)` (inversa analitica; la altura no cambia).
/// El ojo apunta con su calibracion de reposo a este punto y, girado con el cuerpo, mira al
/// objetivo real. Con `yaw == 0`, `point` exacto.
pub fn rest_frame_point(center: Vec3, yaw: f64, point: Vec3) -> Vec3 {
    if yaw == 0.0 {
        return point;
    }
    center + Mat4::rotation_y(-yaw).transform_dir(point - center)
}

/// Lo que muestra el panel del seguimiento (QA/HUD).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HorizontalFollowStatus {
    pub follow: HorizontalFollow,
    /// Azimut actual del punto elegido (`None` sin direccion XZ o sin criatura).
    pub target_azimuth: Option<f64>,
    /// Hay criatura (sin ella no hay seguimiento).
    pub has_creature: bool,
}

/// Lineas ASCII en mayusculas (fuente 5x7), de 44 caracteres como mucho: `H`, presets, giro del
/// cuerpo y objetivo efectivo, error y rapidez reales, aviso sin direccion XZ y la regla de esta
/// candidata. Se dibujan con el panel generico de la candidata 01 (`draw_distant_turn_panel`).
pub fn horizontal_follow_lines(
    status: &HorizontalFollowStatus,
    last_preset: CreaturePreset,
) -> Vec<String> {
    // `+ 0.0` convierte -0.0 en 0.0 para no mostrar "-0.0".
    let deg = |r: f64| r.to_degrees() + 0.0;
    let follow = &status.follow;
    let mut lines = vec![
        if follow.is_active() {
            "SEGUIMIENTO HORIZONTAL: ON   H APAGA".to_string()
        } else {
            "SEGUIMIENTO: OFF  H ENCIENDE (AL FRENTE)".to_string()
        },
        format!(
            "7 CERCA  8 LEJOS  R REPITE {}",
            match last_preset {
                CreaturePreset::Near => "CERCA",
                CreaturePreset::Far => "LEJOS",
            }
        ),
    ];
    if !status.has_creature {
        lines.push("SIN CRIATURA: SIN SEGUIMIENTO".into());
        return lines;
    }
    lines.push(format!(
        "CUERPO YAW {:+.1}  OBJETIVO {}",
        deg(follow.yaw()),
        follow
            .effective_goal()
            .map_or("--".to_string(), |g| format!("{:+.1}", deg(g)))
    ));
    lines.push(format!(
        "ERROR {}  RAPIDEZ {:.1} GRADOS/S",
        follow
            .error()
            .map_or("--".to_string(), |e| format!("{:+.1}", deg(e))),
        deg(follow.speed())
    ));
    if status.target_azimuth.is_none() && follow.is_active() && follow.goal().is_some() {
        lines.push("SIN DIRECCION XZ: MANTIENE EL ULTIMO".into());
    }
    lines.push(format!(
        "REGLA: T95 {} S  TOPE {} GRADOS/S  SOLO YAW",
        HORIZONTAL_FOLLOW_T95_SECONDS,
        deg(HORIZONTAL_FOLLOW_MAX_SPEED).round()
    ));
    lines
}
