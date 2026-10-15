# Plan priorizado

Revisión: 2026-10-15. Visión: asistente personal útil, progresivamente hacia
Jarvis, empezando por una bóveda local fiable. No fusionar DevHQ/PassGuardian.

1. **Etapa actual en la nube: calidad de recuperación y citas.** Partir de
   `a962e75`, con las PR #9 y #10 integradas. Mantener un banco fijo de
   preguntas/documentos/pasajes, distinguir recuperación de suficiencia de la
   cita y comparar BM25 con E5 real. Primer resultado léxico: 11/14 preguntas
   con respuesta; tres paráfrasis pendientes. No convertir una coincidencia
   temática en una respuesta. La repetición actual conserva documentos, orden
   y citas, con Cargo 0. E5 real sigue bloqueado: añadir `us.aws.cdn.hf.co`,
   conservar `huggingface.co` y `package_managers`, guardar y aplicar en la
   configuración del entorno. El estado declarado `unknown` no demuestra
   acceso; el túnel observado al CDN devuelve 403. Sólo hay consulta de
   configuración disponible en esta sesión. Ver `EVALUACION-RECUPERACION.md`
   y `evaluaciones/preparacion-e5-2026-10-15.json`. No cambiar el banco ni
   interpretar el rechazo del modelo incompleto (Cargo 101) como validación E5.
2. **Cierres integrados que no hay que rehacer.** Correcciones tipadas, PIN,
   metadatos y revocación vectorial (PR #2); identidad entre bibliotecas
   (PR #3); publicación después de clasificar (PR #4); OCR entre hilos y
   recuperación v1 (PR #5); reconciliación de duplicados (PR #6).
   Conservar sus regresiones. Esto no cierra por extensión versiones,
   sesiones ni toda la matriz de completitud BM25/kNN.
3. **Validación Windows pendiente.** La PR #7 aporta fixture y oráculo de
   producto; la #8 registra instalación/arranque/desinstalación en runner.
   Repetir los criterios MSI reforzados cuando GitHub Actions pueda iniciar
   jobs y recorrer visualmente la app instalada en un invitado desechable.
   La búsqueda semántica real requiere además un modelo local comprobado.
4. **Persistencia, recuperación y rendimiento.** Revisar lo que sigue abierto
   tras la PR #5: secuencia/rechazo de esquemas futuros, mudanzas consistentes
   con WAL, backups/restauraciones aislados y errores de inicialización.
   Después medir rehash incremental, límites/cobertura OCR/Office e informes
   completos de indexación. No migrar la bóveda personal desde esta sesión.
5. **Relaciones, retención y dependencias.** Revisar versiones/sesiones y
   completitud de búsqueda con documentos largos, colecciones y papelera.
   La política de borrado completo sigue siendo una decisión independiente;
   seguir `RETENCION-METADATOS.md`. Actualizar selectivamente dependencias
   tras comprobar avisos vigentes, sin actualización masiva.
6. **Utilidad siguiente, condicionada a la medición.** Tras comparar E5 con
   el banco fijo: documento ausente → recuperación/fusión; documento correcto
   pero cita insuficiente → selección de pasajes. Si recuperación y
   citas son fiables, preparar respuestas breves desde un documento, con
   fuentes y abstención; después preguntas entre documentos. Colecciones
   con propósito, notas, roles y checklist siguen como opciones posteriores.

Memoria futura pequeña, explícita, corregible y ligada a fuentes. No almacenar
cada clic ni convertir inferencias en recuerdos permanentes. Medir utilidad
para encontrar/completar tareas, precisión, correcciones, latencia, consumo y
control de retención. No usar proporción de aserciones automáticas como éxito.

Validar con bases temporales y fixtures. Nunca abrir la bóveda real para probar;
no ejecutar `npm run app` como validación rutinaria. No fusionar ni desplegar.
