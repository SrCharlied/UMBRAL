# Criatura estática en el juego (ruta debug, opt-in)

**Qué es:** una entidad opcional del `World` que dibuja el modelo texturizado de la criatura **dentro del juego real**. Usa el mismo framebuffer, el mismo z-buffer y la misma cámara de persecución de la nave que el resto de la escena, y se controla con los controles normales de la nave. Usa `draw_model_with` y `TexturedShader` (ver [`textured-preview.md`](textured-preview.md)).

**Qué no es:**
- **No usa el render del visor** ([`creature-viewer.md`](creature-viewer.md)): `render_view` limpia el cuadro y crea su propia cámara.
- **No es una ubicación final del nivel.**
- **No incluye** ojo que siga a la nave, animación, LOD, mipmaps, física ni colisiones. La **revelación por oxígeno** es otra opción aparte (`--creature-reveal`, ver [más abajo](#revelación-por-oxígeno-creature-reveal)); sin ella la criatura siempre es visible.
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

## Revelación por oxígeno (`--creature-reveal`)

Primera candidata, opt-in. Con poco oxígeno, la criatura aparece poco a poco como una presencia translúcida sobre la escena real. Si el oxígeno se recupera, vuelve a ocultarse gradualmente. Su aprobación artística está pendiente.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal [--debug-oxygen 0.25]
```

| Flag | Contrato |
|---|---|
| `--creature-reveal` | Requiere `--creature-model` y `--creature-texture`. **No** implica `--near-creature` ni mueve la nave. Con `--legacy` se rechaza |
| `--debug-oxygen X` | Oxígeno de depuración inicial, finito en `[0, 1]` (por defecto 1), fijado antes del primer cuadro. También vale sin criatura, para comparar al mismo O₂; el cuadro del juego normal no cambia. Con `--legacy` se rechaza. Las teclas `1`–`4` siguen igual: 1.0, 0.5, 0.25 y 0.05 |

Los dos se validan antes de leer archivos. Sin estos flags, el juego y la criatura estática no cambian.

Para recorrer el medidor manualmente, `M` aumenta el oxígeno y `N` lo reduce en **5 puntos
porcentuales por pulsación**, con límites de 0 % y 100 %. Mantener la tecla no repite pasos;
ambas teclas juntas se cancelan. El medidor se ajusta a porcentajes enteros al dar un paso,
sin deriva acumulativa en los umbrales. Los presets `1`–`4` se conservan y se aplican antes
de `M`/`N` si coinciden en un cuadro. El porcentaje actual aparece en el overlay (`F1`).

### Estado (`src/visual/creature_reveal.rs`)

- **Objetivo:** `reveal_target(O₂)` es un `smoothstep` continuo entre el umbral de aviso de UMB-310 (`start_oxygen` 0.50, oculta) y el crítico (`full_oxygen` 0.10, entera).

  | Tecla | O₂ | Objetivo |
  |---|---|---|
  | `1` | 1.00 | 0 |
  | `2` | 0.50 | 0 |
  | `3` | 0.25 | 0.684 |
  | `4` | 0.05 | 1 |

- **Transición:** `step_reveal` persigue el objetivo con tiempo **real**. Llega al 95 % en `rise_seconds` (2 s) al aparecer y en `fall_seconds` (0.7 s) al ocultarse. Los dos tiempos son ajustables en `RevealSettings::new` y se validan en `[0.01, 60]` s.
  - Cuando faltan menos de `REVEAL_SNAP_EPSILON` (0.001) para el objetivo, el valor se fija exactamente en él. Por eso 0 y 1 se alcanzan de verdad: ir de 0 a 1 tarda ~4.6 s y de 1 a 0, ~1.6 s.
  - Trocear el tiempo en cuadros no cambia el resultado (salvo ese ajuste final).
- **Tiempo real:** `World::update_creature_reveal(segundos)` es un paso visual separado de `World::update`. `app.rs` le pasa el tiempo real entre cuadros **sin** el límite de 0.1 s del `dt` físico.
  - El efecto no suma al tiempo orbital, `F5` no lo congela y la física y la cámara no cambian.
  - Un cuadro muy largo, por ejemplo al arrastrar la ventana, lo lleva casi directo al objetivo.
- **Estado puro:** solo guarda los ajustes y la visibilidad mostrada. No depende del tiempo absoluto, y la malla, el transform y la textura nunca cambian.
- **Saltos al objetivo sin transición:** al activar el modo, con `set_creature`, `R` (`reset_ship` o `place_near_creature`), `place_near` y `snap_creature_reveal`. `R` conserva el oxígeno. En headless se arranca ya en el objetivo, así que las capturas son deterministas.
  - `clear_creature` conserva el modo, y `disable_creature_reveal` vuelve a la criatura siempre visible.
- **Política para valores no finitos:**
  - Un O₂ no finito deja la criatura oculta.
  - Un objetivo no finito se trata como 0.
  - Una visibilidad no finita salta al objetivo.
  - Un tiempo NaN, negativo o cero no avanza; un tiempo infinito llega al objetivo.
  - Todo resultado es finito y está en `[0, 1]`.

### Render

| Visibilidad | Qué se dibuja |
|---|---|
| 0 | **Nada**: ni se rasteriza, ni se escribe profundidad, ni se cuenta. Píxeles y `RenderStats` son idénticos a no tener criatura |
| 1 | La ruta estática de siempre, en el mismo orden (antes de la nave). Píxeles y contadores son idénticos a la variante estática |
| Entre 0 y 1 | `RevealCompositor`, un **fantasma de una sola capa** dibujado **después** de cuerpos, estrellas y nave |

Pasos del compositor en la visibilidad parcial:
1. Copia los píxeles del cuadro real.
2. Dibuja la criatura opaca e iluminada, como la estática, con el z-buffer real.
3. Anota en una máscara los píxeles cuyos fragmentos aceptaron el z-test. En `triangle_shaded`, sombrear implica escribir, así que cada píxel anotado termina con la superficie más cercana.
4. Mezcla **una sola vez por píxel**, en luz lineal: `fondo + (criatura − fondo) · visibilidad`. Usa `srgb8_to_linear`, inversa exacta de `to_rgb8`, comprobada en los 256 códigos.

Qué se ve con este compositor:
- Lo que está delante de la criatura (nave o planeta) queda intacto.
- Lo que está detrás (estrellas, planetas o la nave) se ve a través de ella.

Límites:
- **Sobre el espacio casi negro, la mezcla se percibe sobre todo como atenuación.** La translucidez solo se distingue donde hay estrellas, planetas o la nave detrás. Al mezclar en lineal, el brillo percibido crece rápido: con visibilidad 0.68 (tecla `3`) queda cerca del 85 % del de la estática (≈ 0.68^(1/2.4)).
- **No es transparencia física de varias capas.** Las partes internas de la criatura no se ven a través de su superficie frontal.
- **Tiene que ser lo último que se dibuja en 3D:** el z-buffer queda con la profundidad de la criatura. Una entidad que en el futuro se dibuje después la trataría como opaca. No es una transparencia general ni corrige la profundidad para otras entidades.
- **Contadores:** son los del cuadro sin criatura más los de dibujar la criatura al final. La mezcla no suma escrituras. Los contadores aditivos son iguales a los de la estática; los fragmentos y los píxeles escritos cambian porque cambia el orden.
- **Frustum:** descartada por el frustum, se cuenta como la estática descartada y no copia nada.
- **Buffers:** dentro de `World`, en un `RefCell`, y reutilizados entre cuadros. Siguen la resolución del framebuffer y no se reservan con visibilidad 0 o 1 ni con la criatura descartada. Ocupan unos 5 bytes por píxel (copia de color y marca) más 4 por píxel cubierto: unos 4.6 MB a 1280×720 más la máscara. `World::creature_reveal_buffer_bytes` informa del total.
- **Costo:** con visibilidad parcial se rasteriza lo mismo que la criatura estática, más la copia y la mezcla. **No está medido.**

### Pruebas (`tests/creature_reveal.rs`, 17)

Todos los fixtures son sintéticos (un cubo y dos cuadrados apilados de colores únicos); no hace falta el asset ni se abren ventanas.

- **Ajustes y objetivo:** validación, valores de las teclas, monotonía, recorte y valores no finitos.
- **Paso temporal:** 95 % en los segundos configurados, llegada exacta a los extremos, monotonía sin pasarse, composición en N trozos, un cuadro de 0.25 s sin límite y entradas no finitas.
- **Mezcla:** inversa sRGB exacta y mezcla lineal (distinta de mezclar en sRGB).
- **`World`:**
  - **Extremos:** sin modo, todo queda igual; oculta es igual a no tener criatura y entera es igual a la estática, con píxeles y contadores.
  - **Transiciones:** usan tiempo real, sin `update` y sin `F5`.
  - **Saltos y cambios de criatura:** los recolocados conservan el O₂ y saltan al objetivo; quitar y poner la criatura funciona como se describe arriba.
- **Compositor:**
  - En cada píxel, el resultado es exactamente la mezcla lineal del cuadro sin criatura con el de la criatura opaca.
  - Las caras superpuestas se mezclan una sola vez.
  - La nave y un planeta, delante o detrás, se comportan como se describe arriba.
  - Los contadores no suman escrituras de la mezcla.
  - Los buffers solo existen en la visibilidad parcial visible, siguen la resolución y se reutilizan.
- **Binario** (siempre `--headless`): combinaciones inválidas rechazadas sin crear PNG, protección de `--output`, parcial al 68 %, oculta con 5 objetos, entera idéntica byte a byte a la estática sin mover la nave, y `--debug-oxygen` sin criatura idéntico al juego normal.

## Revelación por etapas (`--creature-reveal-stages`)

Candidata opt-in, con la **variante B** que eligió Charlie («ojo con un poco de cuenca»): primero aparece el ojo y después el cuerpo. La revelación uniforme (`--creature-reveal`) se conserva tal cual como otra variante. Los dos modos son excluyentes.

```bash
cargo run --release -- --creature-model ... --creature-texture ... --near-creature --creature-reveal-stages [--debug-oxygen 0.25]
```

| Flag | Contrato |
|---|---|
| `--creature-reveal-stages` | Requiere `--creature-model` y `--creature-texture`. Se rechaza con `--legacy` y con `--creature-reveal` («elige un modo»). **No** implica `--near-creature` ni mueve la nave |

- `--debug-oxygen`, las teclas `1`–`4` y `M`/`N` funcionan igual que con la revelación uniforme.
- Las combinaciones se validan antes de leer archivos.
- La región del ojo se valida al cargar: **falla cerrada** si no toma ninguna cara o las toma todas.

### Identidad del ojo (`src/visual/staged_creature_reveal.rs`)

- **`CREATURE_EYE_REGION`:** son del ojo **todas las caras** cuyo centroide, en espacio de objeto del OBJ, queda a menos de **0.20** de **(0.049, 0.059, 0.311)**. No hay clasificación por color.
  - Con el asset actual da 291 caras de ojo y 30 805 de cuerpo.
  - **Está calibrada para este asset Meshy:** otro modelo o un remesh exigen recalibrar.
- **`StagedParts::split`** parte el modelo **una vez**: al activar el modo o en `set_creature`.
  - Las dos partes copian vértices, UV, normales, materiales y bounds originales, así que el transform es exactamente el de la criatura. Las caras se reparten sin solaparse y sin perder ninguna.
  - No guardan textura: dibujan con la de la `StaticCreature` original, prestada.
  - **Memoria de la caché:** dos copias de los arreglos más las caras repartidas. Con el asset actual, `cache_bytes()` informa 7 079 504 bytes (≈ 7.1 MB). Es una duplicación consciente, sin optimizar.

### Etapas (candidatas, no aprobadas)

`StagedRevealSettings` se valida al construirse:
- `0 ≤ full < hint < eye_full < start ≤ 1`.
- `hint_alpha` en `[0, 1)`.
- Tiempos como en V1.

| O₂ | Ojo | Cuerpo |
|---|---|---|
| ≥ 50 % (`start`) | 0 | 0 |
| 50 → 30 % (`eye_full`) | 0 → 1 (smoothstep) | 0 |
| 30 → 25 % (`hint`) | 1 | 0 → **0.02** (smoothstep): contorno muy tenue |
| 25 → 5 % (`full`) | 1 | 0.02 → 1 (smoothstep) |
| ≤ 5 % | 1 | 1: ruta estática exacta |

- **Un único progreso normalizado** `p` en `[0, 1]` sigue al O₂ de forma lineal por tramos. Los dos alfas salen de `p`, así que **el cuerpo solo es mayor que 0 con el ojo exactamente en 1**, también mientras transiciona (al aparecer y al recuperar O₂).
- **Transición:** `p` avanza con `step_reveal` de V1, 2 s al aparecer y 0.7 s al ocultarse (95 %), con tiempo real. La aplica `World::update_creature_reveal`, como en V1.
- **Saltos al objetivo:** `R`, `place_near` y `set_creature` conservan el O₂ y saltan a su objetivo.
- **Alfas:** son **opacidad lineal** del compositor, no brillo percibido. Sobre el espacio casi negro, un 2 % lineal se percibe como algo más: `≈ 0.02^(1/2.4)` ≈ 20 % del brillo.

### Render

| Estado | Qué se dibuja |
|---|---|
| Ambos en 0 | Nada: píxeles y `RenderStats` idénticos a no tener criatura |
| Ambos en 1 | La ruta estática de siempre (antes de la nave): píxeles y contadores exactos |
| Parcial | **Después** de cuerpos, estrellas y nave: el ojo con `RevealCompositor::render_part` y, solo con el ojo opaco, el cuerpo |

Lo que garantiza ese orden en el parcial:
- **Con el cuerpo en 0, el cuerpo no se dibuja.** La banda púrpura que está delante del ojo no abre una raya negra; el ojo se ve completo.
- **Con el cuerpo a medias:**
  - El cuerpo que está **delante** del ojo se mezcla **sobre el ojo opaco**.
  - El cuerpo que está **detrás** queda tapado por el ojo.
  - Dentro del cuerpo sigue habiendo una sola capa, como en V1.
- **Lo último en 3D:** igual que V1.

Descarte, contadores y buffers:
- **Descarte:** se usa la esfera de la **criatura completa** antes de tocar las partes. Si queda fuera, se cuenta como la estática descartada y no se copia nada. Respeta `--no-culling` y F3.
- **Contadores:** **una** criatura lógica (`objects_submitted`). Los vértices, triángulos y fragmentos son los que dibujan realmente las partes: con las dos, los vértices del modelo cuentan dos veces porque cada parte los transforma. La mezcla no suma escrituras.
- **Buffers:** los del compositor de V1, reutilizados entre cuadros.

### Compositor generalizado (`src/visual/creature_reveal.rs`)

- `RevealPart { model, transform, texture }` es una vista prestada.
- `RevealCompositor::render_part` hace lo siguiente según alfa:

  | Alfa | Resultado |
  |---|---|
  | 0 | Nada |
  | 1 | Opaco, igual que `StaticCreature::render` |
  | Parcial | El mismo fantasma de V1 |

- Así se dibujan subconjuntos sin copiar la textura ni tocar el renderer.
- `RevealCompositor::render` mantiene su firma y su comportamiento exactos.

### Pruebas (`tests/staged_creature_reveal.rs`, 17)

Todos los fixtures son sintéticos.

- **Ajustes:** valores hostiles y por defecto.
- **Progreso:** extremos, teclas (25 % → ojo 1 y cuerpo 0.02), monotonía, nunca cuerpo con ojo parcial (también al transicionar), tiempo real como V1.
- **ROI y partición:** validación de la ROI, partición disjunta y completa sin tocar la geometría, y fallo cerrado.
- **Compositor:** `render_part` igual a las rutas de V1.
- **`World`:**
  - Extremos exactos.
  - Fase del ojo sin raya.
  - Contorno al 25 % y cuerpo sobre el ojo.
  - Contadores de una criatura lógica.
  - Descarte con y sin culling.
- **Ciclo de vida:** caché, fallos que no cambian nada, exclusión con V1, saltos al objetivo con `R`, buffers.
- **Binario:** validación, fallo cerrado de la ROI con otro modelo, 25 % con 6 objetos, 100 % con 5, y 5 % idéntico a la estática.

## Pendiente

- **Inspección visual y prueba manual de Charlie** en ventana (controles, `R` con `--near-creature`, legibilidad, y la revelación con las teclas `1`–`4`).
- **Decisiones de rendimiento** (LOD, mipmaps, malla reducida, ocultar detrás del Sol) y la **ubicación final** en el nivel.
- **Ojo que sigue a la nave, animación e integración con la textura o el material de Persona 1.** Integración de la revelación con el oxígeno real de UMB-205 y con `LucidSettings` (UMB-310).
- **Origen del modelo:** es externo, generado con Meshy AI. Su uso y su declaración dependen de la rúbrica y de los términos de Meshy, que no se han verificado aquí.
