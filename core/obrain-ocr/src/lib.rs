//! # obrain-ocr — leer lo que es una imagen · Fase 3
//!
//! La foto de la TIE, el pasaporte escaneado, la receta fotografiada con el
//! móvil: los documentos personales llegan muchas veces como **imágenes**, y
//! una imagen no tiene texto que M2 pueda extraer. Hasta aquí salían «sin
//! texto» —una foto— o «vacíos» —un PDF escaneado—, y no se encontraban
//! buscando lo que dicen.
//!
//! Este crate pone el reconocedor que **ya trae Windows** (`Windows.Media.Ocr`)
//! detrás de un contrato pequeño, [`Ocr`]. Dos decisiones:
//!
//! 1. **Nada sale de la computadora.** El reconocedor es del sistema y trabaja
//!    sin conexión. No hay modelo que descargar ni servicio al que llamar.
//! 2. **Recibe bytes, no rutas.** Quien lee el archivo es M2, después de su
//!    triaje (M1: nunca hidratar un archivo de la nube). El OCR no abre nada
//!    por su cuenta, así que no puede saltarse esa regla.
//!
//! Fuera de Windows no hay reconocedor y [`platform`] devuelve `None`: las
//! imágenes siguen como estaban. Los tests del resto del núcleo usan un OCR de
//! juguete, igual que M4 usa un embebedor de juguete.

/// Páginas de un PDF escaneado que se reconocen como mucho. Un escaneo de un
/// documento personal son unas pocas; un libro escaneado de 300 páginas
/// tardaría minutos en cada indexado y no es lo que se busca aquí.
pub const MAX_PDF_PAGES: u32 = 20;

/// Un reconocedor de texto.
pub trait Ocr: Send + Sync {
    /// Quién reconoce, para la procedencia (I9): `windows.media.ocr/es-ES`.
    fn engine(&self) -> String;

    /// El texto de una imagen: los bytes de un JPG, PNG, BMP, GIF o TIFF.
    fn image(&self, bytes: &[u8]) -> Result<String, String>;

    /// El texto de un PDF escaneado, página a página, hasta `max_pages`.
    /// `None` si este reconocedor no sabe pintar un PDF.
    fn pdf(&self, _bytes: &[u8], _max_pages: u32) -> Option<Result<String, String>> {
        None
    }
}

