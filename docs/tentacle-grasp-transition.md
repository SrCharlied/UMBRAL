# Transición al agarre del tentáculo: fase A, contacto anatómico

**Estado:** adaptador puro con pruebas propias. **No** está conectado al juego: ni `app` ni `main` lo
llaman, y no hay lanzador ni preview. **No** implementa el cierre (fase B) ni la presentación (fase C).
No es aceptación.

Módulo: `src/visual/tentacle_grasp_transition.rs`. Pruebas: `tests/tentacle_grasp_transition.rs`, con
assets privados opt-in (`#[ignore]`) y las mismas claves de entorno que el preflight.

## Qué resuelve

El solver espacial (`SpatialRig::deform`, sin cambios) coloca la **punta axial** donde se le pide. El
contacto anatómico es otra cosa: es el **centroide de los 105 vértices grip**. Ese centroide va rígido con
el marco distal: `grip - punta axial = R_distal · L`, con `|L| = 0.6337` OBJ. Por eso, si se pide la punta
en el origen de la nave, el contacto queda a unos 0,61–0,66 OBJ del ancla aprobada (CONTACT02 del preflight:
0.6087753648552583).

### Ancla aprobada (`ApprovedContactAnchor::derive`)

- **Fuente:** el centroide del grupo grip del manifest sobre la pose aprobada **cruda** del `grip_frame`
  del manifest (168), con `GrowthData::base_positions`. Es una lectura pura, sin una malla temporal de
  `evaluate`.
- **Coordenadas:** se expresa en coordenadas del modelo de la nave con la inversa **exacta** de
  `ship_obj_matrix(168)`. Resultado ship-local: `[-2.15720964, 0.18263688, -1.22987014]`.
  - El oráculo del padre usa la inversa de columnas ortogonales. Coincide con este dentro de su propio
    error de reconstrucción (8,3e-10 OBJ; diferencia de 6e-8 en unidades de nave).
- **Escala:** la matriz de la nave del clip tiene escala `1/71.3739` y el juego usa 71.3739 mundo/OBJ.
  El ancla se transporta sin reescalar.
- **Convenciones de cuadro, sin renombrarlas:** `grip_frame` 168 da la misma ancla que la etiqueta
  *agarre* 169. La etiqueta *cierre* 133 es otra pose (diferencia de 12 unidades de nave) y no es
  intercambiable.
- **Lo que no es:** "centroide = origen de la nave". En el clip, el centroide queda a 0,035 OBJ del origen.

### Contacto (H-CONTACT-01, `ContactAdapter::solve`)

**Objetivo.** El adaptador busca la punta axial `T` (OBJ) tal que:

```
centroide_grip(deform(T, memoria)) = cuerpo⁻¹ · nave_dibujada · ancla
```

Aquí, `nave_dibujada` es `Ship::transform(SHIP_MODEL_YAW).matrix()` y `cuerpo` es
`creature_render_transform()`.

**Búsqueda acotada.**
- Arranca en el origen de la nave (el solver raw).
- Usa Newton con el Jacobiano por diferencias finitas de la **misma** deformación, más una búsqueda lineal
  monótona.
- Topes: `CONTACT_MAX_ITERATIONS` 16 iteraciones de Newton y 12 mitades por iteración.
- Los valores son candidatos.

**Publicación, todo-o-nada.** Antes de escribir, la malla del `T` final se recalcula en buffers temporales
y se exige:
- error **real** ≤ `CONTACT_TOLERANCE_OBJ` (1e-5, la de CONTACT02);
- `T` dentro de `[bola mínima, MAX_REACH_OBJ]`, el dominio del controlador;
- raíz27 exacta bit a bit;
- esfera finita.

Si algo falla, devuelve `Err` y no toca los buffers.

**Invariantes.**
- El adaptador solo lee matrices y datos aprobados. No escribe World, nave, cámara ni persecución.
- No cambia la escala, no traslada el brazo y no toca el modelo ni el material.
- Topología, UV y materiales no cambian: se escriben solo las posiciones y normales de la plantilla.
- La raíz y el proximal salen del reposo exacto de `deform`. El adaptador además verifica la raíz.
- La memoria que recibe se usa tal cual en cada evaluación. La que se devuelve (`pose.memory`) la decide
  pasar el llamador.

## Corrección acotada CONTACT-BIND-01

