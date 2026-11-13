//! Lector minimo (seguro, sin dependencias) del manifest embebido en un ejecutable PE.
//!
//! Recorre el directorio de recursos (tipo `RT_MANIFEST` = 24) y analiza el XML del manifest.
//! Todo acceso a los bytes esta acotado: una entrada corrupta o truncada devuelve `Err`.

/// Datos relevantes del manifest de un ejecutable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestInfo {
    /// Valor de `requestedExecutionLevel level="..."`, si el manifest lo declara.
    pub execution_level: Option<String>,
    /// El manifest depende de Microsoft.Windows.Common-Controls 6.x.
    pub common_controls_v6: bool,
}

const RT_MANIFEST: u32 = 24;
const MAX_RESOURCE_DEPTH: usize = 3;

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    let slice = bytes
        .get(at..at.checked_add(2).ok_or("desplazamiento fuera de rango")?)
        .ok_or_else(|| format!("lectura fuera de rango en {at:#x}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes
        .get(at..at.checked_add(4).ok_or("desplazamiento fuera de rango")?)
        .ok_or_else(|| format!("lectura fuera de rango en {at:#x}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

fn rva_to_offset(sections: &[Section], rva: u32) -> Result<usize, String> {
    for section in sections {
        let span = section.virtual_size.max(section.raw_size);
        if rva >= section.virtual_address && rva - section.virtual_address < span {
            let offset = u64::from(section.raw_offset) + u64::from(rva - section.virtual_address);
            return usize::try_from(offset).map_err(|_| "offset fuera de rango".to_string());
        }
    }
    Err(format!("RVA {rva:#x} fuera de toda seccion"))
}

/// Busca el primer recurso de manifest y devuelve su contenido.
fn find_manifest_bytes(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err("no es un ejecutable PE (falta MZ)".into());
    }
    let pe = u32_at(bytes, 0x3C)? as usize;
    if bytes.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0") {
        return Err("firma PE invalida".into());
    }
    let coff = pe + 4;
    let section_count = usize::from(u16_at(bytes, coff + 2)?);
    let optional_size = usize::from(u16_at(bytes, coff + 16)?);
    let optional = coff + 20;
    let directories_at = match u16_at(bytes, optional)? {
        0x10B => optional + 96,
        0x20B => optional + 112,
        other => {
            return Err(format!(
                "magic del encabezado opcional desconocido: {other:#x}"
            ));
        }
    };
    // Directorio de recursos = entrada 2 del arreglo de directorios de datos.
    let resource_rva = u32_at(bytes, directories_at + 2 * 8)?;
    if resource_rva == 0 {
        return Err("el ejecutable no tiene directorio de recursos".into());
    }
    let table = optional + optional_size;
    let mut sections = Vec::new();
    for index in 0..section_count {
        let at = table + index * 40;
        sections.push(Section {
            virtual_size: u32_at(bytes, at + 8)?,
            virtual_address: u32_at(bytes, at + 12)?,
            raw_size: u32_at(bytes, at + 16)?,
            raw_offset: u32_at(bytes, at + 20)?,
        });
    }
    let root = rva_to_offset(&sections, resource_rva)?;

    // Baja por el primer hijo de cada nivel: tipo (RT_MANIFEST) -> nombre -> idioma.
    let mut directory = root;
    for depth in 0..MAX_RESOURCE_DEPTH {
        let named = usize::from(u16_at(bytes, directory + 12)?);
        let ids = usize::from(u16_at(bytes, directory + 14)?);
        let mut chosen = None;
        for index in 0..named + ids {
            let entry = directory + 16 + index * 8;
            let id = u32_at(bytes, entry)?;
            let target = u32_at(bytes, entry + 4)?;
            let is_named = index < named;
            if depth == 0 && (is_named || id != RT_MANIFEST) {
                continue;
            }
            chosen = Some(target);
            break;
        }
        let target = chosen.ok_or("el ejecutable no tiene recurso de manifest")?;
        let is_subdirectory = target & 0x8000_0000 != 0;
        let offset = root + (target & 0x7FFF_FFFF) as usize;
        if depth == MAX_RESOURCE_DEPTH - 1 {
            if is_subdirectory {
                return Err("estructura de recursos inesperada".into());
            }
            let data_rva = u32_at(bytes, offset)?;
            let size = u32_at(bytes, offset + 4)? as usize;
            let start = rva_to_offset(&sections, data_rva)?;
            let end = start.checked_add(size).ok_or("tamano fuera de rango")?;
            return bytes
                .get(start..end)
                .ok_or_else(|| "manifest truncado".to_string());
        }
        if !is_subdirectory {
            return Err("estructura de recursos inesperada".into());
        }
        directory = offset;
    }
    Err("estructura de recursos inesperada".into())
}

/// Valor del atributo `name` (comillas simples o dobles) dentro de `tag`.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(found) = rest.find(name) {
        let after = &rest[found + name.len()..];
        let before_ok = rest[..found]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace());
        let trimmed = after.trim_start();
        if before_ok && let Some(value) = trimmed.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next().filter(|c| *c == '\'' || *c == '"')?;
            let inner = &value[1..];
            return inner.find(quote).map(|end| &inner[..end]);
        }
        rest = after;
    }
    None
}

/// Etiquetas `<name ...>` del XML (sin interpretar anidamiento).
fn tags<'a>(xml: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> {
    let open = format!("<{name}");
    let mut rest = xml;
    std::iter::from_fn(move || {
        let start = rest.find(&open)?;
        let after = &rest[start..];
        let end = after.find('>')?;
        rest = &after[end..];
        Some(&after[..end])
    })
}

/// Lee el manifest embebido en los bytes de un ejecutable PE.
pub fn read_manifest(bytes: &[u8]) -> Result<ManifestInfo, String> {
    let manifest = find_manifest_bytes(bytes)?;
    let xml = std::str::from_utf8(manifest).map_err(|e| format!("manifest no UTF-8: {e}"))?;
    let execution_level = tags(xml, "requestedExecutionLevel")
        .find_map(|tag| attribute(tag, "level"))
        .map(str::to_string);
    let common_controls_v6 = tags(xml, "assemblyIdentity").any(|tag| {
        attribute(tag, "name") == Some("Microsoft.Windows.Common-Controls")
            && attribute(tag, "version").is_some_and(|v| v.starts_with("6."))
    });
    Ok(ManifestInfo {
        execution_level,
        common_controls_v6,
    })
}
