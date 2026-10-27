//! Señales de **contenido**, sobre el texto que M2 ya extrajo.
//!
//! Este módulo **no abre ningún archivo**. Lee la tabla `chunks`, que es una
//! proyección que ya existe. Es lo que le permite correr después de M2 sin
//! reabrir 460 archivos y, sobre todo, sin poder hidratar nada de OneDrive ni
//! por accidente: no tiene acceso al disco.
//!
//! ## La regla que gobierna el módulo entero
//!
//! **Ninguna señal viaja con el texto que la disparó.** [`Signal`] no tiene
//! campo para el fragmento encontrado, y eso es deliberado: un clasificador
//! que copia la clave de API que acaba de encontrar dentro del registro de
//! auditoría —que es append-only e inmutable por I1/I4— habría creado una fuga
//! permanente mientras intentaba evitarla. La evidencia que se guarda es
//! *dónde mirar*: identificador de regla y ordinal del fragmento.
//!
//! ## Por qué casi todas las reglas llevan suma de verificación
//!
//! «Veinte dígitos seguidos» está en cualquier volcado de datos. Un IBAN,
//! en cambio, lleva un módulo 97 y un DNI español una letra de control
//! calculada: comprobarlos convierte una heurística con 4 % de falsos
//! positivos en algo que se equivoca una vez entre miles. Cuando una señal no
//! tiene forma de verificarse, se apoya en un prefijo que ningún texto humano
//! produce por casualidad (`AKIA`, `ghp_`, `-----BEGIN`).

use crate::rules::Signal;
use obrain_core::Sensitivity;
use std::ops::Range;

/// La señal más fuerte que aparezca en el texto, o `None`.
///
/// Las de bóveda se comprueban primero y cortan: si hay una clave privada
/// dentro, da igual que además haya un IBAN.
pub fn scan(text: &str) -> Option<Signal> {
    locate(text).map(|(s, _)| s)
}

/// Como [`scan`], y además **dónde**: el tramo de bytes de `text` que disparó
/// la señal.
///
/// El tramo no viaja al log —la evidencia sigue siendo una dirección, regla y
/// fragmento—: se recalcula al enseñar el «¿por qué?», sobre el texto que ya
/// está en `chunks`, para marcarlo en la vista previa como los vencimientos.
pub fn locate(text: &str) -> Option<(Signal, Range<usize>)> {
    vault_signal(text).or_else(|| sensitive_signal(text))
}

// ────────────────────────────────────────────────────────────────── bóveda

fn vault_signal(text: &str) -> Option<(Signal, Range<usize>)> {
    // Una clave privada en PEM no se puede confundir con nada.
    if let Some(r) = find_pem(text) {
        return Some((
            sig("contenido:clave-privada-pem", Sensitivity::Vault, 0.99),
            r,
        ));
    }
    if let Some(r) = prefix_then_run(text, "AKIA", 16, 16, is_upper_alnum) {
        return Some((sig("contenido:clave-de-aws", Sensitivity::Vault, 0.95), r));
    }
    for p in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if let Some(r) = prefix_then_run(text, p, 30, 255, is_token_char) {
            return Some((
                sig("contenido:token-de-github", Sensitivity::Vault, 0.95),
                r,
            ));
        }
    }
    for p in ["xoxb-", "xoxp-", "xoxa-", "xoxs-", "xoxr-"] {
        if let Some(r) = prefix_then_run(text, p, 10, 255, |c| is_token_char(c) || c == b'-') {
            return Some((sig("contenido:token-de-slack", Sensitivity::Vault, 0.95), r));
        }
    }
    if let Some(r) = prefix_then_run(text, "AIza", 35, 35, is_token_char) {
        return Some((
            sig("contenido:clave-de-google", Sensitivity::Vault, 0.95),
            r,
        ));
    }
    // `sk-` es corto; lo que lo hace fiable es exigir 32 caracteres de token
    // detrás sin un espacio.
    if let Some(r) = prefix_then_run(text, "sk-", 32, 255, is_token_char) {
        return Some((
            sig("contenido:clave-de-modelo", Sensitivity::Vault, 0.90),
            r,
        ));
    }
    if let Some(r) = find_jwt(text) {
        return Some((sig("contenido:jwt", Sensitivity::Vault, 0.85), r));
    }
    if let Some(r) = find_env_secret(text) {
        return Some((
            sig("contenido:variable-con-secreto", Sensitivity::Vault, 0.90),
            r,
        ));
    }
    if let Some(r) = find_url_password(text) {
        return Some((
            sig("contenido:url-con-contrasena", Sensitivity::Vault, 0.85),
            r,
        ));
    }
    None
}

