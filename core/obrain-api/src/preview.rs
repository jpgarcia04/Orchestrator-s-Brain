//! **Qué se ve del archivo.** La segunda mitad de M5b.
//!
//! `EXPERIENCE.md` §4.4 describe el panel de entidad —qué es, lo observado, lo
//! inferido con su «¿por qué?», la corrección— y no dice en ninguna parte qué
//! se ve *del archivo en sí*. Eso es lo que hay aquí, y el usuario lo planteó
//! con la frase que ordena el hito: *«si no puedo ver los archivos, no va a
//! tener mucho chiste la app»*.
//!
//! ## La trampa, que es de las que se pagan caras
//!
//! Un archivo `cloud_dehydrated` **es un puntero: leerlo lo descarga.** M1 se
//! definió entero por no hidratar ni uno, y una miniatura generada al vuelo lo
//! violaría en silencio y a escala — que es como se pierde de golpe la
//! confianza en un producto que promete no tocar tu nube. La regla no es
//! negociable:
//!
//! > **Previsualiza sólo lo ya extraído y lo local. Hidratar jamás como efecto
//! > secundario.**
//!
//! Traducida a este módulo son dos caminos con costes distintos, y por eso van
//! en este orden:
//!
//! 1. **El texto no abre nada.** Sale de `chunks`, la proyección que M2 ya
//!    escribió ([ADR-0005]). Un archivo en la nube no tiene fragmentos porque
//!    M2 tampoco lo abrió; el que los tiene es porque estaba local cuando se
//!    extrajo. No hace falta ninguna guarda aquí — no hay nada que guardar.
//! 2. **La miniatura sí abre un archivo**, y por eso
//!    [`Availability::may_read_content`] va **delante** de `File::open` y no
//!    dentro. La guarda es la de M1, importada; no una copia con las mismas
//!    cadenas, que es como se desincronizan dos definiciones de lo mismo.
//!
//! ## Y cuando no hay nada que enseñar
//!
//! §4.6: *«no lo sé» es una pantalla diseñada, no un error*. Un hueco en blanco
//! no dice si el archivo está vacío, si está en la nube, si es un formato
//! desconocido o si el programa se rompió. Por eso [`Preview::reason`] existe y
//! por eso se construye leyendo `availability` y `extraction_status` del log en
//! vez de inventarse un mensaje genérico: **el sistema ya sabía la respuesta,
//! sólo no la estaba diciendo.**
//!
//! [ADR-0005]: ../../../docs/adr/0005-donde-vive-el-texto-extraido.md

use base64::Engine as _;
use obrain_core::{EntityId, Result, Store, TimePoint};
use obrain_scan::Availability;
use std::path::Path;

/// Cuánto texto viaja al panel de una vez, en bytes.
///
/// Un documento del corpus real llega a cientos de miles de caracteres y el
/// panel es una caja con barra de desplazamiento, no un lector. Mandarlo entero
/// en cada selección del explorador —que cambia con cada flecha— sería pagar
/// megabytes por el puente para enseñar dos pantallas.
pub const TEXT_BUDGET: usize = 40_000;

/// Lado mayor de la miniatura, en píxeles.
pub const THUMB: u32 = 720;

/// Más grande que esto no se decodifica.
///
/// Decodificar es memoria: un PNG de 40 MB puede ser una imagen de 20.000 ×
/// 20.000 y varios GB en RAM. El límite protege de un archivo, no de un ataque.
pub const MAX_IMAGE_BYTES: u64 = 48 * 1024 * 1024;

/// Extensiones que se intentan decodificar. Es una lista corta a propósito:
/// cada formato que entra es un decodificador más compilado dentro.
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Lo que se puede enseñar de un archivo, y si no se puede, por qué.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Preview {
    pub id: String,
    pub path: Option<String>,
    /// Tal y como la escribió M1. `None` **no es lo mismo que local**: es un
    /// valor que no se reconoce, y ante la duda no se toca el archivo.
    pub availability: Option<String>,
    /// La guarda de M1 ya resuelta, para que la interfaz no vuelva a
    /// interpretar la cadena por su cuenta.
    pub readable: bool,
    pub sensitivity: &'static str,
    /// `text` · `image` · `none`.
    pub kind: &'static str,
    /// El tramo del documento que se enseña. Ver [`TEXT_BUDGET`].
    pub text: Option<String>,
    /// Byte del documento donde empieza [`Preview::text`].
    pub offset: usize,
    /// Caracteres del documento **entero**, no del tramo. Es lo que permite
    /// decir «viendo 40.000 de 210.000» en vez de fingir que eso es todo.
    pub chars: usize,
    pub truncated: bool,
    /// `[inicio, fin)` **dentro de `text`**: el tramo que explica la consulta.
    /// Lo elige [`obrain_extract::evidence::pick`], igual que la prueba de la
    /// lista de resultados ([ADR-0016]).
    ///
    /// [ADR-0016]: ../../../docs/adr/0016-la-prueba-se-elige-el-indice-no-se-filtra.md
    pub highlight: Option<(usize, usize)>,
    /// La miniatura, ya como `data:` URI. Se genera **en el núcleo**: dejar que
    /// la ventana leyera el archivo por su cuenta pondría la guarda de la nube
    /// dentro de un webview, que es el sitio donde no puede estar.
    pub image: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Por qué no hay nada que enseñar, dicho con todas las letras.
    pub reason: Option<String>,
    /// Es sensible, hay PIN y no está desbloqueado: el contenido **no ha
    /// salido del núcleo** (ADR-0021). No es un borroso: no hay nada que
    /// destapar hasta que se escriba el PIN.
    pub locked: bool,
}

