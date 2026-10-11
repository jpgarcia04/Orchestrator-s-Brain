# Auditoría de confianza — 2026-10-11

Alcance: bóveda local de documentos elegidos por la persona. Rust modular,
SQLite/SQLCipher, Tauri y Svelte; `obrain-api` es una biblioteca de dominio.
No se implementan conversación, memoria general, conectores, sincronización
ni integración con DevHQ o PassGuardian.

Base comprobada: `c183f3c`, checkout limpio al empezar. El informe anterior
era externo; este documento conserva sus hallazgos sin datos personales.
**Confirmado aquí** significa código inspeccionado y fixture sintético ejecutado;
**deducido** significa riesgo de un camino leído; **pendiente** conserva una
reproducción comunicada por la auditoría anterior, sin repetirla en esta sesión.

## Fronteras corregidas

| Problema | Evidencia | Decisión y regresión |
|---|---|---|
| `correct` acepta observaciones y rutas arbitrarias | Confirmado: cinco regresiones nuevas fallan antes; `path_observed_at` fue aceptado | Lista cerrada de predicados y tipos; `rechaza_observaciones_y_controles_sin_escribir` |
| `correct(expires_on)` salta el PIN de `set_expiry` | Confirmado con base en memoria y PIN sintético | Autorizar sujeto y destino en el núcleo; `correccion_generica_no_salta_el_pin` y confirmación protegida |
| Relaciones confirmadas enviando una etiqueta como texto | Confirmado por lectura del puente y de `confirmar` de la vista previa | `Brain::confirm(subject, assertion)` recupera objeto tipado vigente y valida pertenencia; prueba con `version_of` y destino distinto de su etiqueta |
| Corrección de sensibilidad sin actualizar el control | Confirmado: se conserva `Normal` antes del arreglo | Aserción, proyección del documento/paquetes y purga de vectores en una unidad atómica |
| URL íntegra y evidencia sensible hacia el cliente bloqueado | Confirmado: fixture con `?token=SINTETICO` sale antes del arreglo | Lista de metadatos públicos con validación de tipos; ocultar literales, evidencia, fuente y tramos protegidos antes de serializar |
| Bloqueo no retira coordinadamente respuestas ya recibidas/en vuelo | Confirmado en el estado real de Svelte; regresión histórica falla antes del `await` | Revocación local previa al comando, generación de sesión y reconstrucción de vistas; pruebas de petición y consulta de seguridad pendientes |
| Error consultando seguridad equivale a no tener PIN | Confirmado: `pedirPin()` devuelve `true` en el estado antiguo | Estado desconocido deniega acceso; pruebas del estado real y del transporte |
| Caducidad sólo comprobada por futuras llamadas; una lectura renueva | Confirmado por lectura; caducidad nueva comprobada con reloj monotónico controlado | Plazo fijo desde autenticación, `restante_ms`, temporizador local y sondeo; ver ADR-0034 |
| Vectores de `vault` sobreviven sin modelo o participan en kNN | Confirmado por lectura y fixtures de revocación | Purga en las escrituras de sensibilidad del producto, filtro de sensibilidad actual en kNN. Dependencia mínima de esta etapa |

Pruebas principales: `core/obrain-api/tests/confianza.rs`, prueba de reloj en
`core/obrain-api/src/lib.rs`, pruebas de `provenance.rs` y `obrain-embed/src/index.rs`,
`ui/tests/app-security.test.mjs` y `ui/tests/pin-session.test.mjs`.

Los errores de escritura se inyectan mediante triggers **sólo en fixtures**:
se comprueba que una fecha anterior sigue vigente y que una proyección fallida
no deja la corrección añadida. Los triggers del historial del producto no cambian.

## Información pública por diseño

El PIN protege contenido y operaciones del producto. Siguen visibles nombres,
rutas observadas, identificadores, facetas/tipos, tamaño, modificación del archivo,
disponibilidad conocida, sensibilidad y topología de relaciones/colecciones.
Las fechas de caducidad, fragmentos, imágenes, literales desconocidos, procedencia
web, fuentes/evidencias libres y tramos de contenido de documentos protegidos
se retiran con el bloqueo. Los predicados públicos se validan según su tipo,
sin convertir un texto arbitrario en un tamaño, nivel o relación fiable.
`packages` y `lens` también pasan por el control; abrir y revelar comparten
la autorización y el cerrojo de Tauri durante la solicitud al sistema.