// ────────────────────────────────────────────────────────────────── sensible

fn sensitive_signal(text: &str) -> Option<(Signal, Range<usize>)> {
    if let Some(r) = find_iban(text) {
        return Some((sig("contenido:iban", Sensitivity::Sensitive, 0.90), r));
    }
    if let Some(r) = find_dni(text) {
        return Some((sig("contenido:dni-o-nie", Sensitivity::Sensitive, 0.85), r));
    }
    if let Some(r) = find_card(text) {
        return Some((
            sig("contenido:tarjeta-de-pago", Sensitivity::Sensitive, 0.80),
            r,
        ));
    }
    None
}

/// `-----BEGIN … PRIVATE KEY-----`: de la cabecera al final de su marcador.
fn find_pem(text: &str) -> Option<Range<usize>> {
    let fin_marca = "PRIVATE KEY-----";
    if !(text.contains("-----BEGIN") && text.contains(fin_marca)) {
        return None;
    }
    let i = text.find("-----BEGIN")?;
    let f = text[i..]
        .find(fin_marca)
        .map_or(i + "-----BEGIN".len(), |k| i + k + fin_marca.len());
    Some(i..f)
}

const fn sig(rule: &'static str, level: Sensitivity, confidence: f64) -> Signal {
    Signal {
        rule,
        level,
        confidence,
    }
}

// ──────────────────────────────────────────────────────────── primitivas

fn is_upper_alnum(c: u8) -> bool {
    c.is_ascii_digit() || c.is_ascii_uppercase()
}

/// Alfabeto de los tokens de proveedor y de base64url.
fn is_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// ¿Hay un `prefix` seguido de entre `min` y `max` caracteres de `pred`?
///
/// El límite superior importa tanto como el inferior: `AKIA` seguido de 200
/// mayúsculas es un volcado hexadecimal, no una credencial de AWS.
fn prefix_then_run(
    text: &str,
    prefix: &str,
    min: usize,
    max: usize,
    pred: fn(u8) -> bool,
) -> Option<Range<usize>> {
    let b = text.as_bytes();
    text.match_indices(prefix).find_map(|(i, _)| {
        let start = i + prefix.len();
        let mut n = 0;
        while start + n < b.len() && pred(b[start + n]) {
            n += 1;
        }
        (n >= min && n <= max).then_some(i..start + n)
    })
}

/// `eyJ…·eyJ…·` — cabecera y cuerpo de un JWT, ambos base64 de un `{`.
fn find_jwt(text: &str) -> Option<Range<usize>> {
    let b = text.as_bytes();
    text.match_indices("eyJ").find_map(|(i, _)| {
        let mut j = i;
        while j < b.len() && is_token_char(b[j]) {
            j += 1;
        }
        // cabecera de al menos 10 caracteres, y detrás un punto y otro `eyJ`
        (j - i >= 10 && b.get(j) == Some(&b'.') && b[j + 1..].starts_with(b"eyJ")).then_some(i..j)
    })
}

/// Valores que están en un `.env` para ser sustituidos, no para ser usados.
fn is_placeholder(v: &str) -> bool {
    let v = v.trim().trim_matches(['"', '\'']);
    if v.len() < 8 {
        return true;
    }
    let low = v.to_ascii_lowercase();
    if v.starts_with('<') || v.starts_with("${") || v.starts_with('$') {
        return true;
    }
    // Una sola letra repetida: `xxxxxxxx`, `--------`.
    if v.bytes().all(|c| c == v.as_bytes()[0]) {
        return true;
    }
    [
        "your",
        "changeme",
        "change_me",
        "example",
        "placeholder",
        "todo",
        "dummy",
        "sample",
    ]
    .iter()
    .any(|m| low.contains(m))
}

