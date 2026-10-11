# Prompt para la siguiente conversación — después de la Fase 3

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`). Háblame en español mexicano; prefiero que te tardes y lo hagas bien.

## Dónde estamos

- Rama de trabajo: **`claude/orchestrators-brain-fase-3-wdiheq`**. Último commit: «Fase 3.5: vigilar las bibliotecas en vivo (ADR-0024)». Si el entorno te deja en otra rama, cámbiate a ésa. Si ya se fusionó a `main`, parte de `main`.
- Lee primero `docs/ESTADO.md` y los ADR 0023 (vencimientos) y 0024 (vigilar en vivo). Corre `git status` y `git log --oneline -8` para confirmar que todo está.
- La Fase 3 está hecha en código: sensibilidad por carpeta (0020), cifrado y PIN (0021), OCR (0022), vencimientos y la tarjeta «Caduca pronto» (0023) y la vigilancia en vivo (0024). Además, `cargo fmt` pasa en todo el repo y ya existe el `LICENSE` (MIT). La Fase 4 sigue congelada.
- Si te paso errores o capturas de mi Windows, arréglalos antes que cualquier otra cosa.

## Lo que sigue, en este orden

1. **Los documentos borrados no desaparecen.** Si borro un archivo, sigue en Recientes, en «Caduca pronto», en el explorador y en la búsqueda, porque el escáner nunca registra los borrados. Con la vigilancia en vivo se nota mucho más. Choca con una pregunta abierta del maestro (§10: *¿el conocimiento sobrevive al archivo que lo originó?*, postura provisional: sí). **Antes de programar, enséñame las opciones con coste** y déjame elegir. Por ejemplo: retractar la ruta observada y dejar el conocimiento en el log; esconderlo de las listas pero dejarlo buscable como «ya no está en tu bóveda»; distinguir «borrado» de «movido fuera de la biblioteca» o de «USB desconectado». Luego el ADR y después el código, con tests.
2. **Añadir una fecha de caducidad a mano** en un documento donde el detector no vio ninguna (hoy «cambiar» sólo sale donde ya hay fecha; está anotado en el ADR-0023). También con opciones antes si tiene coste para el diseño: no le muevas al diseño más de lo necesario.
3. **El «¿por qué?» de la sensibilidad** enseña la evidencia en crudo (`regla=… fragmento=…`). Que se lea como el de los vencimientos: con palabras de persona y, si es del contenido, con el tramo marcado en el texto.
4. **La marca de carpeta se pierde al renombrar la carpeta** (ADR-0020, «En contra»): la solución de verdad es que una carpeta conserve su identidad al renombrarse (el `FileID` de un directorio en NTFS). Investiga y propónmelo con opciones.
5. Después, enséñame las **preguntas abiertas del maestro (§10)** que conviene decidir ahora (regiones del usuario, papelera, un documento dentro de otro) y pregúntame si descongelamos algo de la Fase 4.

## Entorno en la nube

- ONNX Runtime: la descarga de `cdn.pyke.io` está bloqueada; no la esquives. `pip download onnxruntime --no-deps`, descomprime el wheel, copia `libonnxruntime.so.1.x` a un `lib/` con los enlaces `libonnxruntime.so.1` y `libonnxruntime.so`, y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib`. **Cada llamada a la terminal empieza de cero**: exporta las tres variables en cada una.
- Tauri en Linux: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`. En `ui/`, `npm install`.
- Playwright está en `/opt/node22/lib/node_modules` (`NODE_PATH`), con Chromium en `/opt/pw-browsers`.
- Código de Windows: `rustup target add x86_64-pc-windows-msvc` y `cargo clippy -p obrain-keys -p obrain-ocr --target x86_64-pc-windows-msvc --all-targets -- -D warnings`. El núcleo entero no se puede comprobar así (SQLCipher necesita el compilador de C de Microsoft); para una dependencia nueva, compruébala sola en un crate de prueba.
- Banco de pruebas (nada de esto va al repo):
  - Un puente HTTP (tiny_http) con los mismos comandos que `ui/src-tauri/src/main.rs`, que sirve `ui/dist` con un `window.__TAURI_INTERNALS__.invoke` inyectado.
  - Los eventos de Tauri (`indexando`, `indexado`) se sacan de una cola que la página consulta cada 400 ms.
  - El puente también arranca el vigía.
  - OCR de apoyo con Tesseract (`apt-get install tesseract-ocr tesseract-ocr-spa`), sólo en el puente.
  - La bóveda ficticia se genera con Pillow y fpdf2 en un venv. Los datos son inventados, e incluye los ejemplos de vencimientos: «VALIDEZ 15 11 2026», «CADUCA 12/2028», etc.
  - Ojo: si dejas un `inyectar.js` junto al binario, el puente carga ése antes que el de la variable de entorno.
- Procesos: `pgrep -f` y `pkill -f` se encuentran a sí mismos; usa `pgrep -x`.
- Rutas de Windows: no las escribas con barras invertidas dentro de strings de Python en un heredoc; usa la herramienta de edición. Tras `cargo fmt`, los reemplazos de texto por script pueden no casar: comprueba que se aplicaron.

## Cómo trabajar

- Una pieza a la vez, con tests, clippy limpio (núcleo y `ui/src-tauri`), `cargo fmt --check`, `npm run build` y capturas antes de darla por buena.
- Las decisiones con coste, **antes** del código: enséñamelas y déjame elegir. Luego el ADR, y después el código.
- Al cerrar cada pieza, actualiza `docs/ESTADO.md`, el maestro y el README.
- Commits claros y subidos a la rama. **No abras un PR si no te lo pido.**
- Al final, recuérdame lo que me toca hacer a mí en Windows (está en `docs/ESTADO.md`).
