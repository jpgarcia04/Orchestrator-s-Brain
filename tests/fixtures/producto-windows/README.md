# Biblioteca sintética de producto para Windows

Todos los documentos son inventados. No hay datos personales auténticos ni credenciales. El número `4111 1111 1111 1111` es una tarjeta de prueba conocida, no utilizable para pagar.

No registres esta carpeta del repositorio como biblioteca: cópiala a una raíz temporal de una VM o Windows Sandbox. `tools/biblioteca-sintetica.ps1` prepara la copia y realiza el cambio posterior.

## Resultado esperado

| Caso | Consulta / acción | Resultado esperado |
| --- | --- | --- |
| Recuerdo parcial | `nodo azafrán` | `apuntes-redes.md` por BM25, sin conocer el nombre |
| PDF | `matrícula laboratorio` | `constancia-matricula.pdf`, vista previa con texto y procedencia de extracción |
| Copia exacta | Abrir ambos recibos de septiembre | Mismo hash de bytes y vínculo `duplicate_of`, con evidencia de hash |
| Documento contenido | Abrir `anexo-cita.txt` y `expediente-AURORA.txt` | `part_of` del anexo al expediente, con párrafos y tramo; la cita HTML sólo comparte tema, sin inclusión demostrada |
| Colección | Crear `Trámite AURORA` y añadir cita, anexo y PDF | Tres miembros tras cerrar y reabrir; filtro por `matrícula` dentro de la colección |
| Sensibilidad por nombre | Abrir `Pasaporte de Lidia Robles.txt` y contrato | Nivel sensible; con PIN bloqueado no se expone el texto |
| Sensibilidad por contenido | Abrir `nota-tarjeta-prueba.txt` | Nivel sensible por regla de tarjeta, sin citar el número en evidencia persistente |
| Actualización | Ejecutar fase `Cambio` y actualizar | Los apuntes contienen `topología de respaldo`; el contrato conserva identidad tras mover y renombrar; el recibo copia deja de ser duplicado |
| Persistencia | Cerrar y reabrir | Biblioteca, colección, marcas y estado del PIN persisten; el desbloqueo temporal no persiste |

La búsqueda semántica sólo puede verificarse si el modelo local está presente **dentro** del entorno aislado. El resultado léxico no acredita esa rama.
