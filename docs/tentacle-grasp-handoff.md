# Transición al agarre, fase B1: traspaso de la postura alcanzada al contacto calibrado

**Estado:** núcleo puro con pruebas propias. **No** está conectado al juego: ni `app` ni `main` lo llaman,
y no hay ventana, render ni lanzador. **No** implementa el cierre orgánico del clip (B2) ni la preview (C).
No es aceptación.

Módulo: `src/visual/tentacle_grasp_handoff.rs`. Pruebas: `tests/tentacle_grasp_handoff.rs`, con assets
privados opt-in (`#[ignore]`). La cadena de las pruebas es la real: World al 5 % → controlador →
`SpatialPursuit::step` hasta la llegada → instantánea publicada → B1 → malla.

## Problema

Al llegar, la persecución publica una malla con la punta axial en el origen de la nave. El contacto
calibrado de A (`ContactAdapter::solve`) pide otra punta, a unos 0,61–0,66 OBJ de ahí. Conectar
directamente a A haría saltar el primer cuadro entre 0,63 y 0,69 OBJ en los casos medidos.

## Política única H-HANDOFF-01

### `ReachedPose::capture(&SpatialPursuit)`

Copia el brazo que la persecución **publicó**: la capa `composite()` desde `body_prefix()`, más la
`SpatialPose` del último cuadro (`last().shaft`). Exige que la memoria publicada sea la de esa pose. Sin
brazo deformado (reposo con O2 alto o sin paso), devuelve `Err`.

### `GraspHandoff::start(&adapter, &reached, nave, cuerpo, duración)`

- **Captura** las matrices de la nave dibujada y del cuerpo. La duración es explícita, en segundos;
  `HANDOFF_CANDIDATE_DURATION_S` = 1 s es **candidata**, no una regla del juego.
- **Comprueba** que `adapter.rig().deform(punta, memoria)` reproduce la postura publicada **bit a bit**.
  Con el mismo enlace anatómico es la propiedad dt 0 del solver espacial; un enlace ajeno se rechaza.
- **Primer cuadro:** es una **copia exacta** de la postura alcanzada (buffers, pose y memoria).

### `advance(nave, cuerpo, dt)`

**Validación previa.**
- `dt` debe ser finito y estar en `[0, MAX_PURSUIT_DT]`.
- Las matrices deben ser **las capturadas, bit a bit**.
- Si algo falla, devuelve `Err` sin cambiar nada y sin adelantar el reloj.

**Pausa y hold.** `dt = 0` y el estado de contacto no recalculan ni avanzan nada.

**Cada cuadro.**
1. `s = smoothstep(t / D)`.
2. Se recalcula el destino A con la **memoria entrante** (la publicada). La memoria A1 gira durante el
   traspaso y el destino calibrado cambia con ella; en el oblicuo deriva hasta 4,5e-4 OBJ.
3. Se publica `deform(T0 + (A − T0)·s, memoria)` con el rig del adaptador.

**Último cuadro (`t = D`).** Publica la solución A **tal cual**: buffers, pose y memoria. El destino está
calculado con la misma memoria que describe los buffers publicados.

**Publicación conjunta.** Todo se prepara en temporales: raíz exacta y salto finito antes de publicar
buffers, pose y estado. No se interpola la malla: cada cuadro es una deformación real, que conserva
topología, UV, materiales, raíz y proximal.

## Medido (bytes finales)

| Caso (cadena real) | Salto del 1.er cuadro | Salto máx. de toda la malla por cuadro, 30 fps | Contacto final (OBJ) |
|---|---|---|---|
| Escena 5 % | 0 (A directo: 0,629) | 0,0314 | 9e-11 |
| Recto | 0 | 0,0316 | 9e-13 |
| Oblicuo | 0 | 0,0343 | 3e-11 |
| Nave y cuerpo girados (guiñada, cabeceo/alabeo) | 0 | 0,0329 | 5e-12 |

**Refinamiento** desde la misma instantánea, con dt de 1/30, 1/60 y 1/120 s:
- máximo/dt = 0,943 constante;
- el primer salto cae como dt² (2,0e-3 → 5,2e-4 → 1,3e-4).

No hay salto geométrico esencial en estos casos. Por acumulación de coma flotante sale un cuadro final
residual (31 en vez de 30) con un salto de ~1e-10.

**Rechazos atómicos probados:**
- `dt` negativo, NaN, infinito o mayor que el máximo;
- nave movida 1e-3, cuerpo girado 1e-6 rad o matriz NaN, en las tres fases;
- duración inválida, persecución en reposo y enlace ajeno;
- A sin solución a mitad del traspaso (nave oblicua a r 0,7 del lazo): no se publica nada y el reloj no
  avanza.

## Límites (declarados, no resueltos)

1. **Restricción numérica del estudio:** las matrices quedan fijas desde `start`. Si la nave o el cuerpo
   se mueven, `advance` rechaza; B1 no congela ni mueve nave o cámara. La política de redirigir o
   reiniciar ante un movimiento real se elegirá y probará al conectarlo (C).
2. **Velocidad:** con 1 s, la malla se mueve como máximo a ~0,94 OBJ/s (≈ 67 u/s de mundo). La
   persecución usa 15/10/5 → 12/24/48 u/s; B1 no las toca. Si se quiere otro ritmo, se cambia la
   duración candidata.
3. **Garantías:** no hay garantía universal de continuidad ni de colisión. GEO-SAFETY-01 y sp10 siguen
   abiertos y el solver es el mismo, byte a byte.
4. **Orientación:** igual que en A, se iguala la **posición** del contacto, no la orientación de la espiral
   respecto a la nave.
5. **Coste:** una llamada a A por cuadro (14–18 evaluaciones de `deform`); el rendimiento no está medido.
