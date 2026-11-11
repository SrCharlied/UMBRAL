# Persecución espacial 3D del tentáculo (UMB-311, prototipo opt-in)

Variante **espacial** de la persecución viva: el brazo aprobado persigue la **posición actual** de la
nave, con una deformación 3D continua en el tiempo, también por detrás de la criatura.

**Es un prototipo de persecución. NO es captura:**

- sin agarre, arrastre, bloqueo de input, muerte, audio, apagón, explosión, cooldown ni colisiones;
- el tentáculo **nunca mueve la nave ni la cámara**;
- sin el flag, el juego no cambia; la persecución viva plana (`--tentacle-live-pursuit`) y la
  composición observadora (`--tentacle-capture-debug`) siguen como estaban.

El 0 % de O₂ sigue **indeciso**: aquí solo se sostiene la rapidez del 5 %.

**Código:** `src/visual/tentacle_spatial_pursuit.rs`.

## Uso

```text
umbral --creature-model <Meshy.obj> --creature-texture <texture_1024_linear_box_diagnostic.png> \
  --creature-solar-reveal [--lucid-surface --void-eye --lucid-perception --watching-stars] \
  --tentacle-spatial-pursuit --tentacle-capture-package <capture_v2> \
  --tentacle-growth-package <growth_package.txt> --tentacle-arm-correspondence <arm_correspondence.txt>
```

- **Validación previa a cualquier IO:** requiere `--creature-solar-reveal`; incompatible con
  `--tentacle-live-pursuit`, `--tentacle-capture-debug` (F7) y `--tentacle-capture-time`; no admite
  `--legacy`; exige las tres rutas.
- **Protección de entradas:** la misma que la composición (grafo SC01-MTL, FileID, RAII y preflight antes
  de cada guardado headless o `P`).
- **Controles:** los de siempre (O₂ con `1`–`4` y `M`/`N`; `H` seguimiento horizontal). Estado en el título.

## Controlador (compartido, sin cambios)

Es el mismo `PursuitController` de la persecución viva: punta en mundo, nave **actual**, rapidez por O₂
12/24/48 u/s (15/10/5 %, candidatos, no retocados), aceleración 48 u/s², llegada exacta sin pasarse
(`DECISION_LLEGADA_CHARLIE.json`), recogida a 24 u/s y reposo exacto. Solo cambia la **bola mínima**: la
del dominio exacto de esta familia (abajo).

## Deformación GEO-SPATIAL-01: codo espacial con memoria de lado

Topología aprobada 4801/9585 como dato (sin reexportar). Proximal y raíz en las posiciones **exactas** del
Meshy (bit a bit). Anillos = copias **rígidas** del lazo P. Distal y espiral **rígidos** con el marco final
real (misma anatomía, UV, material y grosor).

**Curva** (OBJ, desde el lazo P con tangente `t`, radio de codo `ρ = 2 ×` contorno):

1. **Primer codo** de radio `ρ` hacia el **lado de memoria** `m` (unitario, ⟂ `t`), con giro
   `θ₁ = f · θ_plano(punta)`; `θ_plano` es el giro del camino plano equivalente y depende solo de la
   distancia axial y transversal de la punta, así que es continuo también en el polo trasero.
   `f = SPATIAL_FIRST_ELBOW_FRACTION = 0.5` (candidato).
2. **Camino plano codo + recta** (Dubins) desde el final del primer codo, con su tangente, hasta la punta.
   La recta final cubre al distal: punta axial = centro distal + `a ·` tangente final.

Si `m` coincide con el lado de la punta, la curva es **exactamente** la plana de la familia anterior. Si
no, es una curva 3D (los dos codos en planos distintos).

**Memoria de lado (ajuste A1, «perezosa por margen»):** `m` gira alrededor de `t` hacia el lado de la
punta solo lo **mínimo** para mantener el margen a la singularidad del segundo codo ≥ 50°
(`SPATIAL_SAFE_MARGIN`), más una deriva lenta de `1 · |ΔT| / |T|` (`SPATIAL_SIDE_DRIFT`), y nunca más de
`8 · |ΔT| / |T|` por paso (`SPATIAL_SIDE_RATE`). Todos son candidatos. Sin memoria (primer cuadro) toma el
lado de la punta o, sobre el eje, una perpendicular determinista. Cerca del polo trasero el margen sobra
(≈ 90°) y `m` casi no gira: la curva depende con continuidad de (punta, `m`). Con `dt = 0` la memoria
no cambia ni un bit. Al volver al reposo la memoria se borra.

La primera ley (girar siempre hasta `8 · |ΔT| / |T|` hacia la punta) pasaba el fixture sintético, pero en
el brazo real el distal y la espiral convierten cada radián de memoria en ≈ 1,5 OBJ de movimiento
(ganancia 19 en LP-R01). A1 es el único ajuste nombrado de la hipótesis.

