//! Triaje: decidir **antes de tocar un archivo** si se puede tocar.
//!
//! Ésta es la etapa que casi nadie implementa y que en Windows es crítica
//! (`PROJECT-MASTER.md` §4.3). Muchos archivos en disco **no tienen bytes**:
//! son placeholders de OneDrive/Dropbox. Abrirlos dispara la descarga.
//!
//! Todo lo de este módulo trabaja sobre los atributos que ya vienen en la
//! enumeración del directorio. **No abre ni un archivo.**

/// `FILE_ATTRIBUTE_*` de Win32. Se definen aquí en vez de importarlos para
/// que el módulo compile y se pueda testear en cualquier plataforma.
pub mod attr {
    pub const READONLY: u32 = 0x0000_0001;
    pub const HIDDEN: u32 = 0x0000_0002;
    pub const SYSTEM: u32 = 0x0000_0004;
    pub const DIRECTORY: u32 = 0x0000_0010;
    pub const REPARSE_POINT: u32 = 0x0000_0400;
    /// Contenido no disponible localmente (archivado).
    pub const OFFLINE: u32 = 0x0000_1000;
    /// **Abrir el archivo ya dispara la descarga.** Ni siquiera se puede
    /// pedir un handle para leer metadatos.
    pub const RECALL_ON_OPEN: u32 = 0x0004_0000;
    /// Se puede abrir sin coste; **leer los datos** dispara la descarga.
    pub const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
}

/// Disponibilidad real del contenido de un archivo.
///
/// La distinción entre los dos modos de nube no es un detalle: determina si
/// podemos siquiera pedir un handle. Confundirlos es lo que hace que un
/// indexador ingenuo descargue 40 GB sin avisar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Los bytes están en disco. Se puede abrir, identificar y hashear.
    Local,
    /// `RECALL_ON_DATA_ACCESS` — se puede abrir para metadatos e identidad,
    /// pero **leer el contenido lo descarga**.
    CloudDehydrated,
    /// `RECALL_ON_OPEN` — **abrirlo ya lo descarga**. No se toca.
    CloudPlaceholder,
    /// Marcado offline sin recall (cinta, archivado externo).
    Offline,
}

impl Availability {
    /// ¿Se puede pedir un handle sin provocar tráfico de red?
    pub fn may_open(self) -> bool {
        matches!(self, Availability::Local | Availability::CloudDehydrated)
    }

    /// ¿Se pueden leer los bytes sin provocar una descarga?
    pub fn may_read_content(self) -> bool {
        matches!(self, Availability::Local)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Availability::Local => "local",
            Availability::CloudDehydrated => "cloud_dehydrated",
            Availability::CloudPlaceholder => "cloud_placeholder",
            Availability::Offline => "offline",
        }
    }
}

/// Clasifica a partir de los atributos crudos. Función pura: no toca el disco.
///
/// El orden importa. `RECALL_ON_OPEN` se comprueba primero porque es el más
/// restrictivo, y un archivo puede llevar varios atributos a la vez.
pub fn classify(attributes: u32) -> Availability {
    if attributes & attr::RECALL_ON_OPEN != 0 {
        Availability::CloudPlaceholder
    } else if attributes & attr::RECALL_ON_DATA_ACCESS != 0 {
        Availability::CloudDehydrated
    } else if attributes & attr::OFFLINE != 0 {
        Availability::Offline
    } else {
        Availability::Local
    }
}

pub fn is_directory(attributes: u32) -> bool {
    attributes & attr::DIRECTORY != 0
}

/// Un punto de reanálisis que **no** es de sincronización en la nube: junction,
/// symlink. Recorrerlos invita a ciclos infinitos.
pub fn is_plain_reparse(attributes: u32) -> bool {
    attributes & attr::REPARSE_POINT != 0
        && attributes & (attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_archivo_normal_es_local() {
        assert_eq!(classify(0), Availability::Local);
        assert_eq!(classify(attr::READONLY | attr::HIDDEN), Availability::Local);
    }

    #[test]
    fn placeholder_de_onedrive_no_se_puede_ni_abrir() {
        let a = classify(attr::RECALL_ON_OPEN | attr::OFFLINE | attr::REPARSE_POINT);
        assert_eq!(a, Availability::CloudPlaceholder);
        assert!(!a.may_open(), "abrirlo dispararia la descarga");
        assert!(!a.may_read_content());
    }

    #[test]
    fn deshidratado_se_puede_abrir_pero_no_leer() {
        let a = classify(attr::RECALL_ON_DATA_ACCESS | attr::OFFLINE);
        assert_eq!(a, Availability::CloudDehydrated);
        assert!(a.may_open(), "se puede identificar sin descargar");
        assert!(!a.may_read_content(), "hashearlo lo descargaria");
    }

    #[test]
    fn recall_on_open_gana_a_recall_on_data_access() {
        // Un archivo con ambos atributos debe tratarse como el más restrictivo.
        let a = classify(attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS);
        assert_eq!(a, Availability::CloudPlaceholder);
        assert!(!a.may_open());
    }

    #[test]
    fn una_junction_no_es_un_placeholder() {
        assert!(is_plain_reparse(attr::REPARSE_POINT | attr::DIRECTORY));
        assert!(!is_plain_reparse(attr::REPARSE_POINT | attr::RECALL_ON_DATA_ACCESS));
    }
}
