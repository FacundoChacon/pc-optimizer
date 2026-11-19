//! Raiz del volumen cuya Papelera se vacia (design D-03).

use std::fmt;
use std::path::PathBuf;

/// Raiz de un volumen local (`C:\`). Es el UNICO objetivo que acepta el puerto de vaciado: no se
/// puede construir "todas las Papeleras" (raiz nula) ni una carpeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeRoot(char);

impl VolumeRoot {
    /// Solo letras `A`-`Z` (ASCII); normaliza a mayuscula.
    pub fn from_drive_letter(letter: char) -> Option<Self> {
        letter
            .is_ascii_alphabetic()
            .then(|| Self(letter.to_ascii_uppercase()))
    }

    /// Letra de la unidad, en mayuscula.
    pub fn letter(&self) -> char {
        self.0
    }

    /// La raiz como ruta (`C:\`).
    pub fn as_path(&self) -> PathBuf {
        PathBuf::from(format!("{}:\\", self.0))
    }
}

impl fmt::Display for VolumeRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:\\", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_drive_letter_should_normalize_letters_to_an_uppercase_root() {
        for letter in ['c', 'C', 'd', 'Z'] {
            let root = VolumeRoot::from_drive_letter(letter).unwrap();
            let upper = letter.to_ascii_uppercase();
            assert_eq!(
                root.as_path(),
                std::path::PathBuf::from(format!("{upper}:\\"))
            );
        }
    }

    #[test]
    fn from_drive_letter_should_reject_anything_that_is_not_a_latin_letter() {
        for letter in ['1', 'ñ', ' ', '\\', ':', '\0', 'é'] {
            assert_eq!(VolumeRoot::from_drive_letter(letter), None, "{letter:?}");
        }
    }

    #[test]
    fn display_should_show_the_drive_with_a_single_backslash() {
        let root = VolumeRoot::from_drive_letter('c').unwrap();
        assert_eq!(root.to_string(), "C:\\");
        assert_eq!(root.letter(), 'C');
    }
}