**Corrección H-FIX01 (reintento acotado, 1/1): memoria acotada por el presupuesto de continuidad.** A1
mantenía el margen en 50° cambiando de golpe de la deriva (1 · |ΔT|/|T|) a un giro de mantenimiento de hasta
2,6 · |ΔT|/|T| con la memoria ≈ 141° por detrás; la espiral rígida convertía ese ritmo en ganancia de malla
≈ 10 (medido; ver `fix01/evidence/DIAGNOSTICO_CAUSA.json`). Ahora el giro que decide A1 se **acota** para que
el desplazamiento máximo de la malla que causa la memoria (todos los vértices, a la punta nueva) no supere
`SPATIAL_CONTINUITY_GAIN · |ΔT|` (= 8, la cota del contrato LP-R01/R02) menos el desplazamiento que causa
la punta sola. Por la desigualdad triangular el salto del cuadro queda dentro de la cota (más un redondeo
del orden de 1e-15). Solo se aceptan giros verificados; sin presupuesto, la memoria no gira ese cuadro. A1,
la familia, el dominio y los demás candidatos no cambian. Coste: 2–3 evaluaciones de malla por cuadro con
giro (sin medir rendimiento).

**Marcos:** rotaciones compuestas de los dos codos (mínimas para arcos circulares). No hay `bend_side` ni
`side_switched`: no hay ramas. La pose (`SpatialPose`) publica los dos giros, el giro total, la rotación
real del distal, el giro de la memoria en el cuadro y el **margen a la singularidad** del segundo codo
(`π −` ángulo entre la tangente tras el primer codo y la punta).

**Por qué es continua (y la plana no):** la familia plana obliga a que el plano contenga `t` y la punta;
en el polo trasero su normal no tiene límite (LP-R01) y sus dos ramas difieren en el umbral (LP-R02). Aquí
la dirección del primer codo es la memoria (no se normaliza el lado de la punta), y el segundo codo nunca
mira hacia atrás mientras la memoria siga a la punta: en los barridos medidos el margen fue ≥ 82°.

## Dominio declarado (no «cualquier distancia/dirección»)

- **Bola mínima** `3ρ + sqrt(ρ² + a²)` (≈ 0,62 OBJ ≈ 44 u en el asset real; la plana era ≈ 0,40 OBJ):
  el final del primer codo dista ≤ `2ρ` del lazo, así que desde ahí la recta final es ≥ `a` y la punta es
  **exacta para cualquier memoria**. Más cerca, solo sobre el eje de reposo (salida y recogida).
- **Alcance** 12 OBJ (≈ 856 u) como antes; fuera se recorta el objetivo y el título lo dice.
- **Errores atómicos:** punta/memoria no finitas, memoria paralela a `t`, punta fuera de `[a, 12]`,
  marcos no rígidos, vértices/normales no finitos, normales que cambian de norma o anillos no rígidos:
  `Err` antes de escribir; nada cambia.

## Composición

Igual que la persecución viva: `SpatialPursuit` reutiliza `SolarCaptureDebug` (carga, identidad y `bind`
preparados sin publicar), escribe el tramo en una **copia** de la capa `Body` y la dibuja con el mismo
hook (mismo shader, textura 1024 y alfa del cuerpo; escena → ojos de estrellas → overlays → percepción).
En reposo no hay capa: el cuadro es el de siempre. Sin E/S ni recarga por cuadro.

## Estado medido (fix01, NO aceptación)

Brazo REAL (Meshy), todos los vértices, criterio heredado `salto <= 8 · paso + 1e-5`:

| Barrido | Antes (A1) | Con H-FIX01 |
|---|---|---|
| Prueba real original (LP-R01, vuelta de azimut, LP-R02 en r 0,62/1/3) | 66 violaciones, ganancia 9,96 | 0 violaciones, ganancia máx. 7,97 |
| Regresión nueva (historia del padre, r 0,62/0,75/1, memoria opuesta/perpendicular, planos vertical/diagonal; 34 421 cuadros) | 621 violaciones, ganancia máx. 10,31 | 0 violaciones, ganancia máx. 7,93 |

En la ráfaga el margen a la singularidad baja como mucho a 49,5° (la memoria gira a ≈ 1,8 · |ΔT|/|T|). La
cota se cumple **por construcción y casi sin holgura** (≈ 7,9–7,97): no es una prueba de continuidad del
algoritmo en todo el dominio. El plano heredado en el mismo brazo salta 1,44 OBJ para un paso de 1,4e-5.

**sp10** (plazo interno: la memoria alcanza el lado de la punta en < 60° de recorrido) sigue RED: la memoria
es perezosa y ahora además está acotada. Sus cláusulas de inicialización, tope, perpendicularidad y mismo
estado están cubiertas por una guarda añadida; la migración del plazo es decisión del padre.

## Límites declarados

- **Margen a la singularidad sin suelo garantizado:** si en una trayectoria no medida la memoria quedara
  acotada mucho tiempo, el margen podría seguir bajando y la ganancia de la punta sola crecer; si esta sola
  supera 8, la memoria se congela (no hay respaldo).

- Prototipo de persecución: sin agarre ni arrastre; no mueve la nave; 0 % indeciso.
- **Autocolisiones** globales (tramo/distal contra cuerpo) no se tratan.
- La memoria hace que la forma dependa del **camino** de la punta (no solo de su posición): dos visitas al
  mismo punto pueden dar codos distintos, sin saltos entre cuadros.
- Con la memoria a destiempo (la nave cruza detrás del cuerpo) el codo se ve girar en 3D alrededor del
  eje del lazo; es continuo pero visible. Juicio artístico pendiente de Charlie.
- Cámara de seguimiento, grosor del tramo y percepción: sin cambios (ver la persecución viva).
- Pruebas: `tests/tentacle_spatial_pursuit.rs` (sintéticas + reales `#[ignore]` con las mismas variables
  de entorno que la persecución viva). Secuencia: `examples/tentacle_spatial_pursuit_sequence.rs`.
