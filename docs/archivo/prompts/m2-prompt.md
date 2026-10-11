# Prompt para M2 — Leer el disco

Copia todo lo de abajo en una conversación nueva de Claude Code, con working directory en este repo.

---

Estoy construyendo **Orchestrator's Brain**, un motor de conocimiento personal local-first. La arquitectura y las decisiones ya están cerradas y documentadas — léelas antes de escribir nada:

1. [`docs/PROJECT-MASTER.md`](../../PROJECT-MASTER.md) — documento vivo: modelo de datos, invariantes, stack, roadmap. Lee especialmente §3 (modelo de datos), §4.4 (enriquecimiento determinista) y los informes de M1/M3 al final.
2. [`docs/EXPERIENCE.md`](../../EXPERIENCE.md) — diseño de experiencia (no aplica a M2, pero da contexto).
3. `docs/adr/0001` a `0004` — decisiones fundacionales con sus alternativas descartadas.

**Código ya existente y funcionando** (léelo, no lo reescribas):

- [`core/obrain-core`](../../../core/obrain-core) — M0. Log de aserciones bitemporal. `Store`, `Assertion`, `Method`, `Facet`, `TimePoint`. Invariantes I1/I4 impuestos por triggers de SQLite. 14 tests.
- [`core/obrain-scan`](../../../core/obrain-scan) — M1. Descubrimiento, identidad `(volumen, FileID)` + BLAKE3, triaje de placeholders de OneDrive (`triage::Availability`), exclusiones (`exclude::ExclusionSet`). CLI: `obrain-scan <ruta> --db <archivo> [--verify] [--dry-run]`.
- [`core/obrain-link`](../../../core/obrain-link) — M3. Enlazadores deterministas: `duplicates`, `structure` (carpetas → entidades `Container`), `provenance` (`:Zone.Identifier` → `downloaded_from`), `sessions` (agrupación por `mtime`), `versions` (heurística de nombre). CLI: `obrain-link <db> [--sample]`.

Todo compila con `cargo build --manifest-path core/Cargo.toml`. Toolchain MSVC + Build Tools ya instalados en esta máquina. 50 tests en verde, clippy limpio con `-D warnings`.

## Qué es M2

Del roadmap en `PROJECT-MASTER.md` §9:

> **M2 — Leer el disco.** Extracción por formato, chunking, FTS5. Criterio de "hecho": búsqueda léxica instantánea sobre el corpus completo.

M2 va **después** de M1 y M3 a propósito (orden invertido respecto al roadmap original): M1 y M3 ya dejaron 1.291 archivos identificados, triados y conectados por 2.480 aristas deterministas, sin tocar IA ni extracción. M2 añade la capa de contenido sobre ese cimiento.

## Alcance de M2

Crear un nuevo crate `core/obrain-extract`:

1. **Extracción de texto por formato**, empezando por lo que de verdad hay en el corpus (mira los datos, no asumas): PDF, texto plano/Markdown, y HTML como mínimo. Office (docx/xlsx/pptx) si el tiempo da, si no, se difiere explícitamente al informe.
2. **Triaje de extracción previo a abrir el archivo**: reutiliza `Availability` de `obrain-scan` — nunca se extrae texto de un archivo no local (mismo invariante que M1: no hidratar OneDrive).
3. **Chunking** del texto extraído. Define una estrategia simple y documentada (por tamaño con solape, o por párrafo) — no hace falta que sea sofisticada, pero **debe versionarse**: cada chunk debe poder rastrearse a `(estrategia, versión)`, porque el invariante I7 del maestro (versionado de embeddings) depende de que el chunking también esté versionado. M2 no hace embeddings, pero deja el campo listo.
4. **Persistencia del texto extraído** y de los chunks — decide dónde encaja esto en el esquema de `obrain-core` (¿tabla nueva? ¿aserciones con predicado `has_chunk`? Argumenta la elección, no la des por sentada).
5. **Índice FTS5** de SQLite sobre los chunks. Es built-in en `rusqlite` con la feature `bundled`, que ya está activada.
6. **CLI** `obrain-extract <db> [--sample]` en el mismo estilo que `obrain-scan` y `obrain-link`: un binario que se ejecuta sobre una base de datos ya escaneada y enlazada, con un informe final legible.
7. **Query de búsqueda léxica** de extremo a extremo: dado un término, devuelve archivos con snippet y ranking BM25 (FTS5 lo da nativo con `bm25()`).

## Restricciones que no son negociables

- **Nunca extraer contenido de un archivo no local.** Es el mismo invariante que protegió M1 (`Availability::may_read_content()`). Escribe un test que lo verifique explícitamente, como se hizo en M1/M3.
- **Cero llamadas a un modelo de IA.** M2 es determinista, igual que M3. La IA entra en M4.
- **Todo triggerado por triggers de SQLite donde aplique** (I1/I4 ya están; no los toques).
- **`cargo clippy --all-targets -- -D warnings` limpio** antes de dar nada por terminado.
- **Todos los tests en verde**, incluidos los 50 ya existentes — no deben romperse.

## Metodología que ya demostró su valor en M1/M3 — repítela

En M3 hubo tres defectos reales que los contadores no mostraban y que sólo aparecieron **auditando muestras reales del resultado**, no confiando en "N cosas procesadas": fotos encadenadas como versiones, series numeradas mal agrupadas, archivos vacíos falsamente "duplicados". Cada uno se volvió un test de regresión con el caso real que lo destapó.

Para M2, eso significa:

1. **Ejecuta contra el corpus real de esta máquina** (`C:\Users\pacox\OneDrive`, ya indexado — puedes reescanear con `obrain-scan` si hace falta una base fresca), no sólo contra datos sintéticos de test.
2. Añade un modo `--sample` al CLI que muestre extracciones reales (texto extraído de 5-10 archivos variados, snippets de búsquedas de prueba) para poder juzgar la calidad a ojo, no sólo contar bytes.
3. Si encuentras basura en la extracción (texto ilegible de un PDF escaneado, ruido de un HTML con mucho boilerplate, etc.), no lo escondas: repórtalo igual que se reportó lo de las cachés de Chromium en M1, y decide si se filtra o se deja para un hito posterior con justificación explícita.
4. Verifica la aritmética de lo que reportes (como se hizo con "35.583 − 33.517 podados = 2.066 vistos" en M1): si el informe dice "N archivos con texto extraído", comprueba que N tiene sentido contra el total y contra cuántos son de formatos soportados.

## Al terminar

1. Actualiza `docs/PROJECT-MASTER.md`: marca M2 como hecho en la tabla del roadmap (§9) con un informe real igual de riguroso que los de M1 y M3 (números reales, aritmética verificada, defectos encontrados y corregidos, problemas que quedan abiertos).
2. Si tomas una decisión de arquitectura no trivial (dónde persistir los chunks, qué chunker usar), documéntala como ADR nuevo en `docs/adr/` siguiendo el formato de los cuatro existentes — contexto, decisión, alternativas descartadas con motivo, consecuencias a favor y en contra.
3. No hagas commit tú mismo — deja el árbol de trabajo listo y dime qué cambió para que yo decida cuándo commitear.

Sé exigente contigo mismo con el mismo rigor que hubo en M1 y M3: no reportes "hecho" hasta haberlo verificado contra datos reales, no sintéticos.
