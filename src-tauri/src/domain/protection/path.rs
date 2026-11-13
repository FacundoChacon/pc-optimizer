//! Normalizacion lexica pura de rutas Windows (design D-06).
//!
//! No toca el disco: solo decide si una cadena es una ruta de unidad interpretable sin
//! ambiguedad y la lleva a una forma comparable (`NormalizedPath`). Todo lo dudoso se rechaza
//! con un `PathRejection` (el servicio lo convierte en `Protected`).

use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Por que una cadena no es una ruta local interpretable (fail-closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathRejection {
    Empty,
    NonUnicode,
    NotAbsolute,
    NetworkPath,
    DevicePath,
    AlternateDataStream,
    InvalidCharacter,
    ReservedDeviceName,
    TrailingDotOrSpace,
    EscapesRoot,
    VerbatimNotNormalized,
}

/// Un componente de ruta: grafia original (para mostrar) y forma plegada (para comparar).
#[derive(Debug, Clone)]
pub struct Component {
    original: String,
    folded: String,
}

impl Component {
    /// Construye un componente ya validado.
    pub fn new(original: &str) -> Self {
        Self {
            original: original.to_string(),
            folded: fold_component(original),
        }
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn folded(&self) -> &str {
        &self.folded
    }
}

/// Ruta absoluta de unidad, sin `.`/`..`, comparable sin distinguir mayusculas como NTFS.
///
/// La igualdad y el hash usan la unidad y los componentes plegados; `Display` conserva la
/// grafia original.
#[derive(Debug, Clone)]
pub struct NormalizedPath {
    drive: char,
    components: Vec<Component>,
}

impl PartialEq for NormalizedPath {
    fn eq(&self, other: &Self) -> bool {
        self.drive == other.drive && self.same_folded(&other.components)
    }
}

impl Eq for NormalizedPath {}

impl Hash for NormalizedPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.drive.hash(state);
        for component in &self.components {
            component.folded.hash(state);
        }
    }
}

impl fmt::Display for NormalizedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:\\", self.drive)?;
        for (index, component) in self.components.iter().enumerate() {
            if index > 0 {
                f.write_str("\\")?;
            }
            f.write_str(&component.original)?;
        }
        Ok(())
    }
}

impl NormalizedPath {
    fn same_folded(&self, other: &[Component]) -> bool {
        self.components.len() == other.len()
            && self
                .components
                .iter()
                .zip(other)
                .all(|(a, b)| a.folded == b.folded)
    }

    /// Letra de unidad en mayuscula.
    pub fn drive(&self) -> char {
        self.drive
    }

    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// Cantidad de componentes (la raiz del volumen tiene 0).
    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Es la raiz del volumen (sin componentes).
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn is_volume_root(&self) -> bool {
        self.is_empty()
    }

    /// Componentes de `self` bajo `base` (vacio si son iguales); `None` si no esta debajo.
    pub fn strip_prefix(&self, base: &NormalizedPath) -> Option<&[Component]> {
        if self.drive != base.drive || self.components.len() < base.components.len() {
            return None;
        }
        let (head, rest) = self.components.split_at(base.components.len());
        base.same_folded(head).then_some(rest)
    }

    /// `self` es `base` o esta dentro de `base`.
    pub fn starts_with(&self, base: &NormalizedPath) -> bool {
        self.strip_prefix(base).is_some()
    }

    /// `self` esta dentro de `base` y no es `base`.
    pub fn is_strictly_under(&self, base: &NormalizedPath) -> bool {
        self.strip_prefix(base).is_some_and(|rest| !rest.is_empty())
    }

    /// Los primeros `count` componentes (0 = raiz del volumen).
    pub fn prefix(&self, count: usize) -> NormalizedPath {
        NormalizedPath {
            drive: self.drive,
            components: self.components.iter().take(count).cloned().collect(),
        }
    }

    /// Ruta del directorio padre; `None` en la raiz del volumen.
    pub fn parent(&self) -> Option<NormalizedPath> {
        (!self.components.is_empty()).then(|| self.prefix(self.components.len() - 1))
    }

    /// Agrega componentes fijos (constantes de las reglas; no se validan).
    pub fn joined(&self, parts: &[&str]) -> NormalizedPath {
        let mut components = self.components.clone();
        components.extend(parts.iter().map(|part| Component::new(part)));
        NormalizedPath {
            drive: self.drive,
            components,
        }
    }

    /// Forma Win32 como `PathBuf` (para el puerto de archivos).
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.to_string())
    }
}

/// Plegado de mayusculas como NTFS: uno a uno por caracter del plano basico.
///
/// `to_uppercase` solo se acepta si produce exactamente un caracter del BMP; asi `ß` no pasa
/// a `SS` ni se tocan caracteres fuera del BMP (la tabla `$UpCase` no los mapea).
pub fn fold_component(name: &str) -> String {
    name.chars().map(fold_char).collect()
}

fn fold_char(c: char) -> char {
    if u32::from(c) > 0xFFFF {
        return c;
    }
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) if u32::from(single) <= 0xFFFF => single,
        _ => c,
    }
}

/// Componente con forma de nombre corto 8.3 (`PROGRA~1`, `ANAMAR~1.TXT`).
///
/// Aproxima `^[^~.]{1,6}~[0-9]{1,6}(\.[^.]{0,3})?$`.
pub fn looks_short(component: &str) -> bool {
    let (stem, extension) = match component.split_once('.') {
        Some((stem, extension)) => (stem, Some(extension)),
        None => (component, None),
    };
    if extension.is_some_and(|ext| ext.contains('.') || ext.chars().count() > 3) {
        return false;
    }
    let Some((base, digits)) = stem.split_once('~') else {
        return false;
    };
    (1..=6).contains(&base.chars().count())
        && (1..=6).contains(&digits.len())
        && digits.chars().all(|c| c.is_ascii_digit())
}

