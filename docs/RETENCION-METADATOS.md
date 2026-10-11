# Historial de metadatos: procedimiento pendiente de aprobación

Estado: **propuesta revisable; no se ha ejecutado sobre datos existentes**.
La protección de presentación y las nuevas observaciones no necesitan migración.
Los valores antiguos continúan cifrados en el log; esta versión no los borra.

## Decisión que falta

Elegir entre conservar el historial íntegro con el filtrado de esta versión o
generar una bóveda nueva que elimine de los valores históricos de procedencia
web rutas, credenciales, consultas y fragmentos. La segunda opción pierde
deliberadamente parte del historial y necesita aprobación de retención. Una
reindexación sola no elimina esas aserciones. No cambiar triggers en el sitio.

## Secuencia para una operación autorizada posterior

1. Cerrar todos los procesos Brain y el vigía. Identificar explícitamente base,
   llave y clave de recuperación; comprobar que la base seleccionada es la que
   se pretende tratar. No imprimir valores ni rutas personales en informes.
2. Abrir una conexión SQLCipher de mantenimiento con la llave adecuada.
   Crear una **copia consistente mediante el motor** (`Store::export_encrypted`
   o backup online equivalente), nunca copiar sólo el archivo principal con
   un WAL activo. Verificar que no hay escritor competidor. Conservar la
   llave asociada a la copia y el material de recuperación sin exponerlos.
3. Restaurar esa copia bajo una ruta temporal aislada, sin bibliotecas ni
   actualización automática: usar un harness de mantenimiento, no la app.
   Abrir con recuperación y comprobar `PRAGMA integrity_check`, relaciones,
   conteos, aserciones elegidas y lectura de fixtures/documentos autorizados.
   Guardar resultados agregados y hashes, sin volcar texto. No avanzar si la
   restauración no se verifica; una copia no basta.
4. Construir otra base cifrada **nueva**, manteniendo IDs, temporalidad,
   relaciones, correcciones y controles. Aplicar únicamente la transformación
   aprobada de URLs/evidencias. Copiar con una herramienta explícita que preserve
   los invariantes al crear la base, no desactivar triggers de la original.
   Verificar igualdades de conteos y diferencias sólo en los campos aprobados.
   No reusar ciegamente evidencias que contengan una URL original.
5. Probar de nuevo restauración con llave y recuperación de la candidata;
   verificar búsqueda, PIN, colecciones y papelera en aislamiento. Conservar
   original/backup/candidata como generaciones separadas antes de cambiar nada.
6. Preparar un manifiesto de generaciones con archivos de base y llave y sus
   hashes. Cambiar la generación activa sólo con procesos detenidos, mediante
   un paso recuperable; nunca encadenar mudanzas de base, WAL y llave sin
   registro. Ante una interrupción, seleccionar la generación completa que
   pase apertura e integridad, no mezclar base de una generación con llave de otra.
7. Retener la generación original durante el periodo aprobado. Si hay fallo,
   detener procesos y restaurar el conjunto original ya verificado. Eliminar
   generaciones anteriores es **otra operación de retención**, también explícita.

Antes de ejecutar hace falta implementar y probar el harness de mantenimiento
con fixtures, concretar la selección de evidencias y el cambio recuperable de
generaciones, y presentar los resultados de restauración al usuario. Este
documento no afirma que exista ya un comando de backup/saneado del producto.
Ninguna aprobación está pendiente para los arreglos de código de esta etapa.