`ContactAdapter::new(package, arm_rest)` ahora construye y posee privadamente un `SpatialRig`
con el mismo `package.data()` del que extrae raíz y grip. El reposo proviene del brazo real:
en World se extrae por `correspondence.arm_vertex()`, igual que en `SpatialPursuit::bind`.
No se sustituye por `base_positions(1)`. El constructor copia los datos de reposo mediante el
constructor espacial existente, sin cambiar ese solver ni sus validaciones.

El rig sólo se expone como `adapter.rig(): &SpatialRig`, nunca mutable. `solve` conserva su
argumento rig como capacidad explícita y exige `std::ptr::eq(rig, &self.rig)` antes de leer
matrices, evaluar, indexar grupos o escribir. Un rig ajeno devuelve `Err` aunque tenga exactamente
las mismas posiciones, normales y tamaños; no se intenta probar equivalencia parcial. Tampoco hay
un constructor que acepte un rig independiente. El propietario, los índices y el rig no pueden
separarse mediante la API pública segura. No se usa un hash incompleto, ni máscaras P/D, ni la
igualdad de longitudes como prueba de identidad. El resto del algoritmo de contacto no cambia.

Migración: añadir el reposo real al constructor y pasar `adapter.rig()` a `solve` para controles
válidos. La prueba adversaria sigue pasando rig B independiente: el error es representable y se
comprueba en ejecución, no mediante un fallo de compilación. Los siete nombres, entradas y oráculos
anteriores se conservan; A06 exige además `Ok`. Ledger: `binding_fix01/TEST_API_MIGRATION.json`.

Recibos propios en `binding_fix01/evidence`: RED original conservado (error declarado 0, error del
grip verdadero 0.08000703842423962 OBJ); controles A/A y B/B con grip verdadero; permutación
coherente A/B del mismo tamaño rechazada sin publicación; rig ajeno menor de 60 frente a 72 vértices
rechazado sin pánico; rig independiente con reposo idéntico también rechazado. La tolerancia del
grip verdadero sigue siendo 1e-5 OBJ. Esta corrección no constituye aceptación ni integración Root.

## Medido antes de la corrección de enlace (recibos originales en `implementation01/evidence`)

| Caso (datos reales) | Gap raw (OBJ) | Gap publicado (OBJ) | Iteraciones / evaluaciones |
|---|---|---|---|
| Nave de la escena (port de CONTACT02) | 0.608775 | 7e-14 | 3 / 14 |
| Recto (preflight) | 0.605818 | 7e-12 | 3 / 14 |
| Oblicuo (preflight) | 0.661156 | 2e-15 | 4 / 18 |
| Cuerpo y nave girados: guiñada, y cabeceo/alabeo, con memoria (6 casos) | 0.62–0.66 | ≤ 6e-11 | 3–4 / 14–18 |
| Oblicuo a r 0,7 del lazo | — | **Err**: no converge | — |
| Recto a r 0,9 | — | **Err**: el resultado cae dentro de la bola mínima (0,579 < 0,621) | — |

El muestreo es explícito. No garantiza la convergencia para toda orientación o distancia.

## Límites y decisiones para la fase B (no implementadas)

1. **Salto de la punta.** El objetivo calibrado difiere del origen de la nave en ~|L| (≈ 0,6 OBJ ≈ 43
   unidades de mundo). Hoy la persecución lleva la punta al origen de la nave. Si B cambia de golpe al
   objetivo calibrado, la punta saltaría. Hace falta decidir si el controlador persigue el objetivo
   calibrado o si se mezcla. No hay continuidad temporal demostrada.
2. **Orientación de la espiral respecto a la nave.** Se iguala la **posición** del contacto, no la
   **orientación** del agarre aprobado. El marco distal es el que da el solver espacial. No se midió la
   orientación relativa ni la colisión volumétrica.
3. **Dominio.** Cerca del cuerpo, en oblicuo a unos 0,7–0,9 OBJ del lazo, el adaptador rechaza. Qué hacer
   ahí es una decisión de B.
4. **Coste.** Son 14–18 evaluaciones de `deform` por contacto en los casos medidos (hasta ~260 en el peor
   caso acotado). No se midió el rendimiento.
5. **Ruta en runtime.** Sin conexión al runtime, el camino inactivo y el de O2 alto no cambian porque
   nadie llama al adaptador. La equivalencia visual de la ruta real se demostrará en B/C.
6. **RED heredados.** sp10, los dos planares y GEO-SAFETY-01 siguen documentados aparte. CONTACT01/02 del
   preflight quedan como RED históricos intactos.
