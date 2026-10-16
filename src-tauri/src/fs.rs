//! Lo justo para el explorador de archivos propio de la app (sin los cuadros de diálogo de Windows).

use crate::error::{AppError, Res};
use serde::Serialize;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

const HIDDEN: u32 = 0x2;
const SYSTEM: u32 = 0x4;
const MAX_ITEMS: usize = 5000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    /// `documents`, `desktop`, `downloads`, `home` o `drive`
    pub kind: String,
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
    pub modified: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub items: Vec<Item>,
}

pub fn drives() -> Vec<Place> {
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| {
            let l = (b'A' + i) as char;
            Place { kind: "drive".into(), name: format!("{l}:"), path: format!("{l}:\\") }
        })
        .collect()
}

/// Contenido de una carpeta: carpetas primero, sin ocultos ni de sistema, y solo los archivos con esas extensiones.
pub fn list(dir: &Path, exts: &[String]) -> Res<Listing> {
    if !dir.is_absolute() {
        return Err(AppError::new("io.badPath"));
    }
    let mut items: Vec<Item> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if m.file_attributes() & (HIDDEN | SYSTEM) != 0 {
                return None;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let dir = m.is_dir();
            if !dir && !exts.is_empty() {
                let ext = Path::new(&name).extension()?.to_string_lossy().to_lowercase();
                if !exts.iter().any(|x| x.eq_ignore_ascii_case(&ext)) {
                    return None;
                }
            }
            let modified = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
            Some(Item { path: e.path().to_string_lossy().to_string(), name, dir, size: if dir { 0 } else { m.len() }, modified })
        })
        .take(MAX_ITEMS)
        .collect();
    items.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Listing {
        path: dir.to_string_lossy().to_string(),
        parent: dir.parent().map(|p| p.to_string_lossy().to_string()),
        items,
    })
}

/// Ruta final para guardar: carpeta + nombre, con la extensión puesta y sin caracteres prohibidos.
pub fn save_path(dir: &str, name: &str, ext: &str) -> Res<PathBuf> {
    let clean: String = name.trim().chars().filter(|c| !r#"<>:"/\|?*"#.contains(*c) && !c.is_control()).take(150).collect();
    let clean = clean.trim_end_matches(['.', ' ']).to_string();
    if clean.is_empty() {
        return Err(AppError::new("input.empty"));
    }
    let mut p = PathBuf::from(dir).join(clean);
    if !ext.is_empty() && p.extension().is_none_or(|e| !e.eq_ignore_ascii_case(ext)) {
        let with = format!("{}.{ext}", p.file_name().unwrap_or_default().to_string_lossy());
        p.set_file_name(with);
    }
    if !p.is_absolute() {
        return Err(AppError::new("io.badPath"));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_path_cleans_names() {
        let p = save_path("C:\\x", "Mi: llavero?.", "poxipass").unwrap();
        assert_eq!(p, PathBuf::from("C:\\x\\Mi llavero.poxipass"));
        assert_eq!(save_path("C:\\x", "kit.PDF", "pdf").unwrap(), PathBuf::from("C:\\x\\kit.PDF"));
        assert!(save_path("C:\\x", " ?* ", "pdf").is_err());
        assert!(save_path("relativa", "a", "pdf").is_err());
    }

    #[test]
    fn lists_folders_first_and_filters() {
        let d = std::env::temp_dir().join(format!("poxipass-fs-{}", crate::vault::model::new_id()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("b.poxipass"), b"x").unwrap();
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        let l = list(&d, &["poxipass".into()]).unwrap();
        let names: Vec<&str> = l.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["sub", "b.poxipass"]);
        assert_eq!(list(&d, &[]).unwrap().items.len(), 3);
    }
}
