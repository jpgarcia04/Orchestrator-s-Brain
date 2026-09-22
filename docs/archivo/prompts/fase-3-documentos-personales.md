# Prompt para la siguiente conversación — Fase 3

Copiar y pegar tal cual al empezar la conversación nueva.

---

Hola, soy JP. Seguimos con **Orchestrator's Brain** (`jpgarcia04/Orchestrator-s-Brain`), mi segundo cerebro para documentos personales (pasaporte, TIE, pólizas, matrícula, contratos). Háblame en español mexicano; prefiero que te tardes y lo hagas bien a que vayas rápido.

## Dónde estamos

- Todo el trabajo reciente está en la rama **`claude/project-analysis-restructure-vtq3g9`** (último commit: «Fase 2: la interfaz nueva en Svelte 5»). Parte de ahí. Si ya se fusionó a `main`, parte de `main`.
- Lee primero **`docs/ESTADO.md`** (una página: qué funciona, cómo se usa, qué sigue). Después, en `docs/PROJECT-MASTER.md`, la §8.1 (seguridad), la §9 (roadmap y fases) y la §10 (decisiones pendientes, sobre todo la del PIN). También los ADR 0006 (sensibilidad), 0013 (el mapa es la pantalla) y 0018 (bibliotecas).
- Fases 0, 1 y 2 hechas: bibliotecas, nombres limpios, `obrain indexar`, reindexado en segundo plano e interfaz nueva en Svelte 5 (el explorador como escritorio, la galaxia como vista «Mapa», `Ctrl K`, vista previa, Sensibles, noche y día, «Añadir carpeta»).

## Paso 0 — Antes de programar, pregúntame

1. **La maqueta** (https://claude.ai/artifact/Mnw9t6JbpgCHUx8pZ5JXKi): pregúntame si confirmo **el explorador como pantalla de inicio** y el **aspecto de noche por defecto**. Si confirmo, escribe el ADR-0019, que sustituye al 0013, y actualiza el registro del maestro y la §10. Si no, ajusta la interfaz a lo que te diga.
2. **¿Ya corrí la app en mi Windows?** (`cd ui`, `npm install`, `npm run app`). Nunca se ha probado en Windows de verdad, solo en un navegador del banco de pruebas. Si te paso errores o capturas, arréglalos antes que nada.

## Paso 1 — Fase 3, en este orden

1. **Sensibilidad por carpeta.** Hoy la puerta (`core/obrain-guard`) solo mira nombres y texto, y en el banco las fotos de la TIE salieron como `normal` porque una foto no tiene texto. Quiero poder marcar una carpeta como sensible (por ejemplo `01 Identidad`) y que todo lo que tenga dentro lo herede, también lo que llegue después. Tiene que ser una aserción del usuario (I5: gana siempre y ningún reescaneo la pisa; ver `Brain::correct`). En la interfaz, un botón en la cabecera de la carpeta. Con tests.
2. **Cifrado en reposo y PIN.** Es la decisión grande: **no la implementes sin antes enseñarme las opciones** (por ejemplo SQLCipher o cifrar solo lo sensible; clave en DPAPI de Windows o derivada de una frase de paso; si el PIN protege *ver* o protege *descifrar*; y qué pasa si se me olvida). Dímelo claro antes, no después. Cuando elija, un ADR y luego el código. Tiene que estar antes de que meta mi pasaporte de verdad.
3. **OCR local** para fotos y PDF escaneados. Preferentemente el OCR de Windows (`Windows.Media.Ocr`, vía el crate `windows`), detrás de `#[cfg(windows)]` para que los tests sigan pasando en Linux. Nada sale de la computadora. Empieza por las imágenes; los PDF escaneados, después.
4. **Vencimientos:** detectar fechas de caducidad en el texto (p. ej. «Vigencia: hasta el 30 de junio de 2027») como aserciones inferidas, con su «¿por qué?», corregibles, y una tarjeta «Caduca pronto» en el Inicio.
5. Si sobra tiempo: vigilar la carpeta en vivo (crate `notify`), que es lo que falta de M5c.

## Cómo trabajar

- Una pieza a la vez: tests, `cargo clippy --workspace --all-targets -- -D warnings` (núcleo y `ui/src-tauri`), `npm run build` en `ui/` (incluye `svelte-check`). Comprueba la interfaz con capturas antes de darla por buena.
- **Entorno en la nube:** la descarga de ONNX Runtime de `cdn.pyke.io` está bloqueada. No la esquives: instala `onnxruntime` desde PyPI y compila con `ORT_LIB_LOCATION=<dir>/lib ORT_PREFER_DYNAMIC_LINK=1 LD_LIBRARY_PATH=<dir>/lib` (hace falta el enlace `libonnxruntime.so.1`). Playwright está en `/opt/node22/lib/node_modules` (`NODE_PATH`), con Chromium ya instalado. Para ver la interfaz sin Tauri sirvió un «puente» HTTP pequeño (tiny_http) con los mismos comandos que `ui/src-tauri/src/main.rs`, más un `window.__TAURI_INTERNALS__.invoke` inyectado que llama a ese puente, y una bóveda ficticia generada por script. Ojo con `pgrep -f`/`pkill -f`: se encuentran a sí mismos.
- Actualiza `docs/ESTADO.md`, el maestro y el README al cerrar cada pieza. Haz commits claros y empuja a la rama de trabajo. **No abras un PR si no te lo pido.**
- Pendientes menores que no se me olvidan: el enlace roto a `LICENSE` en el README, y si renombramos la carpeta del repo (el apóstrofo da guerra).

Empieza por el Paso 0.
