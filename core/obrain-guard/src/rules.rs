//! Señales de **nombre**. Se deciden sin abrir un solo archivo.
//!
//! Es la mitad barata del clasificador y la única que funciona sobre lo que no
//! se puede leer: los 14 documentos de Office deshidratados en OneDrive, los
//! `.pfx` que son binarios, las fotos de un pasaporte. Si la sensibilidad
//! dependiera de extraer texto, precisamente los archivos que hay que proteger
//! —los que están cifrados, en la nube o escaneados— serían los que se
//! quedarían en `normal`.
//!
//! ## Por qué hay dos formas de casar un nombre
//!
//! `dni` es una señal fuerte y son tres letras. Buscarlo como subcadena marca
//! `midnight.log` como documento de identidad —m-i-**d-n-i**-g-h-t—, y ése es
//! el tipo de error que destruye la confianza en un control de seguridad. Así
//! que los términos cortos exigen frontera de palabra y los largos y
//! distintivos (`pasaporte`, `password`) van por subcadena, porque
//! `mispasswords.txt` sí debería casar.

use obrain_core::Sensitivity;

/// Una señal que ha disparado. El `rule` viaja al log como evidencia: es lo
/// que permite responder "¿por qué crees que esto es una clave?" sin volver a
/// mirar el archivo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signal {
    pub rule: &'static str,
    pub level: Sensitivity,
    pub confidence: f64,
}

impl Signal {
    const fn new(rule: &'static str, level: Sensitivity, confidence: f64) -> Self {
        Signal { rule, level, confidence }
    }
}

// ─────────────────────────────────────────────────────────── tablas de señales

/// Extensiones cuyo **único propósito** es guardar material criptográfico.
///
/// `key` es el caso incómodo: en un Mac es una presentación de Keynote. En un
/// corpus de Windows la probabilidad es despreciable y el coste de equivocarse
/// —quedar fuera del índice vectorial— es reversible con un clic. Se deja, y
/// se dice aquí en vez de descubrirlo el día del port.
const VAULT_EXT: &[&str] = &[
    "pem", "key", "ppk", "pfx", "p12", "jks", "keystore", "kdbx", "kdb", "gpg", "ovpn",
];

/// Nombres completos (sin extensión) que son secretos por convención universal.
const VAULT_STEM: &[&str] = &[
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "credentials",
    ".netrc",
    "_netrc",
    ".pgpass",
    "htpasswd",
    ".htpasswd",
    ".npmrc",
];

/// Términos largos y distintivos. Subcadena.
const VAULT_SUBSTR: &[&str] = &[
    "password",
    "contrasena",
    "passwd",
    "credencial",
    "credential",
    "secreto",
    "secrets",
    "apikey",
    "api_key",
    "api-key",
    "clave privada",
    "private key",
    "seed phrase",
    "frase semilla",
    "recovery code",
    "codigo de recuperacion",
    "backup code",
    "wallet",
];

/// Documentos de identidad, salud y dinero. Subcadena.
///
/// `contract` **no** está y `contrato` sí. El inglés aparece en cualquier
/// repositorio de código —`contract tests`, `smart contract`, `IContract.cs`—
/// y marcaría medio disco de un programador. El castellano, en la práctica, no.
const SENSITIVE_SUBSTR: &[&str] = &[
    "pasaporte",
    "passport",
    "cedula",
    "carnet de identidad",
    "certificado de nacimiento",
    "acta de nacimiento",
    "birth certificate",
    "licencia de conducir",
    "permiso de residencia",
    "tarjeta sanitaria",
    "historia clinica",
    "historial medico",
    "informe medico",
    "receta medica",
    "medical record",
    "nomina",
    "payslip",
    "payroll",
    "finiquito",
    "declaracion de la renta",
    "tax return",
    "extracto bancario",
    "bank statement",
    "estado de cuenta",
    "tarjeta de credito",
    "credit card",
    "hipoteca",
    "contrato",
    "poliza",
];

/// Siglas. Frontera de palabra **obligatoria**.
///
/// `rfc` se cayó de esta lista al escribirla: en un disco con código, «RFC» son
/// los Request For Comments, no el registro fiscal mexicano. Una sigla que
/// significa dos cosas no es una señal.
const SENSITIVE_TOKEN: &[&str] = &[
    "dni", "nie", "nif", "cif", "curp", "ine", "ssn", "visa", "iban", "irpf",
];

// ──────────────────────────────────────────────────────────────── comparación

/// Minúsculas y sin diacríticos. Es el mismo criterio que
/// `remove_diacritics 2` del índice FTS5: sin esto, `contraseña` y
/// `contrasena` son dos cosas distintas y la mitad de las reglas no dispara en
/// un corpus en castellano.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// ¿Aparece `token` delimitado por algo que no sea alfanumérico?
fn has_token(hay: &str, token: &str) -> bool {
    let b = hay.as_bytes();
    hay.match_indices(token).any(|(i, _)| {
        let antes = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        let j = i + token.len();
        let despues = j == b.len() || !b[j].is_ascii_alphanumeric();
        antes && despues
    })
}