/// La ruta real tiene la misma forma que la lexica: misma unidad, misma cantidad de componentes
/// y cada par igual (plegado) o con el componente lexico en forma corta 8.3.
pub fn same_shape(lexical: &NormalizedPath, real: &NormalizedPath) -> bool {
    lexical.drive == real.drive
        && lexical.components.len() == real.components.len()
        && lexical
            .components
            .iter()
            .zip(&real.components)
            .all(|(l, r)| l.folded == r.folded || looks_short(&l.original))
}

const SEPARATORS: [char; 2] = ['\\', '/'];

/// Interpreta `raw` como ruta absoluta de unidad.
pub fn normalize_lexical(raw: &str) -> Result<NormalizedPath, PathRejection> {
    if raw.is_empty() {
        return Err(PathRejection::Empty);
    }
    if raw.contains('\0') {
        return Err(PathRejection::InvalidCharacter);
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        return normalize_verbatim(rest);
    }
    if raw.starts_with(r"\??\") {
        return Err(PathRejection::DevicePath);
    }
    let mut chars = raw.chars();
    let (first, second) = (chars.next(), chars.next());
    if first.is_some_and(is_separator) && second.is_some_and(is_separator) {
        let third = chars.next();
        let fourth = chars.next();
        let is_device = matches!(third, Some('?' | '.')) && fourth.is_some_and(is_separator);
        return Err(if is_device {
            PathRejection::DevicePath
        } else {
            PathRejection::NetworkPath
        });
    }
    let (drive, rest) = split_drive(raw)?;
    let mut components: Vec<Component> = Vec::new();
    let mut raw_parts = Vec::new();
    for part in rest.split(SEPARATORS) {
        match part {
            "" | "." => {}
            ".." => raw_parts.push(None),
            name => {
                validate_component(name)?;
                raw_parts.push(Some(name));
            }
        }
    }
    for part in raw_parts {
        match part {
            Some(name) => components.push(Component::new(name)),
            None => {
                components.pop().ok_or(PathRejection::EscapesRoot)?;
            }
        }
    }
    Ok(NormalizedPath { drive, components })
}

fn is_separator(c: char) -> bool {
    SEPARATORS.contains(&c)
}

/// `X:` + separador (o fin, solo en verbatim). Devuelve la unidad y lo que sigue a `X:\`.
fn split_drive(raw: &str) -> Result<(char, &str), PathRejection> {
    let mut chars = raw.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else {
        return Err(PathRejection::NotAbsolute);
    };
    if !letter.is_ascii_alphabetic() {
        return Err(PathRejection::NotAbsolute);
    }
    let rest = &raw[2..];
    if !rest.is_empty() && !rest.starts_with(is_separator) {
        return Err(PathRejection::NotAbsolute);
    }
    if rest.is_empty() {
        // `C:` solo es la unidad relativa al directorio actual.
        return Err(PathRejection::NotAbsolute);
    }
    Ok((letter.to_ascii_uppercase(), rest))
}

/// Forma `\\?\...` (ya sin el prefijo): solo `X:\...` sin `/`, `.` ni `..`.
fn normalize_verbatim(rest: &str) -> Result<NormalizedPath, PathRejection> {
    let is_unc = rest
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case(r"UNC\"))
        || rest.eq_ignore_ascii_case("UNC");
    if is_unc {
        return Err(PathRejection::NetworkPath);
    }
    let mut chars = rest.chars();
    let (Some(letter), Some(':')) = (chars.next(), chars.next()) else {
        return Err(PathRejection::DevicePath);
    };
    if !letter.is_ascii_alphabetic() {
        return Err(PathRejection::DevicePath);
    }
    let tail = &rest[2..];
    let body = match tail {
        "" => "",
        _ => tail.strip_prefix('\\').ok_or(PathRejection::NotAbsolute)?,
    };
    let body = body.strip_suffix('\\').unwrap_or(body);
    let mut components = Vec::new();
    if !body.is_empty() {
        for part in body.split('\\') {
            match part {
                "" | "." | ".." => return Err(PathRejection::VerbatimNotNormalized),
                name => {
                    if name.contains('/') {
                        return Err(PathRejection::InvalidCharacter);
                    }
                    validate_component(name)?;
                    components.push(Component::new(name));
                }
            }
        }
    }
    Ok(NormalizedPath {
        drive: letter.to_ascii_uppercase(),
        components,
    })
}

/// Nombres reservados de dispositivo (con o sin extension).
fn is_reserved_device_name(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let upper = base.to_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let mut chars = upper.chars();
    let prefixed = upper.starts_with("COM") || upper.starts_with("LPT");
    if !prefixed || upper.chars().count() != 4 {
        return false;
    }
    matches!(
        chars.nth(3),
        Some('0'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}')
    )
}

fn validate_component(name: &str) -> Result<(), PathRejection> {
    if name.contains(':') {
        return Err(PathRejection::AlternateDataStream);
    }
    if name.chars().any(|c| {
        matches!(c, '<' | '>' | '"' | '|' | '?' | '*') || ('\u{1}'..='\u{1f}').contains(&c)
    }) {
        return Err(PathRejection::InvalidCharacter);
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(PathRejection::TrailingDotOrSpace);
    }
    if is_reserved_device_name(name) {
        return Err(PathRejection::ReservedDeviceName);
    }
    Ok(())
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