/// El reconocedor de este sistema, si lo hay.
///
/// En Windows, el de `Windows.Media.Ocr` con el primer idioma de tu perfil que
/// tenga reconocedor instalado (y si ninguno, español o inglés). Si Windows no
/// tiene ninguno —un idioma sin paquete de OCR—, `None`.
pub fn platform() -> Option<Box<dyn Ocr>> {
    #[cfg(windows)]
    {
        windows_ocr::WindowsOcr::create().map(|o| Box::new(o) as Box<dyn Ocr>)
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows_ocr {
    use super::Ocr;
    use windows::core::HSTRING;
    use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapInterpolationMode, BitmapPixelFormat, BitmapTransform,
        ColorManagementMode, ExifOrientationMode, SoftwareBitmap,
    };
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    type R<T> = windows::core::Result<T>;

    /// Guarda sólo el idioma y crea el reconocedor en cada llamada: así el
    /// tipo es `Send + Sync` sin depender de cómo declare WinRT sus objetos, y
    /// crearlo cuesta poco al lado de reconocer una página.
    pub struct WindowsOcr {
        tag: String,
    }

    /// WinRT necesita el hilo inicializado. El indexador corre en un hilo
    /// propio que nadie ha inicializado; si ya lo estaba, esto no hace nada.
    fn init() {
        // SAFETY: inicializar WinRT en el hilo actual; un hilo ya inicializado
        // devuelve un código que no es un error para lo que se hace aquí.
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
    }

    fn engine_for(tag: &str) -> R<OcrEngine> {
        OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(tag))?)
    }

    impl WindowsOcr {
        pub fn create() -> Option<Self> {
            init();
            let engine = OcrEngine::TryCreateFromUserProfileLanguages()
                .ok()
                .or_else(|| ["es-ES", "es-MX", "es", "en-US"].iter().find_map(|t| engine_for(t).ok()))?;
            let tag = engine.RecognizerLanguage().ok()?.LanguageTag().ok()?.to_string();
            Some(WindowsOcr { tag })
        }
    }

    /// Los bytes, como el flujo que piden los decodificadores de WinRT.
    fn stream(bytes: &[u8]) -> R<InMemoryRandomAccessStream> {
        let s = InMemoryRandomAccessStream::new()?;
        let w = DataWriter::CreateDataWriter(&s)?;
        w.WriteBytes(bytes)?;
        w.StoreAsync()?.get()?;
        w.FlushAsync()?.get()?;
        w.DetachStream()?;
        s.Seek(0)?;
        Ok(s)
    }

    /// La imagen, **derecha** —las fotos del móvil llegan giradas y la
    /// orientación va en el EXIF— y reducida si pasa del máximo que acepta el
    /// reconocedor.
    fn preparar(decoder: &BitmapDecoder) -> R<SoftwareBitmap> {
        let max = OcrEngine::MaxImageDimension()?;
        let t = BitmapTransform::new()?;
        let lado = decoder.OrientedPixelWidth()?.max(decoder.OrientedPixelHeight()?);
        if lado > max {
            let f = f64::from(max) / f64::from(lado);
            t.SetScaledWidth((f64::from(decoder.PixelWidth()?) * f) as u32)?;
            t.SetScaledHeight((f64::from(decoder.PixelHeight()?) * f) as u32)?;
            t.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
        }
        decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &t,
                ExifOrientationMode::RespectExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )?
            .get()
    }

    /// Una línea por renglón reconocido: M2 normaliza después, y los saltos de
    /// línea le sirven para fragmentar.
    fn leer(engine: &OcrEngine, bitmap: &SoftwareBitmap) -> R<String> {
        let resultado = engine.RecognizeAsync(bitmap)?.get()?;
        let mut out = String::new();
        for linea in resultado.Lines()? {
            out.push_str(&linea.Text()?.to_string());
            out.push('\n');
        }
        Ok(out)
    }

    impl Ocr for WindowsOcr {
        fn engine(&self) -> String {
            format!("windows.media.ocr/{}", self.tag)
        }

        fn image(&self, bytes: &[u8]) -> Result<String, String> {
            init();
            let r = (|| -> R<String> {
                let engine = engine_for(&self.tag)?;
                let decoder = BitmapDecoder::CreateAsync(&stream(bytes)?)?.get()?;
                leer(&engine, &preparar(&decoder)?)
            })();
            r.map_err(|e| format!("ocr: {}", e.message()))
        }

        fn pdf(&self, bytes: &[u8], max_pages: u32) -> Option<Result<String, String>> {
            init();
            let r = (|| -> R<String> {
                let engine = engine_for(&self.tag)?;
                let max = f64::from(OcrEngine::MaxImageDimension()?);
                let doc = PdfDocument::LoadFromStreamAsync(&stream(bytes)?)?.get()?;
                let mut out = String::new();
                for i in 0..doc.PageCount()?.min(max_pages) {
                    let pagina = doc.GetPage(i)?;
                    let tam = pagina.Size()?;
                    // El doble de la resolución de pantalla (~192 ppp) lee bien
                    // un escaneo, sin pasarse del máximo del reconocedor.
                    let escala = (max / f64::from(tam.Width.max(tam.Height))).min(2.0);
                    let opciones = PdfPageRenderOptions::new()?;
                    opciones.SetDestinationWidth((f64::from(tam.Width) * escala) as u32)?;
                    opciones.SetDestinationHeight((f64::from(tam.Height) * escala) as u32)?;
                    let pintada = InMemoryRandomAccessStream::new()?;
                    pagina.RenderWithOptionsToStreamAsync(&pintada, &opciones)?.get()?;
                    let decoder = BitmapDecoder::CreateAsync(&pintada)?.get()?;
                    let bitmap = SoftwareBitmap::Convert(
                        &decoder.GetSoftwareBitmapAsync()?.get()?,
                        BitmapPixelFormat::Bgra8,
                    )?;
                    out.push_str(&leer(&engine, &bitmap)?);
                    out.push('\n');
                }
                Ok(out)
            })();
            Some(r.map_err(|e| format!("ocr: {}", e.message())))
        }
    }
}

#[cfg(test)]
mod tests {
    /// Fuera de Windows no hay reconocedor: las imágenes siguen sin texto, que
    /// es lo que había. En Windows depende del idioma instalado, y no se
    /// afirma nada.
    #[test]
    fn fuera_de_windows_no_hay_reconocedor() {
        if cfg!(not(windows)) {
            assert!(super::platform().is_none());
        }
    }
}