/// Divide un nombre de archivo en `(raíz, extensión)`, ambos plegados.
///
/// `.env` no tiene extensión: es todo raíz. Es justo el archivo que más
/// importa, así que el caso del punto inicial no puede ser un descuido.
fn split_name(name: &str) -> (String, String) {
    let folded = fold(name);
    match folded.rfind('.') {
        Some(i) if i > 0 => (folded[..i].to_string(), folded[i + 1..].to_string()),
        _ => (folded, String::new()),
    }
}

// ────────────────────────────────────────────────────────────────── reglas

/// Sufijos que, por convención universal, marcan un archivo **de plantilla**:
/// tiene la forma del secreto y ninguno de sus valores.
///
/// Lo encontró la auditoría de la primera pasada real: de los siete archivos
/// que acabaron en bóveda, **tres eran `.env.example`**. Sacarlos del índice
/// vectorial no protege nada —dentro pone `your_password_here`— y sí empobrece
/// el índice de un repositorio de código. Que el nombre ceda ante la plantilla
/// no abre un agujero: si un `.env.example` lleva de verdad una credencial
/// dentro, [`crate::content`] la ve igual, y con más razón que el nombre.
const PLANTILLA_EXT: &[&str] = &["example", "sample", "template", "dist", "tpl"];

/// Clasifica por el **nombre del archivo**. `None` = ninguna señal.
pub fn classify_name(name: &str) -> Option<Signal> {
    let (stem, ext) = split_name(name);

    if PLANTILLA_EXT.contains(&ext.as_str()) {
        return None;
    }

    // `id_rsa.pub` es la clave *pública*. Publicarla es literalmente para lo
    // que existe. Marcarla como bóveda sería el clasificador confundiendo
    // «criptografía» con «secreto».
    if ext != "pub" {
        if VAULT_STEM.contains(&stem.as_str()) {
            return Some(Signal::new("nombre:material-de-clave", Sensitivity::Vault, 0.95));
        }
        if VAULT_EXT.contains(&ext.as_str()) {
            return Some(Signal::new("extension:material-de-clave", Sensitivity::Vault, 0.90));
        }
        // `.env`, `.env.local`, `.env.production`: el archivo donde por
        // convención vive todo lo que no debe estar en el repositorio.
        if stem == ".env" || stem.starts_with(".env.") || ext == "env" {
            return Some(Signal::new("nombre:dotenv", Sensitivity::Vault, 0.90));
        }
    }

    let full = fold(name);
    if let Some(t) = VAULT_SUBSTR.iter().find(|t| full.contains(**t)) {
        return Some(Signal::new(vault_substr_rule(t), Sensitivity::Vault, 0.80));
    }
    if let Some(t) = SENSITIVE_SUBSTR.iter().find(|t| full.contains(**t)) {
        return Some(Signal::new(sensitive_substr_rule(t), Sensitivity::Sensitive, 0.70));
    }
    if SENSITIVE_TOKEN.iter().any(|t| has_token(&full, t)) {
        return Some(Signal::new("nombre:sigla-de-identidad", Sensitivity::Sensitive, 0.65));
    }
    None
}

/// Clasifica por el nombre de la **carpeta que lo contiene**, y sólo hasta
/// `sensitive`.
///
/// Una carpeta llamada `Pasaporte y visados` dice algo real sobre lo que hay
/// dentro, y es la señal que rescata a `IMG_2291.jpg` —un escaneo, sin texto y
/// sin nombre útil— que ninguna otra regla puede ver.
///
/// Se limita al **padre inmediato** y no sube por la ruta a propósito: una
/// carpeta `Contratos` a diez niveles de profundidad no debería teñir los
/// veinte mil archivos que cuelgan de ella. Y nunca llega a `vault`, porque el
/// contexto no es prueba: una carpeta llamada `Claves` puede contener el `.txt`
/// donde apuntaste el wifi de casa.
pub fn classify_parent(dir: &str) -> Option<Signal> {
    let folded = fold(dir);
    let hit = SENSITIVE_SUBSTR.iter().any(|t| folded.contains(*t))
        || SENSITIVE_TOKEN.iter().any(|t| has_token(&folded, t))
        || VAULT_SUBSTR.iter().any(|t| folded.contains(*t));
    hit.then(|| Signal::new("carpeta:contexto-sensible", Sensitivity::Sensitive, 0.55))
}

/// Los identificadores de regla son `&'static str` porque van al log y tienen
/// que ser estables entre ejecuciones. Agrupar por familia en vez de por
/// término evita que el vocabulario del log crezca con cada palabra nueva.
fn vault_substr_rule(t: &str) -> &'static str {
    match t {
        "wallet" | "seed phrase" | "frase semilla" => "nombre:monedero",
        "recovery code" | "codigo de recuperacion" | "backup code" => "nombre:codigo-de-respaldo",
        _ => "nombre:secreto",
    }
}

