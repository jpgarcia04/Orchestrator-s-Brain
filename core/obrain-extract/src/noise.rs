//! Texto que es texto pero no es conocimiento.
//!
//! Auditar el índice real destapó que **doce archivos ocupaban el 62,5 % de
//! los fragmentos**, y ninguno de los doce lo había escrito una persona:
//! bundles de JavaScript minificado, volcados de datos de una aplicación,
//! documentación XML de una librería de terceros y listas de palabras.
//!
//! El maestro ya tenía anotado este problema abierto al cerrar M1: *"la
//! exclusión por lista fija tiene techo; hace falta un clasificador de 'esto
//! es dato de aplicación' basado en señales, no una lista que crece para
//! siempre"*. Este módulo es la primera aportación a ese clasificador, y sólo
//! incluye señales que se pueden defender una por una:
//!
//! - **Minificado** — una línea de 200 KB. Ningún humano escribe así.
//! - **Lista de palabras** — miles de líneas de un solo token. Es un
//!   diccionario, no un documento.
//!
//! Deliberadamente **no** intenta clasificar los casos dudosos —un
//! `package-lock.json`, la documentación de una librería—, porque una
//! heurística agresiva que se come un documento real cuesta mucho más que
//! unos cuantos fragmentos de más. Lo que queda fuera se cuenta en el informe
//! de M2 en vez de esconderse.

/// Por qué un texto no merece entrar en el índice léxico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Noise {
    /// Generado por una máquina para una máquina: bundles, mapas de fuentes.
    Minified,
    /// Un diccionario: `passwords.txt`, `surnames.txt`, `english_wikipedia.txt`.
    WordList,
}

impl Noise {
    pub fn as_str(self) -> &'static str {
        match self {
            Noise::Minified => "minified",
            Noise::WordList => "wordlist",
        }
    }
}

/// Por debajo de esto no se juzga nada: un archivo pequeño no puede contaminar
/// un índice ni aunque sea basura, y las heurísticas se equivocan más.
const MIN_BYTES: usize = 4096;

/// Longitud **media** de línea a partir de la cual el texto no lo escribió
/// nadie. Un párrafo largo de prosa ronda los 300-400 caracteres.
const MINIFIED_MEAN_LEN: usize = 400;

/// Longitud de **una sola** línea que ya delata a un generador de código.
///
/// La media no bastaba: medida contra el corpus real, `index-_Tu1Wy_w.js`
/// —1,6 MB de bundle de Vite— tiene 6.705 líneas y una media de 241
/// caracteres, por debajo del umbral. Pero su línea más larga son **77.789
/// caracteres**. Un bundle es un puñado de líneas gigantescas rodeadas de
/// licencias y sourcemaps; es el máximo, no la media, lo que lo identifica.
/// 10.000 caracteres en una línea son unas 1.500 palabras sin un solo salto.
const MINIFIED_MAX_LEN: usize = 10_000;

/// Mínimo de líneas para considerar que algo es una lista y no un texto con
/// varias líneas cortas seguidas.
const WORDLIST_MIN_LINES: usize = 200;
/// Qué proporción de las líneas tiene que ser un solo token.
const WORDLIST_RATIO: f64 = 0.9;
/// Y cómo de cortas: `passwords.txt` promedia 7 caracteres.
const WORDLIST_MAX_MEAN: f64 = 40.0;

/// Clasifica texto **ya normalizado**. `None` = es texto legítimo.
pub fn classify(text: &str) -> Option<Noise> {
    if text.len() < MIN_BYTES {
        return None;
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }

    let mean = text.len() as f64 / lines.len() as f64;
    let longest = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    if mean >= MINIFIED_MEAN_LEN as f64 || longest >= MINIFIED_MAX_LEN {
        return Some(Noise::Minified);
    }

    if lines.len() >= WORDLIST_MIN_LINES && mean <= WORDLIST_MAX_MEAN {
        let sueltas = lines
            .iter()
            .filter(|l| !l.trim().contains(char::is_whitespace))
            .count();
        if sueltas as f64 >= lines.len() as f64 * WORDLIST_RATIO {
            return Some(Noise::WordList);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_bundle_minificado_es_ruido() {
        // Una sola línea larguísima, como `index-_Tu1Wy_w.js` del corpus real.
        let t = format!("(function(){{{}}})();", "const a=1;b(a);".repeat(2_000));
        assert_eq!(classify(&t), Some(Noise::Minified));
    }

    #[test]
    fn una_lista_de_contrasenas_es_ruido() {
        // `passwords.txt` son 241 KB y 30.000 líneas de un token; aquí basta
        // con pasar de los 4 KB mínimos.
        let t = (0..500)
            .map(|i| format!("contrasenaComun{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(t.len() > MIN_BYTES);
        assert_eq!(classify(&t), Some(Noise::WordList));
    }

    /// Regresión del corpus real: un bundle de Vite tiene **media** de 241
    /// caracteres por línea —bajo el umbral— porque arrastra miles de líneas
    /// de licencias. Lo que lo delata es una sola línea de 77.789.
    #[test]
    fn un_bundle_con_media_baja_pero_una_linea_gigante_es_ruido() {
        let mut t = String::new();
        for _ in 0..3_000 {
            t.push_str("// Copyright (c) 2024 alguien. Licencia MIT.\n");
        }
        t.push_str(&"const a=1;".repeat(2_000)); // una linea de 20.000
        t.push('\n');
        let lineas = t.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            (t.len() as f64 / lineas as f64) < MINIFIED_MEAN_LEN as f64,
            "la media tiene que quedar por debajo del umbral"
        );
        assert_eq!(classify(&t), Some(Noise::Minified));
    }

    #[test]
    fn el_codigo_fuente_normal_no_es_ruido() {
        let t = r#"
/// Resuelve el valor unico de (sujeto, predicado) aplicando precedencia.
pub fn resolve(&self, subject: &EntityId, predicate: &str) -> Result<Option<Assertion>> {
    let sql = format!("SELECT {COLS} FROM assertions WHERE subject = :subject");
    Ok(self.conn.query_row(&sql, params, row_to_assertion).optional()?)
}
"#
        .repeat(40);
        assert_eq!(classify(&t), None, "el codigo fuente es contenido legitimo");
    }

    #[test]
    fn la_prosa_no_es_ruido_por_larga_que_sea() {
        let parrafo = "El documento maestro describe la arquitectura del sistema \
                       y las decisiones que ya no se discuten.\n\n";
        assert_eq!(classify(&parrafo.repeat(200)), None);
    }

    /// Un `.md` con una lista de viñetas es texto, no un diccionario: las
    /// viñetas llevan espacios.
    #[test]
    fn una_lista_de_la_compra_no_es_un_diccionario() {
        let t = (0..300)
            .map(|i| format!("- articulo numero {i} de la lista"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(classify(&t), None);
    }

    #[test]
    fn un_archivo_pequeno_nunca_es_ruido() {
        assert_eq!(classify(&"x".repeat(MIN_BYTES - 1)), None);
        assert_eq!(classify(""), None);
    }
}
