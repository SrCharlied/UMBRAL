# GT1-CARRY: captura efectiva y arrastre (preview aislada, NO aprobada)

`visual::tentacle_grasp_carry` + `examples/tentacle_capture_carry_preview.rs`. Continúa la preview C
aprobada (persecución → B1 → P1) con la **captura** de la nave REAL y su **arrastre** hacia la criatura.
No la llaman `app`/`main`; el juego principal y la preview C no cambian.

## Ruta y orden de cada cuadro

`advance_capture_frame` es el cuadro COMPARTIDO por la ventana, el headless y las pruebas:

1. Teclas solares: con bloqueo se descartan R/7/8 (recolocan la nave). H y G se aplican.
2. `advance_perception_frame` con la entrada **efectiva**: la del jugador sin bloqueo y neutra con bloqueo.
   Avanza el O2, `World::update` (nave, cámara, tiempo) y la criatura con los segundos reales.
3. `CaptureCarry::frame` con `dt = min(elapsed, MAX_PHYSICS_DT)`:
   - sin bloqueo, delega en `GraspPreview` (C exacta);
   - con bloqueo, compone el tramo y hace el commit de la nave y la cámara.
4. Render con `body_override()` y percepción. Lo hace el llamador.

No hay doble avance: con bloqueo la nave ya no tiene velocidad ni inercia angular, y la física no la mueve.
La pose la fija el commit **después** de la física y **antes** del render. El tiempo del World, el cuerpo y
los efectos siguen avanzando; no se congela nada.

## Política

- **Antes del Gripped:** delegación exacta en C. Hay libertad de vuelo, cancelación por movimiento u O2,
  y el arte y los tiempos aprobados. Con el arrastre desactivado (`--legacy-c-sequence`) la secuencia
  reproduce la evidencia aprobada de C byte a byte (PNG y métricas, salvo el prefijo de checkout de la ruta
  de la nave del repo).
- **Bloqueo:** solo en el cuadro del evento Gripped vigente, y después de prevalidar la trayectoria entera
  (32 muestras con `compose` + `validate` de P1).
  - El primer cuadro es una copia exacta de buffers y pose de P1 168, y la capa se comprueba contra la de C.
  - La nave conserva sus bits de pose, pero velocidad e inercia angular pasan a 0
    (`World::commit_captured_ship_pose`, que usa `Ship::new` y reasigna orientación y ajustes).
  - Si se rechaza, queda el hold de C con el evento `CarryRejected`.
- **Arrastre (H1):** agarre autorado de 0,5417 s y arrastre de 3,5 s.
  - **Recorrido de la punta:**
    - `punta(q) = c + (t_lock − c)·(1 − q·(1 − r_end/r0))`, una recta hacia el centro del lazo de reposo;
    - `r_end = max(min_radius, |grip autorado del cuadro 'final' − c|) = 1,2095 OBJ`.
  - **Progreso:** `q` es la curva autorada |nave−origen| de 'arrastre' a 'final', monótona, con
    interpolación lineal.
  - **Forma:** la forma local cerrada 168 se compone con la memoria fija del bloqueo (`HeldClosure`), así que
    la pose es una función pura de `q`.
  - **Nave:** `S = B_actual · G(q) · (B0·G0)⁻¹ · S0`, con `G` = marco rígido del distal
    (`end_rotation`, `distal_center`). Conserva la relación aceptada y no impone una alineación nueva.
- **X:** suelta la nave, que queda en su pose sin inercia, y reinicia C desde el World actual. Si C rechaza
  el reinicio, la última malla queda como observación con la nave libre.
- **O2 > 15 % (o NaN):** queda SUELTA y la última malla se conserva como **observación**. Se revalida cada
  cuadro y la nave queda libre.
- **Errores:** `frame` devuelve `Err` de forma atómica: no cambia ni capa, ni reloj, ni nave, ni cámara,
  ni estado. `abort` es la acción explícita aparte: suelta la nave, la deja sin capa (sin render viejo) y
  en `Aborted`. El helper compartido aborta si el cuadro falla con capa propia. X siempre escapa.

## Mediciones previas (diagnósticos desechables d01b y d02)

- **Arte:** barre más de lo que retrae; |grip−lazo| va de 1,279 a 1,2095 y |nave−origen| de 1,19 a 0,65.
- **Bloqueo real:** punta a 4,11 OBJ y nave a 298 u.
- **H1 (elegida):** nave 298 → 90 u, margen ≥ 2,57, ~1° de azimut. Con H activo, el cuerpo deriva ~0,7°/s
  siguiendo a la nave sujeta.
- **H2 (rechazada):** recta hacia el grip autorado 265. La nave queda 46° fuera del frente y el seguimiento
  H gira sin fin a 18°/s, porque la nave sujeta gira con el cuerpo.

## Superficie (aditiva y retirable)

Cada bloque está anexado al final de su archivo. Quitarlo restaura el byte exacto de BASELINE (ver
`ADDITIVE_LEDGER.json`):

- `tentacle_grasp_closure.rs`: `HeldClosure` y `GraspClosure::held`, de solo lectura; reutilizan `compose`
  y `validate`.
- `tentacle_grasp_preview.rs`: `GraspPreview::closure`, de solo lectura.
- `world.rs`: `World::commit_captured_ship_pose`.
- `mod.rs`: una línea `pub mod`.

## Límites

- **Candidato sin aprobar:** la trayectoria, los tiempos y el extremo son candidatos derivados de datos.
- **Seguimiento H:** con H activo, la nave sujeta y el cuerpo derivan despacio para siempre; H OFF lo detiene.
- **Colisión:** no hay colisión garantizada con el cuerpo.
- **Vista de juego al 5 %:** la percepción aprobada y la cámara pegada a la espiral hacen que el gesto se
  lea mejor en las vistas de diagnóstico.
- **Precondición de C:** tras giros, el ciclo‑2 de cámara y cuerpo puede impedir la quietud; el remedio es
  R o H. R está bloqueada con la nave capturada.
- **Cobertura:** el headless ejercita las teclas por guion. Teclado físico y juicio visual quedan para
  Charlie.
