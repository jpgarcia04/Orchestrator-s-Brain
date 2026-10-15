# Pregunta → documento → pasaje

Medición del 15 de octubre de 2026 sobre el código de producto de `a962e75`
(PR #10 integrada), con el banco añadido en esta entrega. Sólo documentos
ficticios y bases temporales; ninguna apertura de la bóveda personal.

## Qué mide

El banco está en `tests/fixtures/recuperacion/preguntas.json` y usa una copia
local temporal de los 11 documentos de `producto-windows`. Contiene 17
preguntas: 14 con respuesta conocida (fechas, importes, requisitos, recuerdos,
consultas exactas y paráfrasis) y tres sin respuesta en el corpus.

Cada referencia indica documentos aceptables y texto esperado de respuesta.
Las dos copias idénticas de un recibo son resultados válidos; su orden no
penaliza la medición. Antes de consultar se verifica que el texto de cada
respuesta de referencia existe en el documento indexado correspondiente.

Se registran por separado:

- Documento esperado en primera posición y entre los tres primeros resultados.
- Pasaje entre los tres primeros documentos que contenga la respuesta literal
  en el documento correcto. Esta es una medida acotada de cobertura textual,
  no un juez general de comprensión o de veracidad.
- Candidatos devueltos para preguntas sin respuesta. Un buscador puede devolver
  documentos relacionados: ese dato no equivale a respuesta ni a alucinación.
- Tiempo de indexación y de cada búsqueda, como observaciones de una sola
  ejecución, sin promesa de rendimiento ni comparabilidad entre máquinas.

Toda cita recibida se contrasta con el texto normalizado y con una llamada a
`preview_at_version`: los offsets UTF-8 deben abrir exactamente el texto citado
bajo esos hashes. Se vuelve a consultar todo el banco con PIN bloqueado y se
exige que los resultados sensibles no expongan fragmentos ni pasajes.

Los once casos léxicos conocidos son regresiones obligatorias de documento y
respuesta en top 3. Las tres paráfrasis se miden sin exigir que BM25 las resuelva.
La prueba E5 comprueba modelo real, vectores indexados y contribución kNN;
no admite pasar silenciosamente al modo léxico. Sus métricas no se fuerzan a
superar BM25 antes de conocer el resultado.

## Resultado léxico observado

| Medida | Resultado |
| --- | --- |
| Preguntas con respuesta conocida | 14 |
| Documento correcto en primera posición | 11/14 |
| Documento correcto entre los tres primeros | 11/14 |
| Cita con respuesta entre los tres primeros | 11/14 |
| Preguntas sin respuesta | 3 |
| Preguntas sin respuesta que devuelven candidatos | 1/3 |
| Regresiones obligatorias | 11/11 aprobadas |
| Rama semántica | No ejecutada; cero vectores en esta medición |

[Resultado completo por pregunta](evaluaciones/recuperacion-lexica-2026-10-15.json).
El resultado es pequeño y sintético; no representa el rendimiento sobre una
bóveda personal ni valida OCR, instaladores o interacción visual.

Las tres paráfrasis que no recuperan el documento esperado son:

- «¿Cuánto pago mensualmente por el piso?» → contrato: 700 unidades al mes.
- «¿En qué fecha se acaba mi arrendamiento?» → contrato: 31 de agosto de 2027.
- «¿En qué salón nos reunimos para la clase del martes?» → horario: B-14.
  La búsqueda devuelve los apuntes, pero no el horario esperado en top 3.

«¿El contrato de alquiler permite mascotas?» devuelve el contrato, que no
incluye esa información. No hay una función de respuesta generada en este
flujo: la interfaz dice que son citas de la versión indexada. El caso debe
conservarse como requisito de abstención para cualquier futuro contestador.
Los otros dos negativos preguntan por un IBAN y un teléfono inexistentes.

## Repetir sin red

Desde la raíz del repositorio, con Rust y ONNX Runtime configurados. En el
entorno cloud preparado se activan con `source /workspace/.obrain-env/env.sh`.
El directorio de salida debe existir; utiliza rutas distintas por ejecución.

```bash
OBRAIN_EVAL_REPORT=/tmp/brain-lexical.json \
  cargo test --manifest-path core/Cargo.toml --locked -p obrain-api \
  --test recuperacion_banco evaluar_recuperacion_lexica -- --nocapture
```

`OBRAIN_EVAL_REPORT` es opcional: sin él se imprime el informe con `--nocapture`.
La prueba falla ante regresión de los casos obligatorios, citas incoherentes,
error de indexación o exposición de contenido con PIN. El informe se escribe
antes de evaluar el umbral de calidad; un archivo por sí solo no acredita un
comando aprobado: comprueba siempre la salida y el código de Cargo.

## Comparación E5 pendiente

La comprobación actual ya obtiene `config.json` (200) y metadatos oficiales
desde `huggingface.co`. Pesos y tokenizer redirigen con 302 a
`us.aws.cdn.hf.co`; el proxy rechaza el túnel a ese destino con 403. No se
descargaron pesos/tokenizer ni se ejecutó inferencia real.

El entorno declara sólo `huggingface.co` como host personalizado y conserva
`package_managers`; el estado de política es `unknown`, aunque la instancia
está conectada. La instantánea local no incluye el CDN. Son datos de
configuración y observaciones de acceso distintos: el 200 de un host no
acredita acceso al otro ni una política aplicada. La herramienta disponible
sólo consulta el entorno. Queda **añadir `us.aws.cdn.hf.co`, conservar
`huggingface.co` y `package_managers`, guardar y aplicar la configuración**.
No se editó la instantánea local, ni se omitieron proxy o verificación TLS.
Si aparece otra redirección bloqueada, identificar el dominio exacto y usar
el mismo flujo de configuración.

Revisión oficial fijada: `614241f622f53c4eeff9890bdc4f31cfecc418b3`.
Directorio fuera del checkout:
`/workspace/models/multilingual-e5-small/614241f622f53c4eeff9890bdc4f31cfecc418b3`.

| Archivo local | SHA-256 esperado | Comprobación actual |
| --- | --- | --- |
| `model.onnx` | `ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665` | Metadatos LFS oficiales; archivo ausente |
| `tokenizer.json` | `0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39` | Metadatos LFS oficiales; archivo ausente |
| `config.json` | `69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959` | Descargado, 655 bytes, hash local coincidente |

Los metadatos oficiales se consultaron sobre esa misma revisión, no sobre
`main`. No confundir el hash LFS esperado con un hash observado de descarga.
Comandos para completar la descarga tras aplicar la configuración, usando
archivos `.part` hasta que la verificación termine correctamente:

```bash
set -e
source /workspace/.obrain-env/env.sh
revision=614241f622f53c4eeff9890bdc4f31cfecc418b3
model_dir=/workspace/models/multilingual-e5-small/$revision
mkdir -p "$model_dir"
base_url=https://huggingface.co/intfloat/multilingual-e5-small/resolve/$revision
curl --fail --location "$base_url/onnx/model.onnx" -o "$model_dir/model.onnx.part"
curl --fail --location "$base_url/tokenizer.json" -o "$model_dir/tokenizer.json.part"
curl --fail --location "$base_url/config.json" -o "$model_dir/config.json.part"
(
  cd "$model_dir"
  sha256sum --check <<'SHA256'
ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665  model.onnx.part
0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39  tokenizer.json.part
69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959  config.json.part
SHA256
  mv model.onnx.part model.onnx
  mv tokenizer.json.part tokenizer.json
  mv config.json.part config.json
)
```

Con un modelo completo disponible, la prueba no usa la red:

```bash
OBRAIN_EVAL_MODEL=/workspace/models/multilingual-e5-small/614241f622f53c4eeff9890bdc4f31cfecc418b3 \
OBRAIN_EVAL_REPORT=/tmp/brain-hybrid.json \
  cargo test --manifest-path core/Cargo.toml --locked -p obrain-api \
  --test recuperacion_banco evaluar_hibrida_e5 -- --ignored --exact --nocapture
```

Faltando la variable o cualquiera de los archivos, la ejecución explícita falla.
En el suite normal esta prueba figura como ignorada; no cuenta como E5 validado.

## Evidencia de la reanudación

El checkout coincide con `work` y `a962e75`, conservando cambios sin commit.
Faltaban el registro de preparación, el directorio del modelo y la copia
`/workspace/work/obrain-e5/original` citados en el traspaso. No se restauró
contenido: se preservó lo encontrado en
`/workspace/work/obrain-e5/resume-2026-10-15/original` y se creó un
[registro nuevo](evaluaciones/preparacion-e5-2026-10-15.json) que distingue
los hashes esperados, los observados y las ejecuciones realizadas.

- Repetición léxica: Cargo 0, 11/14 documento top 1, 11/14 documento top 3,
  11/14 cita suficiente y 11/11 casos obligatorios. Las 17 filas conservan
  todos sus campos salvo tiempos: candidatos, posiciones, rangos de ramas
  y citas idénticos. Pasan literalidad, navegación por versión y PIN.
  [Informe completo](evaluaciones/preparacion-e5-2026-10-15/lexical-repeat.json),
  [comparación](evaluaciones/preparacion-e5-2026-10-15/comparacion-lexica.json)
  y [salida Cargo](evaluaciones/preparacion-e5-2026-10-15/lexical-cargo.log).
- Tres paráfrasis: vivienda y fin sin candidatos; clase devuelve apuntes,
  sin horario. Dos exactas y once casos léxicos mantienen su orden.
- Sin respuesta: mascotas devuelve el contrato sin evidencia sobre mascotas;
  IBAN y teléfono no devuelven candidatos. Se conservan como negativos.
- Intento E5 explícito: Cargo 101 por archivos locales ausentes, sin informe
  híbrido ni inferencia. El rechazo confirma que no se usa fallback silencioso;
  no proporciona métricas E5.
  [Salida completa](evaluaciones/preparacion-e5-2026-10-15/hybrid-cargo.log).
- Preguntas, respuestas, criterios, código del banco e informe léxico original
  permanecen byte a byte respecto a la copia inicial de esta sesión.

## Próxima decisión

Comparar E5 con el mismo banco, sin cambiar las preguntas para mejorar una
puntuación. Primero comprobar si resuelve las paráfrasis sin degradar consultas
exactas y cómo ordena documentos en preguntas sin respuesta. Conservar tanto
los fallos como los aciertos; revisar también los once casos léxicos y cualquier
regresión de orden, además de documento top 1, top 3 y cita por separado.

Si falta el documento correcto, mejorar recuperación/fusión. Si el documento
está pero la cita no responde, mejorar selección de pasajes. Sólo si ambas
capas son suficientes, preparar un experimento acotado de respuesta con citas
y abstención. El fallo léxico observado está en la recuperación del documento
para tres paráfrasis; todavía no hay comparación que permita decidir el
siguiente cambio de producto tras E5. No añadir conversación o memoria general.
