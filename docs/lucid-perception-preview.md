# Percepción alterada por oxígeno (UMB-310), preview aislado

**Estado:** primera hipótesis, **pendiente de decisión artística de Charlie**. Es un postproceso aislado: no está exportado en `visual/mod.rs` ni integrado a App, World ni CLI. El módulo `src/visual/lucid_perception.rs` solo lo compilan, mediante `#[path]`, `tests/lucid_perception.rs` y `examples/lucid_perception_preview.rs`.

- **No incluye** estrellas u ojos que observan, ni un umbral para ellos (confirmado para el futuro; el umbral sigue por decidir).
- **No añade** audio, HUD, consumo de O₂, modelos, enemigos ni colisiones.
- **No cambia** shaders, cámara, FOV, nave, Sol, criatura, la superficie lúcida ni el ojo del vacío.

## Interfaz

```rust
pub fn lucid_perception(source: &Framebuffer, oxygen01: f64, elapsed_time: f64) -> Result<Vec<u32>, String>
```

Postproceso RGB de un cuadro ya dibujado: devuelve un `Vec` **nuevo** y solo lee `source` (`&`; píxeles, dimensiones y z-buffer intactos). Toda la lectura sale del `source` inmutable; nunca remuestrea en sitio.

## Qué hace

| Tramo de O₂ | Efecto |
|---|---|
| `>= 0.5` | **Identidad exacta**, a cualquier tiempo |
| `0.5 → 0.3` | Advertencia: oscurecimiento periférico que crece de forma continua, estático en el tiempo |
| `< 0.3` | Además, deformación crítica: una onda radial de 3 lóbulos que gira, suave y acotada |
| `<= 0.05` | Oscurecimiento y deformación al máximo |

- **Centro legible:** dentro de una elipse protegida (radio normalizado `0.4`, con las esquinas en 1) no se toca ningún píxel. A 1280×720 son unos 724×408 px centrales, donde caen la nave, la mira y el planeta junto al que se arranca.
- **Periferia:** `keep = 1 − 0.7 · s(O₂) · w(r)`. Ambas funciones son `smoothstep`, con derivada nula en los extremos, sin saltos de umbral. Escala los tres canales por igual: no tiñe la pantalla de púrpura ni de rojo.
- **Deformación:** desplazamiento radial `A · w(r) · sin(3θ + 2π·t/2.5)`, con `A = 0.01 ×` el lado menor (7.2 px a 720p) multiplicada por la curva crítica. Se muestrea con interpolación bilineal y coordenadas acotadas. Es determinista, sin ruido ni aleatoriedad. La fase se reduce a una fracción del periodo antes de multiplicar por `2π`, evitando desbordar esa multiplicación; esto no recupera resolución temporal ya perdida en un tiempo enorme representado como `f64`. El oscurecimiento no late: sin flashes ni strobe.

### Parámetros (hipótesis inicial, NO aprobados)

| Constante | Valor | Nota |
|---|---|---|
| `PERCEPTION_ONSET_OXYGEN` | 0.5 | Alineado con el inicio de las grietas lúcidas (`LUCID_ONSET_OXYGEN`) |
| `PERCEPTION_FULL_OXYGEN` | 0.05 | Alineado con `LUCID_FULL_OXYGEN` |
| `DEFORM_ONSET_OXYGEN` | 0.3 | Inicio del tramo crítico |
| `CLEAR_RADIUS` | 0.4 | Elipse protegida |
| `VIGNETTE_MAX` | 0.7 | Oscurecimiento máximo en las esquinas |
| `DEFORM_MAX_FRACTION` / `DEFORM_LOBES` / `DEFORM_PERIOD_SECONDS` | 0.01 / 3 / 2.5 s | Deformación |

## Validación (antes de reservar o escribir)

Devuelve `Err` claro; no sanea a un valor válido, y valida incluso antes del atajo de identidad:
- **O₂** fuera de `[0, 1]` o no finito.
- **Tiempo** negativo o no finito. Son las mismas reglas que `--debug-oxygen` y `--time` del juego.
- **`Framebuffer` con campos públicos mutados:** dimensiones nulas, `width × height` que desborda (`checked_mul`) o supera `MAX_PIXELS`, o un `pixels.len()` distinto de `width × height`.

## Preview

```
cargo run --release --locked --offline --example lucid_perception_preview -- --output-dir <carpeta-nueva> [--time <s>]
```

**Escenas:**
- Usa `World` real, con la nave versionada y los setters aprobados (`set_lucid_surface`, `set_void_eye`).
- Dos encuadres: el arranque `--near` junto a Antena y junto a Vigilia.
- Cada escena se carga y se dibuja **una vez** con tiempo de mundo (1.3 s) y O₂ de mundo (1.0, sin grietas) **fijos**.
- El baseline es byte-idéntico a `umbral --headless --near N --time 1.3 --debug-oxygen 1 --lucid-surface --void-eye`.

**Salida:**
- Baseline sin postproceso, más un candidato al 100, 50, 25 y 5 % de O₂ del postproceso en dos fases: `t` y `t + 1.25 s`, medio periodo.
- Las imágenes son de 1280×720 y no llevan rótulos.
- `sequence/` contiene 24 cuadros por escena al 5 %: una vuelta de la onda, que se reproduce a 9.6 fps para verse en tiempo real.
- `manifest.json` (escrito solo con std) registra escenas, cámara, nave, O₂ y tiempos de mundo frente a los de postproceso, parámetros y métricas: píxeles distintos, píxeles cambiados en el centro (0) y luminancia de las esquinas.

**Rechazos:** argumentos o tiempo inválidos, duplicados, flags desconocidos o una carpeta existente fallan **antes** de escribir. `create_dir` no sobrescribe ni puede crear un alias de un input.

## Límites conocidos

- **Débil en el espacio:** la periferia de estos encuadres es casi toda fondo negro y oscurecer el negro no se ve. Se nota sobre el Sol en el borde de Antena; en Vigilia el planeta queda dentro de la zona protegida y el cambio casi no se percibe.
- **Bandeado:** sobre el fondo casi negro (`0x03040a`), el redondeo a 8 bits deja arcos tenues de un nivel.
- **Suavizado:** la interpolación bilineal suaviza un poco los bordes desplazados.
