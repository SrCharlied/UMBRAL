# Cierre del agarre P1: preparación local autorizada y cierre desde el Contact de B1

**Estado:** núcleo puro con pruebas propias, elegido por Charlie («quiero ver como queda»). **No** está
conectado a `app`/`main` y no tiene ventana ni render: la preview manual va en C, por separado. No es
aceptación artística.

Módulo: `src/visual/tentacle_grasp_closure.rs`. Pruebas: `tests/tentacle_grasp_closure.rs`, con assets
privados opt-in. La cadena de las pruebas es la real: World al 5 % → controlador → `SpatialPursuit::step`
hasta la llegada → B1 creado **con el adaptador del clip** → Contact → P1 → malla.

## Por qué P1

En el clip, el cierre 133→168 es deformación local pura del distal: el corte, la parte proximal y la nave
están quietos. Pero parte de la forma del coil en 133, que el alcance ya deformó hasta 0,9 OBJ respecto
al reposo, mientras que B1 deja el distal con la forma de reposo. Aplicar solo el desplazamiento 133→168
sobre el reposo pliega unas 100 caras (diagnóstico B2). P1 reproduce la secuencia **local** autorizada
completa desde el reposo, así que nunca transfiere deformación entre formas distintas.

## Política única H-P1-01

### Ventana

- Va del marcador `reposo` del manifest (1) a `grip_frame` (168): 6,958 s a 24 fps.
- Usa el muestreo del paquete, `CapturePackage::frame_at` (floor·fps + 1), recortado a `grip_frame`.
- No acelera nada ni inventa fases. Nunca publica `agarre` 169 ni el arrastre (181 en adelante).
- Las fases (`Preparing` / `Closing` / `Gripped`) salen de los marcadores `cierre` 133 y `grip_frame` 168.

### Referencia

La referencia es el cuadro 1. Su forma local coincide con el reposo del rig a 1e-6 (precisión del export)
y sus normales son **exactamente** las de reposo. Por eso el primer paso no tiene salto.
`ClosureClip::new` exige esa compatibilidad (≤ 1e-5): un reposo con otra forma es un enlace ajeno.

### Forma local

Para cada cuadro autorizado `f`:
1. `X_f = evaluate(f, delta 0)` en una malla temporal; se exige `o(f) = 0`.
2. El marco del corte `F_f` es el ajuste de Horn del lazo D de reposo al lazo D de `X_f`. Usa autovectores
   de Jacobi y rechaza el marco si el gap relativo de autovalores es < 1e-3 (el medido es ≥ 0,79).
3. Se calcula `L_f = F_f⁻¹(X_f)`.
4. Al distal publicado se le suma `end_rotation · (L_f − L_ref)`.

Las normales de esquina D toman la dirección autorizada `F_f⁻¹ N_f`, reescalada a la norma de reposo de
cada esquina: las normales del paquete son las crudas del OBJ y el rig publica cada normal con su norma
de reposo.

### Unión D/G (U2, corte fijado)

El lazo D no recibe desplazamiento: queda como el anillo N rígido del solver, así que el seam es bit a bit
el de B1.

Lo que se midió antes de elegir:
- El lazo autorizado cambia hasta 4,7e-3 OBJ: radial 3,0e-3, tangencial 3,9e-3, axial 2,2e-3.
- Sus segmentos llegan a medir 3,75e-4, así que esa diferencia la absorbe la primera fila de caras finas.
- Si el lazo sigue al arte, se distorsionan las caras G (normales hasta 58°). Si se fija, se distorsionan
  las caras D que tocan el corte (área ×2,9 respecto al arte).
- Repartirlo en una banda de K = 2…12 saltos no mejoró nada.

Se eligió fijar el corte porque conserva la geometría del solver. Fuera de las caras que tocan el corte,
la forma y las normales son las autorizadas (≤ 7e-3° en el diagnóstico).

### Carrier anclado

