# Criatura estática en el juego (ruta debug, opt-in)

**Qué es:** una entidad opcional del `World` que dibuja el modelo texturizado de la criatura **dentro del juego real**. Usa el mismo framebuffer, el mismo z-buffer y la misma cámara de persecución de la nave que el resto de la escena, y se controla con los controles normales de la nave. Usa `draw_model_with` y `TexturedShader` (ver [`textured-preview.md`](textured-preview.md)).

**Qué no es:**
- **No usa el render del visor** ([`creature-viewer.md`](creature-viewer.md)): `render_view` limpia el cuadro y crea su propia cámara.
- **No es una ubicación final del nivel.**
- **No incluye** ojo que siga a la nave, oxígeno, revelación, animación, LOD, mipmaps, física ni colisiones.
- **No sustituye** al Sol ni a ningún planeta.

**Sin flags,** el juego es idéntico al anterior: misma imagen, mismos contadores, mismo comportamiento y ninguna lectura de archivos externos.

## Uso

```bash
cargo run --release -- --creature-model ruta/al/modelo.obj --creature-texture ruta/a/textura.png [--near-creature]
cargo run --release -- --headless --output salida.png --creature-model ... --creature-texture ... [--near-creature]
```

| Flag | Contrato |
|---|---|
| `--creature-model OBJ` y `--creature-texture PNG` | Siempre juntos. Activan la criatura, que se carga **una sola vez** al arrancar; el mundo toma posesión del modelo y de la textura. Mismos formatos que el preview: OBJ con su MTL al lado (no se lee `map_Kd`) y PNG RGB de 8 bits |
| `--near-creature` | Requiere la criatura. Arranca la nave frente a ella, mirándola. Con esta opción, `R` vuelve **a ese arranque**; sin ella, `R` restablece la nave como siempre |

**Rechazos antes de leer archivos o abrir la ventana** (código 1 y mensaje `Error: …`):
- Un par incompleto o un flag sin ruta.
- `--near-creature` sin la criatura.
- `--near N` junto con `--near-creature`: son dos arranques distintos.
- Cualquier flag de la criatura con `--legacy`.
- Con criatura, un `--output` que no es `.png` o que resuelve a la misma ruta que el modelo o la textura (`validate_output_path`).

**Si falla la carga,** el error nombra la ruta. Las rutas solo llegan por la CLI: no hay rutas fijas y el asset no está en Git.

**Controles:** los de siempre de la nave. No se añadieron controles orbitales del visor.

## Ubicación de demostración

| Parámetro | Valor |
|---|---|
| Centro de la esfera envolvente | `(0, 0, −900)` (`DEMO_CENTER`) |
| Radio en mundo | 80 (`DEMO_WORLD_RADIUS`) |
| Giro | 0: el frente original del OBJ (+Z) mira al Sol |

- **Separación:** la mayor extensión orbital es 540 (`max(orbit.radius + body_radius)`; Vigilia: 520 + 20). La esfera de la criatura empieza en 900 − 80 = 820, así que ningún cuerpo la toca en ningún instante. Lo comprueba una prueba con `SolarSystem::demo()`, sin suponer nada por el nombre del asset.
- **Normalización:** el OBJ no está centrado en su origen. La escala es uniforme, `radio_mundo / bounds_radius`, y la traslación compensa el centro envolvente girado y escalado, de modo que la esfera queda exactamente en el centro elegido.
- **Arranque `--near-creature`:** la nave queda en el lado del frente, a 2.5 radios del centro (`SPAWN_DISTANCE_RADII`), mirando al centro. Se calibró con la cámara de persecución real (FOV 60°, 22 detrás y 6 arriba): el ojo queda por encima de la nave y la esfera entera cabe en el FOV vertical (lo comprueba una prueba). A 3.0 radios la criatura se ve más pequeña; es una decisión de legibilidad debug, no artística.
- **Desde el arranque normal,** con la criatura cargada y sin `--near-creature`, la criatura queda **justo detrás del Sol**. Está dentro del frustum, así que paga vértices y rasterizado, pero el z-test la oculta por completo: la imagen es idéntica a la de sin criatura.

## Contratos (`src/visual/static_creature.rs`)

| Pieza | Contrato |
|---|---|
| `CreaturePlacement::new(center, world_radius, yaw)` | Componentes del centro en `[−1e6, 1e6]`, radio en `[1e-3, 1e5]` y yaw finito en `[−2π, 2π]`. Si no, `Err` |
| `transform_for(model_center, model_radius)` | Los bounds del modelo siguen el contrato puro `CreatureBounds` del visor (solo esa validación; no usa su cámara ni su render). Devuelve `Err` si el transform no es finito |
| `StaticCreature::new` / `load` | Toman posesión de `Model` y `Texture`. El transform se calcula **una vez** y no cambia con el tiempo ni con `World::update` |
| `World::set_creature` / `clear_creature` / `creature` | `World::new` no tiene criatura. `set_creature` comprueba antes que la luz real del mundo sirve al shader |
| `World::place_near_creature` | Coloca la nave en `spawn_pose()`. No cambia el tiempo ni los cuerpos y es repetible exactamente; sin criatura devuelve `Err` |

### Render en `World::render`

