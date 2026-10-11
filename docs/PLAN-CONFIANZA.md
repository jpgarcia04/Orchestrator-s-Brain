# Plan priorizado

Revisión: 2026-10-11. Visión: asistente personal útil, progresivamente hacia
Jarvis, empezando por una bóveda local fiable. No fusionar DevHQ/PassGuardian.

1. **Etapa actual: correcciones, PIN y metadatos.** Cerrar escritura de
   observaciones técnicas, confirmar objetos tipados, autorizar en el núcleo,
   reproyectar sensibilidad atómicamente y revocar datos del cliente. URLs
   mínimas nuevas y filtrado del historial al leer, sin sanear la bóveda real.
   Un PR de seguridad sobre el checkout auditado, con commits de núcleo,
   interfaz y documentación. La revocación de vectores entra como dependencia.
2. **Identidad entre bibliotecas y publicación segura.** Son las siguientes
   fronteras de seguridad: ni transferir controles a un reemplazo ni publicar
   contenido antes de clasificarlo. Inventario conjunto de bibliotecas,
   pruebas de ambos órdenes/solapamientos y preservación del guardado atómico.
   Estado no publicable durante extracción; evitar transacciones de OCR enormes.
3. **Relaciones y recuperación correcta.** Reconciliar duplicados, versiones
   y sesiones preservando hechos del usuario. BM25/kNN: filtrar elegibilidad
   antes de cortar, recuperar documentos distintos, incluir papelera y colección
   en la matriz de pruebas.
4. **Persistencia, recuperación y rendimiento.** Migraciones secuenciadas,
   rechazo de futuro, errores de inicialización, mudanzas consistentes con WAL,
   backups/restauraciones aislados; después medición, rehash incremental,
   límites/cobertura de OCR/Office y errores completos de IndexReport.
5. **Retención y dependencias.** Decisión independiente de borrado completo y
   tratamiento del historial; seguir `RETENCION-METADATOS.md`. Actualización
   selectiva de las dos cadenas npm con avisos, sin actualización masiva.
6. **Utilidad cotidiana.** Nombre/ruta y filtros; bibliotecas/exclusiones;
   colecciones con propósito, notas, roles y checklist; renovación y sustitución;
   uso explícito de documentos; evidencia; preguntas y comparaciones citadas.

Memoria futura pequeña, explícita, corregible y ligada a fuentes. No almacenar
cada clic ni convertir inferencias en recuerdos permanentes. Medir utilidad
para encontrar/completar tareas, precisión, correcciones, latencia, consumo y
control de retención. No usar proporción de aserciones automáticas como éxito.

Validar con bases temporales y fixtures. Nunca abrir la bóveda real para probar;
no ejecutar `npm run app` como validación rutinaria. No fusionar ni desplegar.