En cada cuadro autorizado **nuevo**, la punta se recalibra con la **memoria de entrada real**:
- Usa Newton acotado sobre la composición completa, con guardas de determinante y finitud y búsqueda
  lineal.
- El objetivo es que el centroide grip de la **malla final** quede en el ancla aprobada 168, transportada
  por la nave y el cuerpo, con tolerancia 1e-5 OBJ.
- Si no converge, o la punta sale de [bola mínima, `MAX_REACH`], devuelve `Err` y no publica nada.

### Estado y publicación

- **Primer cuadro:** copia exacta del Contact de B1.
- **Cuadro autorizado repetido** (floor): solo avanza el reloj.
- **`dt` = 0:** no cambia nada.
- **En `grip_frame`:** hold determinista.
- **Antes de publicar** se valida todo junto: finitud explícita (un NaN no se esconde tras `max`), norma de
  las normales, raíz exacta, ninguna cara degenerada nueva, punta en el dominio y contacto.

### Enlace

- `ClosureClip` **posee** su `ContactAdapter`, construido del mismo paquete y reposo, y lo expone para crear
  B1.
- `GraspClosure::start` exige `ptr::eq(handoff.adapter(), clip.adapter())`. Usa el getter de solo lectura
  añadido a `GraspHandoff` (ver `ACCESSOR_DELTA.json`).
- Exige además la fase Contact y que los buffers se reproduzcan bit a bit con su punta y memoria.
- Las matrices son las que capturó B1; cualquier cambio da `Err` atómico. Es un límite numérico del
  estudio: no congela ni mueve la nave ni la cámara.

## Medido (bytes finales)

| Caso real | Contacto final (OBJ) | Contacto máx. durante el gesto | Salto máx. por cuadro | Forma local (distancias) | Unión distal-corte | Recorrido de la punta |
|---|---|---|---|---|---|---|
| Escena 5 % | 8e-13 | 1e-10 | 0,040 | 1,0e-6 | 2,4e-3 | 0,785 |
| Recto | 2e-12 | 9e-11 | 0,040 | 1,0e-6 | 2,4e-3 | 0,783 |
| Oblicuo | 3e-12 | 9e-11 | 0,037 | 1,0e-6 | 2,4e-3 | 0,740 |
| Nave y cuerpo girados | 9e-11 | 1,8e-10 | 0,040 | 1,0e-6 | 2,4e-3 | 0,782 |

- **Cuadros:** se publican los 168, del 1 al 168. Con dt de 1/24, 1/30 y 1/60 los bits finales son
  idénticos. Con dt = 0,5 se saltan cuadros y el contacto final sigue en 1e-14.
- **Caras D incoherentes** (normal geométrica opuesta a sus normales de esquina): iguales a las del clip en
  cada cuadro. No hay pliegues nuevos.
- **Pasos de malla:** ≤ 0,0453 OBJ, el paso autorizado a 24 fps. No baja con dt porque el clip está
  cuantizado.

## Límites

1. **Seam:** fijar el corte desvía del arte las caras finas que tocan el corte. Eso se ve en la vista
   cercana de C.
2. **Carrier:** mantener el ancla mueve la punta unos 0,78 OBJ en 7 s, unas 56 u de mundo. El grip queda
   pegado a la nave durante todo el gesto. En el clip, en cambio, el grip se acerca a la nave mientras el
   brazo entero se mueve.
3. **Contacto:** se iguala la **posición** del contacto, no la orientación del agarre respecto a la nave.
   Los cruces nave/brazo del clip aprobado (272–308 pares) y de B1 (464) no se midieron para P1 ni se
   corrigieron.
4. **Movimiento real:** las matrices quedan fijas desde B1. Qué hacer ante un movimiento real lo decide C.
5. **Coste:** un Newton por cuadro autorizado (≤ 8 iteraciones medidas). El rendimiento no está medido.
6. **Fuera de alcance:** sin garantía universal de continuidad ni de colisión; GEO-SAFETY-01 y sp10 siguen
   abiertos.