/// Qué se puede enseñar de esta entidad.
///
/// `query` sólo cambia **dónde se abre** el texto, nunca cuánto se puede leer.
pub fn of(
    store: &Store,
    id: &EntityId,
    query: Option<&str>,
    at: TimePoint,
    locked: bool,
) -> Result<Preview> {
    let texto_de = |p: &str| -> Result<Option<String>> {
        Ok(store
            .resolve(id, p, at)?
            .and_then(|a| a.object.as_text().map(str::to_string)))
    };

    let path = texto_de("path_observed_at")?;
    let etiqueta = texto_de("availability")?;
    let disponibilidad = etiqueta.as_deref().and_then(Availability::parse);
    let status = texto_de("extraction_status")?;

    let mut p = Preview {
        id: id.0.clone(),
        readable: disponibilidad.is_some_and(|a| a.may_read_content()),
        availability: etiqueta,
        sensitivity: store.sensitivity(id).map(|s| s.as_str()).unwrap_or("vault"),
        kind: "none",
        text: None,
        offset: 0,
        chars: 0,
        truncated: false,
        highlight: None,
        image: None,
        width: 0,
        height: 0,
        reason: None,
        locked: false,
        path,
    };

    // ── 0. Con el PIN puesto y sin desbloquear, lo sensible no se lee: ni el
    //       texto de la base ni, sobre todo, el archivo para la miniatura.
    if locked && p.sensitivity != "normal" {
        p.locked = true;
        p.reason = Some("protegido con tu PIN".to_string());
        return Ok(p);
    }

    // ── 1. El texto. No abre ningún archivo: sale de la proyección de M2.
    if let Some(doc) = obrain_extract::index::document_text(store.conn(), id)? {
        let (inicio, fin, marca) = window(&doc, query);
        p.kind = "text";
        p.chars = doc.chars().count();
        p.truncated = fin - inicio < doc.len();
        p.offset = inicio;
        p.highlight = marca;
        p.text = Some(doc[inicio..fin].to_string());
        // Una foto con texto reconocido (OCR) se sigue enseñando como foto:
        // el texto va con ella, debajo. Un escaneo se reconoce por la imagen.
        let es_imagen = p.path.as_deref().is_some_and(is_image);
        if !es_imagen {
            return Ok(p);
        }
    }

    // ── 2. La miniatura. **Aquí sí se abre un archivo**, así que la guarda va
    //       delante. Un archivo en la nube no llega nunca a `thumbnail`.
    if p.readable {
        if let Some(ruta) = p.path.as_deref().filter(|r| is_image(r)) {
            match thumbnail(Path::new(ruta)) {
                Some((datos, w, h)) => {
                    p.kind = "image";
                    p.image = Some(datos);
                    p.width = w;
                    p.height = h;
                    return Ok(p);
                }
                // Que no se pueda decodificar no es un hueco: es una respuesta.
                None => {
                    p.reason = Some(
                        "es una imagen, pero no se pudo abrir o el formato no se reconoce"
                            .to_string(),
                    );
                    return Ok(p);
                }
            }
        }
    }

    // En la papelera (ADR-0025) ya no hay archivo que abrir: sólo lo que se
    // leyó de él, si se leyó algo.
    if p.path.is_none() && p.availability.is_some() {
        p.reason =
            Some("ya no está en tu bóveda, y de este archivo no se había leído texto".to_string());
        return Ok(p);
    }
    p.reason = Some(reason(
        disponibilidad,
        p.availability.is_some(),
        status.as_deref(),
    ));
    Ok(p)
}