/// `NOMBRE_EN_MAYUSCULAS=valor` donde el nombre habla de un secreto.
///
/// Exigir el nombre en `UPPER_SNAKE` es lo que separa una línea de `.env` de
/// `const token = fetchToken()` en un `.ts`. El precio es que un `.ini` con
/// claves en minúscula se escapa; queda anotado como límite conocido y no
/// escondido detrás de una heurística de entropía que nadie podría auditar.
fn find_env_secret(text: &str) -> Option<Range<usize>> {
    const PALABRAS: &[&str] = &[
        "PASSWORD",
        "PASSWD",
        "SECRET",
        "TOKEN",
        "APIKEY",
        "API_KEY",
        "PRIVATE_KEY",
        "ACCESS_KEY",
        "CLAVE",
        "CREDENTIAL",
    ];
    let mut desde = 0;
    for bruta in text.split_inclusive('\n') {
        let inicio = desde;
        desde += bruta.len();
        let sin_salto = bruta.trim_end_matches(['\n', '\r']);
        let line = sin_salto.trim();
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_start_matches("export ").trim();
        let bien_formado = !key.is_empty()
            && key
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            && key.bytes().any(|c| c.is_ascii_uppercase());
        if bien_formado && PALABRAS.iter().any(|p| key.contains(p)) && !is_placeholder(value) {
            let izq = sin_salto.len() - sin_salto.trim_start().len();
            return Some(inicio + izq..inicio + sin_salto.trim_end().len());
        }
    }
    None
}

/// `postgres://usuario:contrasena@host` — la credencial más filtrada del mundo.
fn find_url_password(text: &str) -> Option<Range<usize>> {
    text.match_indices("://").find_map(|(i, _)| {
        let resto = &text[i + 3..];
        let fin = resto
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(resto.len());
        let autoridad = &resto[..fin];
        let ok = match (autoridad.find(':'), autoridad.find('@')) {
            (Some(c), Some(a)) if c < a => {
                let pass = &autoridad[c + 1..a];
                // Ni vacía, ni un marcador, ni un número de puerto.
                !pass.is_empty()
                    && pass.len() >= 4
                    && !pass.bytes().all(|c| c.is_ascii_digit())
                    && !is_placeholder(pass)
            }
            _ => false,
        };
        ok.then_some(i + 3..i + 3 + fin)
    })
}

// ───────────────────────────────────────────────────── sumas de verificación

/// Países con IBAN. Exigir un código real es la primera criba; el módulo 97 es
/// la segunda.
const IBAN_PAISES: &[&str] = &[
    "AD", "AE", "AT", "BE", "BG", "CH", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "GB", "GI",
    "GR", "HR", "HU", "IE", "IS", "IT", "LI", "LT", "LU", "LV", "MC", "MT", "NL", "NO", "PL", "PT",
    "RO", "RS", "SE", "SI", "SK", "SM", "TR", "UA",
];

