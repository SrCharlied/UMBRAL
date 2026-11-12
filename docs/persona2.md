# Persona 2: navegación y gameplay

Entrega integrada de Hito 2 (UMB-201..208), verificada el 2026-11-04 en
`feat/umb-persona2-gameplay`. Persona 2 aporta control progresivo, asistencia
orbital, colisiones, gravedad opcional, oxígeno, recursos, registros y estados
de partida, conectados a `World` y a los controles de aplicación. Consume las
APIs de cámara, escena y renderer del equipo; shaders, procedural, modelos y
HUD artístico corresponden a las otras contribuciones.

## Contratos actuales

- `World::new(model, aspect) -> Result<World, String>` construye el mundo real.
- `World::update(&ShipInput, dt)` integra gameplay, nave, navegación, colisiones,
  recolección y cámara. Tiempo en segundos; posiciones en unidades de mundo.
- `World::render(&mut Framebuffer) -> RenderStats` ejecuta el renderer CPU real.
- `World::reset_ship()` reinicia partida, catálogos, reloj, cámara y overrides.
- `World::place_near(index, surface_gap)` es colocación para demostración.
- `World::oxygen01()` entrega el valor visual; `gameplay.oxygen01` es el real.
  `set_debug_oxygen(value)` y `clear_debug_oxygen()` controlan el override visual.
- `GameplayState` expone oxígeno normalizado, conteos, estados terminales,
  `resources`, `logs`, eventos del cuadro y `last_log` persistente.
- `GameplayState::tick(dt)`, `collect_near(position, system, time)` y
  `collect_along(start, end, system, start_time, end_time)` permiten pruebas
  aisladas; `tests/persona2_integration.rs` verifica la integración usando `World`.
- `Collectible::position(system, time)` obtiene el ancla orbital y axial.
- `NavigationState` ofrece `apply_gravity`, `assist`, `resolve_collisions`,
  `assisted_planet` y `gravity_velocity`.
- La aplicación expone `GameplayAction`,
  `apply_gameplay_action` y `gameplay_feedback_lines`. Controles O/R/1-4/0
  corresponden a asistencia, reinicio y oxígeno visual/real; su prueba está
  en `tests/persona2_app.rs`.

## Parámetros observados

| Configuración | Valores predeterminados | Unidades/efecto |
|---|---|---|
| `ShipSettings` | `max_speed=60`, `boost_multiplier=5`, `reverse_fraction=0.25` | velocidad, multiplicador y fracción de reversa |
| Respuesta nave | `speed_response=1.5`, `turn_rate=1.4`, `turn_response=6` | 1/s, rad/s, 1/s |
| `GameplaySettings` | `drain_per_second=1/420`, `pickup_recovery=0.15` | fracción de oxígeno/s; recuperación normalizada |
| Proximidad | `pickup_radius=8`, `sun_arrival_gap=30` | radio y distancia a superficie solar |
| `NavigationSettings` | `ship_radius=2`, `orbit_gap=20`, `orbit_capture_gap=100` | unidades de mundo |
| Asistencia | `orbit_angular_speed=0.45`, `orbit_response=2` | rad/s y 1/s |
| Gravedad opcional | `gravity_strength=0`, `max_gravity_acceleration=30` | desactivada por defecto; aceleración acotada |

Para demostración individual: cambiar `drain_per_second` altera duración de la
partida; cambiar respuesta/velocidad altera navegación; `orbit_assist` activa
el seguimiento orbital. No requiere modificar shaders ni renderer.

## Cobertura de aceptación

| Historia | Evidencia automatizada / límite |
|---|---|
| UMB-201 | Suite existente de nave y navegación; comparación FPS y frenado. FOV se prueba fuera del sistema para aislar el lente. |
| UMB-202 | `World` completa más de 2π de ángulo firmado a 30 y 144 FPS siguiendo un planeta móvil; conserva radio, cámara y salida manual. Exige mirada al centro `dot > .99` y `up` no degenerado. |
| UMB-203 | Contacto, separación, barrido relativo contra cuerpos móviles y selección del primer impacto en `tests/navigation.rs`. |
| UMB-204 | Gravedad optativa acotada; sin impulso oculto durante asistencia, cubierta por pruebas de navegación. |
| UMB-205 | 420 segundos con pasos reales a 30/144 FPS; mitad de oxígeno a 210 s, muerte a 420 s y bloqueo terminal. |
| UMB-206 | Recuperación exacta, evento único, contador único y una instancia menos enviada al renderer. Regresión adicional para cruce en boost entre extremos de un cuadro. |
| UMB-207 | Las seis permutaciones de registros mediante updates de `World`; eventos, último registro y conteos. |
| UMB-208 | Sol no gana antes de los tres registros; victoria congela estado. Tres reinicios sin duplicar catálogos ni conservar progreso/cámara/override. Pasos grandes de muerte y victoria terminal. |

