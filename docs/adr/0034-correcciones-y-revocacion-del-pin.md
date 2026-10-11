# ADR-0034 — Correcciones restringidas y revocación del PIN

- Estado: implementada, validada con fixtures y app nativa debug aislada; MSI/NSIS release pendiente.
- Fecha: 2026-10-11.
- Actualiza: ADR-0021 (duración del PIN), ADR-0006 (escrituras de sensibilidad).

## Decisión

`Brain::correct` admite sólo `expires_on` (texto ISO válido o `nunca`),
`sensitivity_level` (`normal`, `sensitive`, `vault`) y `version_of`/
`occurred_during` (entidad existente distinta del sujeto). El vocabulario
registrado no concede por sí solo permiso de corrección. Marcas de carpeta y
colecciones tienen operaciones específicas. El núcleo comprueba protección del
sujeto y destino; el frontend no elige método, fuente ni confianza.

Confirmar recibe sujeto e ID de aserción vigente, y recupera su objeto tipado
del log. Las rutas para abrir/revelar sólo proceden de observaciones deterministas.
Las aserciones del usuario y sus proyecciones se escriben atómicamente mediante
savepoints, compatibles con las transacciones de etapa. No se cambia el esquema.

Un desbloqueo dura cinco minutos **desde autenticación**, con reloj monotónico.
Las lecturas de fondo no renuevan el plazo. Se devuelve tiempo restante, nunca
un permiso persistente. El cliente lo descuenta conservadoramente, revoca al
caducar y sondea estado al recuperar foco y cada segundo. Los fallos vigentes
de seguridad revocan, no equivalen a no tener PIN. Generaciones de sesión
descartan respuestas anteriores incluso después de otro desbloqueo. Bloquear
retira contenido antes de esperar a Tauri; si falla el bloqueo se mantiene la
revocación local hasta una acción explícita de autenticación.

La interfaz reconstruye las vistas con datos al revocar; vacía caches de imágenes
y permisos de mostrar. Las claves de recuperación también se retiran. Los
formularios PIN se conservan para mostrar fallos de autenticación.

Nuevas URLs: origen HTTP(S) únicamente. Historial: filtrado en lectura sin
reescritura. La lista de metadatos públicos y los límites están en
`../AUDITORIA-CONFIANZA.md`. El cifrado y la derivación de la llave no cambian.

## Consecuencias

La revocación de vectores y de su procedencia en todas las escrituras de
sensibilidad se concreta en [ADR-0035](0035-revocacion-del-estado-vectorial.md).

Se requiere escribir de nuevo el PIN al cumplirse el plazo, incluso leyendo.
La consulta de seguridad adicional al publicar contenido cuesta un IPC; se
prefiere esa comprobación a presentar una respuesta bajo una concesión caducada.
No se promete aislamiento de procesos de la misma cuenta ni revocación en Word
u otros programas donde ya se abrió un documento. Identidad entre bibliotecas y
publicación durante indexación conservan sus riesgos hasta el siguiente bloque.