fn find_iban(text: &str) -> Option<Range<usize>> {
    let b = text.as_bytes();
    for i in 0..b.len().saturating_sub(14) {
        // frontera por delante, y forma `LL DD`
        if i > 0 && b[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        if !(b[i].is_ascii_uppercase()
            && b[i + 1].is_ascii_uppercase()
            && b[i + 2].is_ascii_digit()
            && b[i + 3].is_ascii_digit())
        {
            continue;
        }
        if !IBAN_PAISES.contains(&&text[i..i + 2]) {
            continue;
        }
        // Se admiten espacios entre grupos: así es como se escribe a mano.
        let mut limpio = String::with_capacity(34);
        let mut j = i;
        while j < b.len() && limpio.len() < 34 {
            if is_upper_alnum(b[j]) {
                limpio.push(b[j] as char);
                j += 1;
            } else if b[j] == b' ' && j + 1 < b.len() && is_upper_alnum(b[j + 1]) {
                j += 1;
            } else {
                break;
            }
        }
        if limpio.len() >= 15 && iban_mod97(&limpio) {
            return Some(i..j);
        }
    }
    None
}

/// Los cuatro primeros caracteres pasan al final, las letras valen 10..35, y
/// el número entero módulo 97 tiene que dar 1.
fn iban_mod97(s: &str) -> bool {
    let mut rem: u32 = 0;
    for c in s[4..].bytes().chain(s[..4].bytes()) {
        rem = if c.is_ascii_digit() {
            (rem * 10 + (c - b'0') as u32) % 97
        } else {
            (rem * 100 + (c - b'A') as u32 + 10) % 97
        };
    }
    rem == 1
}

const DNI_LETRAS: &[u8] = b"TRWAGMYFPDXBNJZSQVHLCKE";

/// DNI (`12345678Z`) y NIE (`X1234567L`), ambos con letra de control.
fn find_dni(text: &str) -> Option<Range<usize>> {
    let b = text.as_bytes();
    for i in 0..b.len() {
        if i > 0 && b[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        // El NIE antepone X, Y o Z, que valen 0, 1 y 2.
        let (prefijo, inicio) = match b[i] {
            b'X' => (0u32, i + 1),
            b'Y' => (1, i + 1),
            b'Z' => (2, i + 1),
            c if c.is_ascii_digit() => (u32::MAX, i),
            _ => continue,
        };
        let esperados = if prefijo == u32::MAX { 8 } else { 7 };
        if inicio + esperados >= b.len() {
            continue;
        }
        let digitos = &b[inicio..inicio + esperados];
        if !digitos.iter().all(u8::is_ascii_digit) {
            continue;
        }
        let letra = b[inicio + esperados];
        if !letra.is_ascii_uppercase() {
            continue;
        }
        // Y nada alfanumérico detrás: `12345678Z9` no es un DNI.
        if b.get(inicio + esperados + 1)
            .is_some_and(u8::is_ascii_alphanumeric)
        {
            continue;
        }
        let mut n: u32 = if prefijo == u32::MAX { 0 } else { prefijo };
        for d in digitos {
            n = n * 10 + (d - b'0') as u32;
        }
        if DNI_LETRAS[(n % 23) as usize] == letra {
            return Some(i..inicio + esperados + 1);
        }
    }
    None
}

/// Tarjeta de pago: Luhn **y** prefijo de emisor conocido.
///
/// Luhn sola acierta por casualidad una vez de cada diez, que en un disco lleno
/// de identificadores numéricos es demasiado. El prefijo es lo que la hace
/// utilizable.
fn find_card(text: &str) -> Option<Range<usize>> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && b[i - 1].is_ascii_alphanumeric()) {
            i += 1;
            continue;
        }
        // Se admiten grupos separados por un espacio o un guion, pero todos los
        // grupos deben tener 4 dígitos o más: así `1 2 3 4 5 6 7 8 9 0 1 2 3 4`
        // —catorce dígitos sueltos de cualquier tabla— no cuenta como tarjeta.
        let mut digitos = String::with_capacity(19);
        let mut j = i;
        let mut grupo = 0usize;
        let mut valido = true;
        while j < b.len() && digitos.len() < 19 {
            if b[j].is_ascii_digit() {
                digitos.push(b[j] as char);
                grupo += 1;
                j += 1;
            } else if (b[j] == b' ' || b[j] == b'-') && j + 1 < b.len() && b[j + 1].is_ascii_digit()
            {
                if grupo < 4 {
                    valido = false;
                    break;
                }
                grupo = 0;
                j += 1;
            } else {
                break;
            }
        }
        if valido
            && !b.get(j).is_some_and(u8::is_ascii_alphanumeric)
            && (13..=19).contains(&digitos.len())
            && emisor_conocido(&digitos)
            && luhn(&digitos)
        {
            return Some(i..j);
        }
        // saltar el número entero: no reprocesar sus dígitos uno a uno
        i = j.max(i + 1);
    }
    None
}