- **Orden:** cuerpos, criatura y nave, sobre el mismo framebuffer. La entidad **no limpia** color ni profundidad: compite por z-test con planetas y nave.
- **Luz real:** la del Sol (`SolarSystem::light_position()`, el origen), en modo `Lit` con ambiente 0.25. Sin luz de cabeza, sin tinte (el `Kd` se ignora) y sin el patrón de `LitPlanet`.
- **Doble cara siempre,** aunque el descarte global de caras traseras (F6) esté activo: la malla de la criatura no es cerrada (25 aristas de borde según la inspección en Blender), y descartar caras traseras abriría huecos.
- **Descarte por frustum antes de transformar,** con la esfera envolvente en mundo; respeta `--no-culling` y F3.
  - Descartada: cuenta 1 objeto enviado y descartado, y sus caras como enviadas y descartadas (como los cuerpos). Cero vértices y cero rasterizado.
  - Con `--no-culling` paga la etapa de vértices aunque los planos de recorte descarten todos sus triángulos.
- **Contadores:** se suman a los del mundo sin contar dos veces.

## Pruebas (`tests/static_creature.rs`, 13)

Todos los fixtures son sintéticos: un cubo descentrado con UV y normales, y una textura RGB de 2×2. No necesitan el asset ni abren ventanas.

- **Colocación:**
  - El modelo descentrado cae exactamente en la esfera elegida con 3 giros, con escala uniforme y el frente en `front()`.
  - Se rechazan valores inválidos, y los extremos válidos dan resultados finitos.
  - La demo queda fuera de todas las órbitas, con un margen de más de 100, y dentro del plano lejano.
- **Carga:** la criatura toma posesión de modelo y textura y el transform sale de los bounds. `load` nombra la ruta que falla y rechaza RGBA.
- **Flags:** `CreatureOptions::from_flags` es puro y no hace IO.
- **Binario** (siempre `--headless`):
  - Las combinaciones inválidas terminan con código 1 sin crear PNG; las rutas inexistentes demuestran que el error es de validación.
  - Una captura igual a la textura se rechaza y la textura queda intacta.
  - El cableado completo dibuja 6 objetos.
- **`World`:**
  - Sin criatura el mundo tiene 5 objetos, y quitar la criatura devuelve exactamente los mismos píxeles y contadores.
  - Visible: la imagen es idéntica a dibujar la criatura sobre el cuadro sin ella con el z-buffer intacto, la cámara de la nave, la luz del Sol y doble cara (0 caras traseras con F6 activo). Los contadores aditivos son exactos.
  - Fuera de vista: con descarte suma `[1, 1, 0, 12, 12, 0, 0, 0]`; con `--no-culling`, `[1, 0, 8, 12, 12, 0, 0, 0]`. En los dos casos la imagen y los fragmentos son idénticos.
  - Profundidad: detrás de un planeta se rasteriza pero no cambia ningún píxel; delante lo tapa, y solo cambian píxeles dentro de su círculo proyectado. La nave tapa a la criatura; sin la nave, la criatura ocupa esos píxeles.
  - Estática: el transform y la malla no cambian tras 120 `update` con controles, mientras la cámara sí se mueve.
  - `--near-creature`: la nave queda del lado del frente, a la distancia exacta, mirando al centro y con la esfera dentro del FOV. Repetirlo da el mismo arranque bit a bit, sin cambiar el tiempo ni los cuerpos.

## Medición

Tiempo real de `World::render` completo (limpiar, estrellas, cuerpos, criatura y nave) a 1280×720, con la criatura real (31 096 triángulos) y la textura candidata de 1024². Build release, en serie y sin otros trabajos; 2 calentamientos y 11 mediciones. Se midió con un *harness* desechable fuera del repositorio:

| Escena (controles reales de la nave) | Con criatura | Sin criatura |
|---|---|---|
| Arranque `--near-creature` (2.5 radios) | 59.8 ms | 5.1 ms |
| Cerca (1.47 radios, tras 2 s de empuje) | 82.7 ms | 4.9 ms |
| Fuera del frustum (tras girar) | 4.4 ms | 5.0 ms |
| Fuera del frustum, `--no-culling` | 9.5 ms | 6.4 ms |
| Arranque normal (oculta detrás del Sol) | 22.7 ms | 8.5 ms |

- **Con la criatura en vista, el cuadro del juego se vuelve mucho más lento.** Es un dato medido, no un perfil, y no atribuye el coste a una etapa concreta. Con estos números no se pueden prometer 60 FPS.
- **Fuera del frustum,** la diferencia está dentro del ruido. Sin descarte se pagan los vértices.
- **El FPS del overlay (F1) no sirve como benchmark para cuadros lentos:** `app.rs` limita `dt` a 0.1 s y lo suaviza. Esta tarea no lo corrige.

## Pendiente

- **Inspección visual y prueba manual de Charlie** en ventana (controles, `R` con `--near-creature`, legibilidad).
- **Decisiones de rendimiento** (LOD, mipmaps, malla reducida, ocultar detrás del Sol) y la **ubicación final** en el nivel.
- **Ojo que sigue a la nave, revelación por oxígeno, animación e integración con la textura o el material de Persona 1.**
- **Origen del modelo:** es externo, generado con Meshy AI. Su uso y su declaración dependen de la rúbrica y de los términos de Meshy, que no se han verificado aquí.