## Evidencia ejecutada y TDD

Comandos ejecutados desde la raíz, Rust `1.99.1`:

```powershell
cargo test --locked --test world el_campo_de_vision_se_abre_con_la_velocidad_y_vuelve_al_frenar -- --exact --nocapture
cargo test --locked --test persona2_integration -- --nocapture
cargo test --locked --test persona2_integration world_boost_crossing_collects_resource_between_frame_endpoints -- --exact --nocapture
cargo test --locked --test persona2_integration world_assist_circles_moving_planet_with_camera_at_both_frame_rates -- --exact --nocapture
cargo test --locked
cargo test --locked --no-fail-fast
cargo clippy --all-targets --locked -- -D warnings
rustfmt --edition 2024 tests/persona2_integration.rs
```

FOV: primero falló la aserción original de apertura. La regresión diagnóstica
confirmó 431 cuadros de contacto solar y velocidad cero: la recta original
`(0,40,700)` hacia `-Z` pasa a 40 unidades del Sol de radio 60 (más nave 2).
Se cambió exclusivamente el setup a `(2000,2000,2000)` hacia `+X`;
la prueba pasó con ambas aserciones originales intactas.

Smoke determinista: tres ejecuciones, cada una con **43 updates y 37 renders**
CPU a framebuffer en memoria de 160×90, sin ventana. El setup coloca
explícitamente nave en cápsulas, registros y vecindad solar; no representa
pilotaje humano. Cada ejecución recoge tres recursos y tres registros y gana.
Compara todos los hashes intermedios, estadísticas, framebuffer final, reloj
y oxígeno entre ejecuciones. Primer hash observado `872bf9c1c3403f48`;
render final escribió 9109 píxeles. No se exportó imagen ni se afirma evidencia
visual o recorrido manual. Los hashes comparan repeticiones en el mismo proceso,
no constituyen golden portable entre plataformas.

## Estado verificado y pendientes

Las regresiones primero reprodujeron dos fallos: pickup cruzado no recogido
(contador 0 en vez de 1) y cámara tangencial (`dot=0.5434975894921991` en vez de
`> .99`). Tras conectar `World::update` a `collect_along` y orientar cámara y
`up` hacia el planeta asistido, ambas pasan a 30/144 FPS. Los casos límite
de navegación previamente reportados también pasan.

- Integración: **9/9 pasan**. Smoke repetido tres veces con los mismos valores
  documentados arriba. FOV: **14/14 pruebas de World pasan** en la suite.
- Suite completa `cargo test --locked --no-fail-fast`: pasó tras integrar
  cámara/barrido; incluye gameplay, navegación, aplicación y pruebas baseline.
- Pasos grandes: con `dt=421`, el consumo agota oxígeno antes de movimiento y
  recolección; impide recuperación por cápsula y victoria aun con tres registros
  junto al Sol. Una victoria ya alcanzada conserva pose, cámara, reloj y oxígeno
  con `dt=10000`. Esta prioridad es el contrato actual, sin simulación de
  eventos parciales dentro de un paso que agote el oxígeno.
- Último Clippy estricto: falla por `collapsible_if` en
  `src/scene/world.rs:198`. Antes también se observaron warnings baseline
  `chunks_exact_to_as_chunks` en `tests/ship_baseline.rs` y `tests/app.rs`;
  queda pendiente comprobar Clippy tras la limpieza final de integración.
- La recolección aproxima la trayectoria del ancla por un segmento entre
  instantes; los pasos grandes no equivalen a pilotaje continuo ni garantizan
  una órbita curva exacta entre extremos.
- Pendiente: validación de pilotaje humano, evidencia visual/video, revisión
  de presentación y comprobación desde clon limpio. Las ejecuciones aquí
  registradas verifican el árbol compartido, sin certificar un commit congelado.