fn emisor_conocido(d: &str) -> bool {
    let dos: u32 = d[..2].parse().unwrap_or(0);
    d.starts_with('4')                        // Visa
        || (51..=55).contains(&dos)           // Mastercard
        || (dos == 34 || dos == 37)           // American Express
        || d.starts_with("6011")              // Discover
        || (dos == 35)                        // JCB
        || (dos == 62) // UnionPay
}

fn luhn(d: &str) -> bool {
    let suma: u32 = d
        .bytes()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let n = (c - b'0') as u32;
            if i % 2 == 1 {
                if n * 2 > 9 { n * 2 - 9 } else { n * 2 }
            } else {
                n
            }
        })
        .sum();
    suma % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nivel(t: &str) -> Option<Sensitivity> {
        scan(t).map(|s| s.level)
    }

    /// El tramo de [`locate`]: exactamente lo que disparó la señal, para
    /// marcarlo en la vista previa.
    #[test]
    fn locate_dice_donde_esta_lo_que_encontro() {
        fn tramo(t: &str) -> (&'static str, &str) {
            let (s, r) = locate(t).unwrap();
            (s.rule, &t[r])
        }
        assert_eq!(
            tramo("TARJETA DE IDENTIDAD\nNIE X1234567L  RESIDENCIA"),
            ("contenido:dni-o-nie", "X1234567L")
        );
        assert_eq!(
            tramo("Domiciliación: ES91 2100 0418 4502 0005 1332, a nombre de…"),
            ("contenido:iban", "ES91 2100 0418 4502 0005 1332")
        );
        assert_eq!(
            tramo("Pago con 4111 1111 1111 1111 el lunes"),
            ("contenido:tarjeta-de-pago", "4111 1111 1111 1111")
        );
        assert_eq!(
            tramo("# config\n  DB_PASSWORD=hunter2hunter2\nOTRA=1"),
            (
                "contenido:variable-con-secreto",
                "DB_PASSWORD=hunter2hunter2"
            )
        );
        assert_eq!(
            tramo("url postgres://jp:s3creta99@db.local/app"),
            ("contenido:url-con-contrasena", "jp:s3creta99@db.local")
        );
        assert!(locate("Apuntes de SQL con Oracle.").is_none());
    }

    #[test]
    fn una_clave_privada_es_boveda() {
        let t = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n";
        assert_eq!(nivel(t), Some(Sensitivity::Vault));
    }

    #[test]
    fn los_tokens_de_proveedor_son_boveda() {
        assert_eq!(
            nivel("AWS_ID = AKIAIOSFODNN7EXAMPLE"),
            Some(Sensitivity::Vault)
        );
        // Token sintético ensamblado dinámicamente para no activar escáneres estáticos de secretos.
        let token_github = format!("token: ghp_{}", "16C7e42F292c6912E7710c838347Ae178B4a");
        assert_eq!(nivel(&token_github), Some(Sensitivity::Vault));
        // Una clave de Google son 39 caracteres exactos: `AIza` + 35.
        let google = "AIzaSyD-1234567890abcdefghijklmnopqrstu";
        assert_eq!(google.len(), 39);
        assert_eq!(nivel(google), Some(Sensitivity::Vault));
    }

    /// Regresión del corpus real: el informe de M2 dejó anotado que en el
    /// índice habían entrado ficheros `.env` con variables de credenciales.
    #[test]
    fn un_env_con_credenciales_es_boveda() {
        let t = "# configuracion\nDATABASE_PASSWORD=Zx9!kqm2LpVt\nPORT=5432\n";
        assert_eq!(nivel(t), Some(Sensitivity::Vault));
    }

    #[test]
    fn un_env_de_ejemplo_no_lo_es() {
        let t = "DATABASE_PASSWORD=your_password_here\nAPI_KEY=<tu-clave>\nSECRET=xxxxxxxx\n";
        assert_eq!(nivel(t), None, "un .env.example no es un secreto");
    }

    /// `const token = ...` en un `.ts` no es una variable de entorno.
    #[test]
    fn el_codigo_en_minusculas_no_dispara_la_regla_de_env() {
        assert_eq!(nivel("const token = await getToken();"), None);
        assert_eq!(nivel("let api_key = process.env.API_KEY;"), None);
    }

    #[test]
    fn una_url_con_contrasena_es_boveda() {
        assert_eq!(
            nivel("postgres://admin:Tr0ub4dor@db.interno:5432/prod"),
            Some(Sensitivity::Vault)
        );
        assert_eq!(nivel("https://ejemplo.com/pagina"), None);
        assert_eq!(
            nivel("http://localhost:5432/salud"),
            None,
            "un puerto no es una clave"
        );
    }

    #[test]
    fn un_iban_valido_es_sensible() {
        // IBAN de prueba con módulo 97 correcto.
        assert_eq!(
            nivel("Cuenta: ES9121000418450200051332"),
            Some(Sensitivity::Sensitive)
        );
        assert_eq!(
            nivel("Cuenta: ES91 2100 0418 4502 0005 1332"),
            Some(Sensitivity::Sensitive)
        );
    }

    #[test]
    fn una_cadena_con_forma_de_iban_pero_sin_checksum_no_dispara() {
        assert_eq!(nivel("ES9121000418450200051333"), None);
        assert_eq!(nivel("XX9121000418450200051332"), None, "pais inexistente");
    }

    #[test]
    fn un_dni_valido_es_sensible() {
        assert_eq!(
            nivel("DNI 12345678Z del solicitante"),
            Some(Sensitivity::Sensitive)
        );
        assert_eq!(nivel("NIE X1234567L"), Some(Sensitivity::Sensitive));
    }

    /// Regresión de la primera pasada real, y el hallazgo del trabajo: el NIE
    /// no aparecía dentro de ningún documento, sino **incrustado en el nombre
    /// del expediente**, y ese nombre estaba citado en la documentación del
    /// propio repositorio. Un identificador no necesita un párrafo alrededor
    /// para ser un identificador.
    #[test]
    fn un_nie_dentro_del_nombre_de_un_expediente_se_detecta() {
        let t = "el archivo consultaExpte_X1234567L_2900202500332.pdf del tramite";
        assert_eq!(nivel(t), Some(Sensitivity::Sensitive));
    }

    #[test]
    fn ocho_digitos_con_una_letra_cualquiera_no_son_un_dni() {
        assert_eq!(nivel("codigo 12345678A"), None);
        assert_eq!(nivel("build 20240115B"), None);
    }

    #[test]
    fn una_tarjeta_valida_es_sensible() {
        assert_eq!(
            nivel("VISA 4111 1111 1111 1111"),
            Some(Sensitivity::Sensitive)
        );
        assert_eq!(nivel("4111111111111111"), Some(Sensitivity::Sensitive));
    }

    #[test]
    fn una_lista_de_numeros_no_es_una_tarjeta() {
        assert_eq!(nivel("1 2 3 4 5 6 7 8 9 0 1 2 3 4"), None);
        assert_eq!(
            nivel("1234567890123456"),
            None,
            "no pasa Luhn ni tiene emisor"
        );
        assert_eq!(nivel("timestamp 1735689600000 evento"), None);
    }

    #[test]
    fn el_texto_normal_no_dispara_nada() {
        let t = "El documento maestro describe la arquitectura del sistema. \
                 La version 2.0 se publico el 15 de enero de 2024 con 1.291 archivos.";
        assert_eq!(nivel(t), None);
    }

    /// La señal no lleva el texto encontrado. Es el invariante del módulo.
    #[test]
    fn la_senal_nunca_transporta_el_secreto() {
        let s = scan("DATABASE_PASSWORD=Zx9!kqm2LpVt").expect("deberia disparar");
        assert!(!s.rule.contains("Zx9"));
        assert_eq!(s.rule, "contenido:variable-con-secreto");
    }
}