/// Por qué no hay nada que enseñar, en las palabras que le sirven a quien mira.
///
/// El orden importa: **la nube manda sobre todo lo demás**. Un `.docx`
/// deshidratado también es «formato sin texto extraído», y decírselo así sería
/// técnicamente cierto y engañoso — sugeriría un fallo del extractor cuando lo
/// que pasa es que el archivo no está en esta máquina.
fn reason(a: Option<Availability>, etiquetado: bool, status: Option<&str>) -> String {
    match a {
        Some(Availability::Local) => match status {
            Some("empty") => {
                "se leyó y no tenía texto: casi siempre es un documento escaneado, y el reconocedor de texto \
                 (OCR) de este equipo no encontró nada legible o no está disponible"
                    .to_string()
            }
            Some("binary") => "se leyó y no era texto, aunque lo pareciera por el nombre".to_string(),
            Some("minified") | Some("wordlist") => {
                "es texto, pero lo escribió una máquina: se dejó fuera del índice a propósito"
                    .to_string()
            }
            Some("too_large") => "es demasiado grande para extraerlo entero".to_string(),
            Some("failed") => "el extractor no pudo con este archivo".to_string(),
            _ => "de este formato no se extrae texto todavía".to_string(),
        },
        Some(_) => {
            "los bytes están en tu nube: leerlos aquí los descargaría, así que no se han tocado"
                .to_string()
        }
        None if etiquetado => {
            "no se reconoce el estado de este archivo, así que no se abre".to_string()
        }
        None => "esta entidad no es un archivo con contenido".to_string(),
    }
}

/// Qué tramo del documento se abre, y dónde está lo que explica la consulta.
///
/// Sin consulta se abre por el principio, que es lo que espera quien va
/// bajando por el explorador. Con consulta se abre **donde está la respuesta**:
/// [ADR-0016] fijó que la prueba se elige y no se recorta por el principio, y
/// enseñar los primeros 40.000 bytes de un documento de 300.000 sería cometer
/// otra vez el mismo error, sólo que con un presupuesto más grande.
fn window(doc: &str, query: Option<&str>) -> (usize, usize, Option<(usize, usize)>) {
    let ancla = query
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .and_then(|q| obrain_extract::evidence::pick(doc, q, obrain_extract::evidence::BUDGET));

    let inicio = match &ancla {
        // Sólo se mueve la ventana si la prueba se quedaría fuera. Mientras
        // quepa, se abre por el principio: es más fácil de leer y no hace falta
        // explicar por qué el documento empieza a media frase.
        Some(ev) if ev.byte_end > TEXT_BUDGET => {
            floor(doc, ev.byte_start.saturating_sub(TEXT_BUDGET / 4))
        }
        _ => 0,
    };
    let fin = ceil(doc, (inicio + TEXT_BUDGET).min(doc.len()));

    let marca = ancla.and_then(|ev| {
        (ev.byte_start >= inicio && ev.byte_end <= fin)
            .then_some((ev.byte_start - inicio, ev.byte_end - inicio))
    });
    (inicio, fin, marca)
}