Las nuevas URLs se reducen a **origen HTTP(S)**: esquema, host y puerto.
Se descartan usuario, contraseña, ruta, consulta y fragmento. También una ruta
puede conceder acceso, por eso no se conservan listas parciales de parámetros.
Una URL histórica se reduce al mostrarla incluso si el documento es normal;
su evidencia/fuente libre no se retransmite. El log histórico no se reescribe.
Una ruta histórica afirmada por `user` no decide la ruta de apertura.

Los módulos de persistencia/extracción son código interno de confianza, no una
API para el frontend. El PIN no aísla frente a otros procesos de la misma cuenta
Windows ni cierra documentos ya abiertos en programas externos. Tampoco cifra
los originales. Sin PIN se conserva el gesto de mostrar el contenido borroso.

## Riesgos posteriores conservados

| Prioridad | Hallazgo | Estado / evidencia reproducible que debe conservarse |
|---|---|---|
| P1 | Identidad entre bibliotecas | Pendiente aquí; auditoría previa reprodujo mover `A/a.txt` a `B/moved.txt`, reemplazar `A/a.txt` e indexar A antes de B. Puede transferir marcas/correcciones. Probar ambos órdenes, raíces solapadas y guardado atómico |
| P1 | Publicación durante indexación | Deducido: extracción se confirma antes de clasificar; nuevas entidades normales. Publicar después de clasificar sin transacción gigante de OCR |
| P2 | Duplicados obsoletos | Pendiente aquí; relación sigue tras cambiar hash. Reconciliar inferencias conservando relaciones del usuario; revisar versiones y sesiones |
| P2 | Completitud de búsqueda | Pendiente aquí; demasiados fragmentos de entidad retirada ocultan un documento vigente. Elegibilidad antes del límite y suficientes documentos distintos en BM25/kNN; probar colecciones y documentos largos |
| P2 | Persistencia/recuperación | Pendiente: secuencia de migraciones, rechazo de esquemas futuros, errores ignorados en `from_store`, mudanza de base/WAL/llave, backup y restauración verificados |
| P3 | Rendimiento y cobertura | Pendiente: rehash completo, OCR/inferencia en transacciones de etapa, informes parciales, OCR PDF limitado a 20 páginas, PDF mixto, Office parcial y límite agregado OOXML |
| Producto | Retención completa | Pendiente: olvidar borra proyecciones pero conserva historial/metadatos; sin originales el log no reconstruye el texto. Ver procedimiento separado |

La primera etapa no resuelve todavía identidad ni publicación durante la
indexación. Son límites de seguridad conocidos, no garantías ya obtenidas.

## Dependencias de la interfaz

`npm audit` del 2026-10-11 mantiene dos paquetes transitivos con severidad alta:
`@xmldom/xmldom@0.8.13` por PixiJS 8.19.0 y `source-map-js@1.2.1` por
Vite 6.4.3/PostCSS 8.5.26. No se ejecutó `audit fix` ni se actualizaron esos paquetes.

El [aviso de xmldom](https://github.com/advisories/GHSA-8344-3jmq-59r6)
describe coste cuadrático al procesar XML; hay también avisos de serialización y
otras rutas de parsing. Pixi usa `DOMParser` nativo en su adaptador de navegador
y xmldom en su adaptador de worker, que aparece como chunk en el build.
No se encontró un flujo de XML/SVG documental hacia ese adaptador en `ui/src`;
**es una inferencia de alcance limitado**, no prueba de inexplotabilidad.
Las miniaturas documentales llegan desde Rust como imágenes raster.

El [aviso de source-map-js](https://github.com/advisories/GHSA-68fv-2mgg-jv7q)
afecta al consumo de mapas indexados. Su consumidor aquí está en la cadena de
construcción CSS de PostCSS; el producto no expone carga de mapas del usuario.
Actualizar selectivamente y verificar sus cadenas en un PR posterior.

La única dependencia Rust añadida es `url`, para aplicar la semántica de URL
sin un parser casero de credenciales/orígenes. Se preservan las versiones ya
bloqueadas; el lockfile Tauri sólo añade el enlace de `obrain-link` a `url`.