fn sensitive_substr_rule(t: &str) -> &'static str {
    match t {
        "nomina" | "payslip" | "payroll" | "finiquito" | "declaracion de la renta"
        | "tax return" | "extracto bancario" | "bank statement" | "estado de cuenta"
        | "tarjeta de credito" | "credit card" | "hipoteca" => "nombre:financiero",
        "historia clinica" | "historial medico" | "informe medico" | "receta medica"
        | "medical record" | "tarjeta sanitaria" => "nombre:salud",
        "contrato" | "poliza" => "nombre:contractual",
        _ => "nombre:identidad",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nivel(name: &str) -> Option<Sensitivity> {
        classify_name(name).map(|s| s.level)
    }

    #[test]
    fn el_material_de_clave_es_boveda() {
        assert_eq!(nivel("servidor.pem"), Some(Sensitivity::Vault));
        assert_eq!(nivel("id_rsa"), Some(Sensitivity::Vault));
        assert_eq!(nivel("copia.kdbx"), Some(Sensitivity::Vault));
        assert_eq!(nivel(".env"), Some(Sensitivity::Vault));
        assert_eq!(nivel(".env.production"), Some(Sensitivity::Vault));
    }

    /// Regresión de la primera pasada real: tres de los siete archivos que
    /// acabaron en bóveda eran `.env.example`. Una plantilla no es un secreto.
    #[test]
    fn una_plantilla_no_es_un_secreto() {
        assert_eq!(nivel(".env.example"), None);
        assert_eq!(nivel(".env.production.example"), None);
        assert_eq!(nivel("credentials.sample"), None);
        // …pero el `.env` de verdad sigue siéndolo.
        assert_eq!(nivel(".env.production"), Some(Sensitivity::Vault));
    }

    /// La clave pública existe para repartirla.
    #[test]
    fn la_clave_publica_no_es_boveda() {
        assert_eq!(nivel("id_rsa.pub"), None);
        assert_eq!(nivel("servidor.pub"), None);
    }

    /// Regresión del corpus real: el informe de M2 dejó anotado que en el
    /// índice léxico había entrado un archivo llamado `VPS password.txt`.
    /// Es el caso que motivó todo este crate.
    #[test]
    fn vps_password_txt_es_boveda() {
        assert_eq!(nivel("VPS password.txt"), Some(Sensitivity::Vault));
    }

    #[test]
    fn los_diacriticos_no_esconden_nada() {
        assert_eq!(nivel("Contraseña del router.txt"), Some(Sensitivity::Vault));
        assert_eq!(nivel("Nómina marzo.pdf"), Some(Sensitivity::Sensitive));
    }

    #[test]
    fn los_documentos_de_identidad_son_sensibles() {
        assert_eq!(nivel("Pasaporte escaneado.pdf"), Some(Sensitivity::Sensitive));
        assert_eq!(nivel("dni-frente.jpg"), Some(Sensitivity::Sensitive));
        assert_eq!(nivel("solicitud VISA 2019.pdf"), Some(Sensitivity::Sensitive));
    }

    /// El error que justifica que existan dos formas de casar: sin frontera de
    /// palabra, `midnight` contiene `dni`.
    #[test]
    fn una_sigla_corta_exige_frontera_de_palabra() {
        assert_eq!(nivel("midnight.log"), None);
        assert_eq!(nivel("visual-studio.lnk"), None);
        assert_eq!(nivel("define.h"), None);
    }

    /// `rfc-2616.txt` es un estándar de internet, no el registro fiscal
    /// mexicano. Por eso `rfc` no está en la tabla.
    #[test]
    fn las_siglas_ambiguas_no_son_una_senal() {
        assert_eq!(nivel("rfc-2616.txt"), None);
    }

    /// El inglés `contract` marcaría medio repositorio de código.
    #[test]
    fn contract_en_ingles_no_dispara() {
        assert_eq!(nivel("IContractResolver.cs"), None);
        assert_eq!(nivel("Contrato de alquiler.pdf"), Some(Sensitivity::Sensitive));
    }

    #[test]
    fn un_archivo_cualquiera_no_tiene_senal() {
        assert_eq!(nivel("informe trimestral.docx"), None);
        assert_eq!(nivel("IMG_2291.jpg"), None);
        assert_eq!(nivel("main.rs"), None);
    }

    /// La carpeta da contexto pero no llega nunca a bóveda.
    #[test]
    fn la_carpeta_solo_sube_hasta_sensible() {
        let s = classify_parent("Pasaporte y visados").expect("deberia disparar");
        assert_eq!(s.level, Sensitivity::Sensitive);
        // Incluso con un término de bóveda en el nombre de la carpeta.
        let s = classify_parent("Contrasenas").expect("deberia disparar");
        assert_eq!(s.level, Sensitivity::Sensitive);
        assert_eq!(classify_parent("Documentos"), None);
    }

    #[test]
    fn plegar_normaliza_mayusculas_y_tildes() {
        assert_eq!(fold("Nómina_MARZO.pdf"), "nomina_marzo.pdf");
        assert_eq!(fold("CONTRASEÑA"), "contrasena");
    }
}