/// Retrocede hasta un límite de carácter. Rebanar UTF-8 por la mitad es un
/// pánico, no un resultado peor.
fn floor(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// ¿Merece la pena intentar decodificar esto como imagen?
///
/// Por extensión y no por contenido, y es deliberado: mirar los bytes mágicos
/// exigiría abrir el archivo, y abrirlo es exactamente lo que la guarda decide
/// si se puede hacer. La comprobación barata va antes que la cara.
fn is_image(path: &str) -> bool {
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    IMAGE_EXT.contains(&ext.as_str())
}

/// Decodifica, reduce y devuelve la miniatura como `data:` URI.
///
/// Devuelve `None` ante cualquier problema —no existe, no se puede leer, el
/// formato miente, es demasiado grande— porque el llamante ya tiene una
/// pantalla para «no lo sé» y un pánico aquí se llevaría la ventana entera por
/// un archivo corrupto.
///
/// **Sólo se llama detrás de la guarda de M1.** Que esta función no la
/// comprueba es intencionado: una guarda repetida en dos sitios es una guarda
/// que un día se cambia en uno solo.
fn thumbnail(path: &Path) -> Option<(String, u32, u32)> {
    if std::fs::metadata(path).ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    // Reducir sí, agrandar no. Un icono de 16 px estirado a 720 se ve peor que
    // el icono, y ocupa cien veces más en el puente para no enseñar nada más.
    let img = if img.width() > THUMB || img.height() > THUMB {
        img.thumbnail(THUMB, THUMB)
    } else {
        img
    };

    // JPEG salvo que la transparencia haga falta **de verdad**, y esa
    // diferencia está medida, no supuesta: una captura de pantalla real del
    // corpus se decodifica como RGBA aunque sea completamente opaca, así que
    // preguntar por el *tipo de color* mandaba al PNG el formato más común del
    // disco — 646 KB en base64 por cada pulsación de la flecha del explorador.
    // Preguntar por los píxeles cuesta un recorrido de la miniatura, que ya
    // está reducida, y sólo deja en PNG lo que se vería como un recuadro negro
    // si se aplastara: iconos y logotipos recortados.
    let (formato, mime) = if usa_transparencia(&img) {
        (image::ImageFormat::Png, "image/png")
    } else {
        (image::ImageFormat::Jpeg, "image/jpeg")
    };
    // JPEG no sabe de canal alfa y `write_to` con RGBA da error en vez de
    // aplastarlo. Se aplasta aquí, que además es lo que se quiere decir.
    let img = if mime == "image/jpeg" && img.color().has_alpha() {
        image::DynamicImage::ImageRgb8(img.to_rgb8())
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, formato).ok()?;
    let datos = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Some((
        format!("data:{mime};base64,{datos}"),
        img.width(),
        img.height(),
    ))
}

/// ¿Hay algún píxel que no sea opaco?
///
/// Se pregunta por los píxeles y no por el tipo de color porque **casi todo lo
/// que trae alfa no lo usa**: las capturas de pantalla de Windows se decodifican
/// como RGBA con el canal a 255 de punta a punta. Es un recorrido de la
/// miniatura ya reducida, no del original.
fn usa_transparencia(img: &image::DynamicImage) -> bool {
    use image::GenericImageView as _;
    img.color().has_alpha() && img.pixels().any(|(_, _, px)| px.0[3] != u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_extension_decide_si_se_intenta_decodificar() {
        assert!(is_image(r"C:\Users\x\Capturas\captura.PNG"));
        assert!(is_image("/home/x/foto.jpeg"));
        assert!(!is_image(r"C:\Users\x\Documentos\visa.pdf"));
        assert!(!is_image("sin_extension"));
    }

    #[test]
    fn sin_consulta_el_documento_se_abre_por_el_principio() {
        let doc = "a".repeat(TEXT_BUDGET * 2);
        let (i, f, marca) = window(&doc, None);
        assert_eq!((i, f), (0, TEXT_BUDGET));
        assert_eq!(marca, None);
    }

    /// [ADR-0016] llevado al panel: si la respuesta está en el byte 200.000, un
    /// presupuesto que empieza en cero enseña un documento sin la parte que lo
    /// explica — que es el mismo fallo con otro tamaño.
    #[test]
    fn con_consulta_la_ventana_va_donde_esta_la_respuesta() {
        let relleno = "palabra de relleno. ".repeat(4_000);
        let doc = format!("{relleno}el aguacate maduro. {relleno}");
        assert!(doc.len() > TEXT_BUDGET, "hace falta que no quepa entero");

        let (i, f, marca) = window(&doc, Some("aguacate"));
        assert!(i > 0, "no movió la ventana");
        let (s, e) = marca.expect("hay tramo marcado");
        let visto = &doc[i..f];
        assert!(visto.contains("aguacate"));
        assert!(
            visto[s..e].contains("aguacate"),
            "la marca apunta a otro sitio: {:?}",
            &visto[s..e]
        );
    }

    /// Mientras la prueba quepa dentro del presupuesto, el documento se abre
    /// por el principio: es más fácil de leer y no hay que explicar por qué
    /// empieza a media frase.
    #[test]
    fn si_la_prueba_cabe_no_se_mueve_la_ventana() {
        let doc = format!("el aguacate maduro. {}", "relleno. ".repeat(9_000));
        let (i, _, marca) = window(&doc, Some("aguacate"));
        assert_eq!(i, 0);
        assert!(marca.is_some());
    }

    #[test]
    fn no_se_parte_un_caracter_multibyte() {
        let doc = "áéíóú ñ € ".repeat(20_000);
        for q in [None, Some("euro"), Some("ñ")] {
            let (i, f, _) = window(&doc, q);
            assert!(doc.is_char_boundary(i) && doc.is_char_boundary(f));
            let _ = &doc[i..f]; // rebanar no puede entrar en pánico
        }
    }

    /// La nube manda sobre el estado de extracción. Decirle a alguien que «de
    /// este formato no se extrae texto» cuando lo que pasa es que el archivo no
    /// está en la máquina sugiere un fallo del programa donde no lo hay.
    #[test]
    fn la_nube_manda_sobre_el_estado_de_extraccion() {
        let r = reason(
            Some(Availability::CloudDehydrated),
            true,
            Some("unsupported"),
        );
        assert!(r.contains("nube"), "{r}");
        assert!(r.contains("descargaría"), "{r}");
    }

    #[test]
    fn una_disponibilidad_desconocida_se_dice_como_lo_que_es() {
        let r = reason(None, true, None);
        assert!(r.contains("no se reconoce"), "{r}");
    }

    #[test]
    fn un_pdf_escaneado_no_es_un_error() {
        let r = reason(Some(Availability::Local), true, Some("empty"));
        assert!(r.contains("escaneado"), "{r}");
    }
}
