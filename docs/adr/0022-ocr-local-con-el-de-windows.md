# ADR-0022 — OCR local: el reconocedor que ya trae Windows, detrás de un contrato de bytes

- **Estado:** Aceptada
- **Fecha:** 2026-09-23
- **Decide:** el «OCR (el de Windows, local)» de la Fase 3 del maestro (§9)
- **Depende de:** [ADR-0005](0005-donde-vive-el-texto-extraido.md), [ADR-0020](0020-sensibilidad-por-carpeta.md), [ADR-0021](0021-cifrado-en-reposo-y-pin.md)

## Contexto

Los documentos personales llegan muchas veces como **imágenes**: la foto de la
TIE hecha con el móvil, el pasaporte escaneado a PDF, la receta fotografiada.
Hasta ahora M2 no tenía nada que leer ahí: las fotos salían «sin texto» y los
PDF escaneados, «vacíos». No se encontraban buscando lo que dicen, y la puerta
(M6) no podía reconocer un NIE que sólo existe como píxeles. Por eso, en el
banco de pruebas, las fotos de la TIE no salieron sensibles; la Fase 3.1
(ADR-0020) lo tapó con la marca de carpeta, pero el hueco de fondo era éste.

El usuario puso dos condiciones: *«Nada sale de la computadora. Empieza por las
imágenes; los PDF escaneados, después.»*

## Decisión

1. **El reconocedor es `Windows.Media.Ocr`**, el que trae el sistema, llamado
   desde el crate `windows` detrás de `#[cfg(windows)]` en un crate nuevo,
   `obrain-ocr`. Trabaja sin conexión, no hay modelo que descargar y no suma
   nada al instalador. Fuera de Windows `obrain_ocr::platform()` devuelve
   `None` y todo sigue como antes.

2. **El contrato recibe bytes, no rutas** (`trait Ocr { engine, image, pdf }`).
   Quien lee el archivo es M2, después de su triaje: M1 no hidrata nunca un
   archivo de la nube, y un OCR que abriera rutas por su cuenta podría
   saltarse esa regla sin querer. Con bytes no puede.

3. **Una foto sólo se lee si hay OCR.** `Format::Image` (jpg, jpeg, png, bmp,
   gif, tif, tiff, webp, heic) existe siempre, pero sin reconocedor se trata
   como un formato que no se sabe leer: exactamente lo que era antes.

4. **Un PDF que sale vacío o falla se reintenta una vez con OCR**, página a
   página (`Windows.Data.Pdf` lo pinta). El extractor se anota como `pdf+ocr`,
   y así los PDF que una pasada sin OCR dejó como `empty` con extractor `pdf`
   se reintentan **una sola vez** cuando por fin hay reconocedor, no en cada
   indexado.

5. **Como mucho 20 páginas** (`MAX_PDF_PAGES`). Un documento personal
   escaneado son unas pocas; un libro escaneado de 300 páginas costaría minutos
   en cada indexado y no es lo que se busca aquí.

6. **Procedencia (I9): `ocr_engine`.** Un texto reconocido puede tener
   errores, y eso tiene que poder decirse. El log anota quién lo leyó
   (`windows.media.ocr/es-ES`) con el predicado `ocr_engine` («texto
   reconocido con»), y se retracta si el archivo deja de leerse con OCR. La
   vista previa lo enseña como «el OCR de Windows (es-ES)».

7. **El texto reconocido es una proyección más** (ADR-0005): entra en el mismo
   índice de fragmentos, lo clasifica la misma puerta (la TIE fotografiada sale
   sensible por su NIE sin marcar nada) y queda dentro de la base cifrada
   (ADR-0021). La vista previa de una foto sigue siendo la foto, con el texto
   reconocido debajo, plegado.

8. **Idioma:** el primero del perfil de Windows que tenga reconocedor
   instalado; si ninguno, `es-ES`, `es-MX`, `es`, `en-US`, en ese orden. Si
   Windows no tiene ninguno, no hay OCR y la interfaz lo dice («el
   reconocedor de texto de este equipo no está disponible»).

9. **Detalles del reconocedor de Windows** que se tuvieron en cuenta: se
   respeta la orientación EXIF (las fotos del móvil vienen giradas), la imagen
   se escala a `OcrEngine::MaxImageDimension` si es más grande, las páginas de
   PDF se pintan a esa medida (con un aumento máximo de ×2 para que el texto
   pequeño se lea), y el hilo se inicializa en WinRT como MTA.

## Alternativas descartadas

- **Tesseract.** Muy probado, pero es una biblioteca en C++ que habría que
  compilar o empaquetar en Windows, con sus modelos de idioma aparte (decenas
  de MB). Es lo que se usa **en el banco de pruebas de Linux** para ver la
  interfaz, y sólo ahí: no está en el repo.
- **`ocrs`** (Rust puro, modelos ONNX). Sin dependencias de C, pero sus modelos
  se descargan aparte y su calidad con texto en español y documentos
  oficiales es menor que la del de Windows. Queda como plan B si algún día
  hiciera falta OCR fuera de Windows.
- **Un servicio en la nube.** Descartado por la condición del usuario: nada
  sale de la computadora (I11).

## Consecuencias

- Las fotos y los PDF escaneados **se encuentran por lo que dicen**, y la
  puerta puede reconocer en ellos un NIE, un pasaporte o una tarjeta.
- Indexar la primera vez con OCR es más lento: reconocer una página cuesta del
  orden de cientos de milisegundos. Sólo pasa una vez por archivo: el hash del
  contenido decide si hay que volver a leer.
- **El código de Windows sólo está comprobado en compilación** (`cargo clippy
  --target x86_64-pc-windows-msvc`) desde Linux; los tests del núcleo usan un
  OCR de juguete, igual que M4 usa un embebedor de juguete. Falta probarlo en el
  equipo del usuario con fotos de verdad.
- Los vencimientos (Fase 3.4) se apoyan en este texto: «VALIDEZ 15 11 2026» en
  la foto de una TIE sólo existe si alguien la ha leído.
