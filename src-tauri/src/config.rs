//! Ajustes de la app (no de la bóveda): tema, bloqueo, recientes, Windows Hello… En
//! `%APPDATA%\com.poxi.poxipass\config.json`. No guarda secretos: las cerraduras de Hello solo se abren con el TPM.

use crate::generator::GenOptions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_RECENT: usize = 8;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HelloSlot {
    pub challenge: String,
    pub wrapped: String,
    /// Última vez que se abrió con la contraseña maestra (cada 14 días se vuelve a pedir)
    pub last_password: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// `None`: el idioma de Windows
    pub lang: Option<String>,
    /// `dark`, `light` o `system`
    pub theme: String,
    pub accent: String,
    pub recent: Vec<String>,
    /// Ruta de cada llavero → su id (para encontrar sus copias aunque el archivo haya desaparecido)
    pub vault_ids: HashMap<String, String>,
    /// Carpeta extra para copias (un USB, Drive, otro disco…); `None` = solo las de AppData
    pub extra_backup_dir: Option<String>,
    /// Minutos sin tocar el PC para bloquear (0 = nunca)
    pub lock_idle_min: u32,
    pub lock_on_session_lock: bool,
    pub lock_on_sleep: bool,
    pub lock_on_minimize: bool,
    /// Segundos hasta borrar el portapapeles (0 = nunca)
    pub clipboard_secs: u32,
    pub close_to_tray: bool,
    pub autotype_enabled: bool,
    pub autotype_hotkey: String,
    pub autotype_sequence: String,
    /// Acceso rápido: barra flotante para buscar y escribir/copiar
    pub quick_enabled: bool,
    pub quick_hotkey: String,
    pub favicons: bool,
    /// Conexión con la extensión del navegador (alta del relé en el registro)
    pub bridge_enabled: bool,
    pub generator: GenOptions,
    /// Última versión con la que se abrió (para enseñar las novedades tras actualizar)
    pub seen_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lang: None,
            theme: "dark".into(),
            accent: "blue".into(),
            recent: vec![],
            vault_ids: HashMap::new(),
            extra_backup_dir: None,
            lock_idle_min: 5,
            lock_on_session_lock: true,
            lock_on_sleep: true,
            lock_on_minimize: false,
            clipboard_secs: 20,
            close_to_tray: true,
            autotype_enabled: true,
            autotype_hotkey: "Ctrl+Alt+A".into(),
            autotype_sequence: "{USERNAME}{TAB}{PASSWORD}{ENTER}".into(),
            quick_enabled: true,
            quick_hotkey: "Ctrl+Shift+Space".into(),
            favicons: false,
            bridge_enabled: true,
            generator: GenOptions::default(),
            seen_version: String::new(),
        }
    }
}

impl Config {
    pub fn load(dir: &Path) -> Self {
        fs::read(dir.join("config.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = fs::create_dir_all(dir);
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let tmp = dir.join("config.json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(tmp, dir.join("config.json"));
            }
        }
    }

    pub fn touch_recent(&mut self, path: &Path, vault_id: &str) {
        let p = path.to_string_lossy().to_string();
        self.vault_ids.insert(p.to_lowercase(), vault_id.to_string());
        self.recent.retain(|r| !r.eq_ignore_ascii_case(&p));
        self.recent.insert(0, p);
        self.recent.truncate(MAX_RECENT);
    }

    /// Lo que viene de la interfaz se recorta a valores válidos (nunca se confía en ella).
    pub fn sanitized(mut self, current: &Config) -> Config {
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = "dark".into();
        }
        if !matches!(self.accent.as_str(), "violet" | "blue" | "emerald" | "pink" | "amber" | "red") {
            self.accent = "blue".into();
        }
        if !matches!(self.lang.as_deref(), None | Some("es") | Some("en")) {
            self.lang = None;
        }
        self.lock_idle_min = self.lock_idle_min.min(24 * 60);
        self.clipboard_secs = self.clipboard_secs.min(600);
        self.autotype_hotkey.truncate(40);
        self.autotype_sequence.truncate(500);
        self.quick_hotkey.truncate(40);
        self.generator = self.generator.clamped();
        // Esto no lo cambia la interfaz por esta vía
        self.recent = current.recent.clone();
        self.vault_ids = current.vault_ids.clone();
        if self.extra_backup_dir.as_ref().is_some_and(|d| d.len() > 1000 || !Path::new(d).is_absolute()) {
            self.extra_backup_dir = current.extra_backup_dir.clone();
        }
        self.seen_version = current.seen_version.clone();
        self
    }
}

/// Cerraduras de Windows Hello por bóveda (`hello.json`). Nunca se mandan a la interfaz.
pub fn load_hello(dir: &Path) -> HashMap<String, HelloSlot> {
    fs::read(dir.join("hello.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save_hello(dir: &Path, map: &HashMap<String, HelloSlot>) {
    let _ = fs::create_dir_all(dir);
    if let Ok(json) = serde_json::to_vec_pretty(map) {
        let _ = fs::write(dir.join("hello.json"), json);
    }
}

impl Config {
    pub fn vault_id_for(&self, path: &Path) -> Option<String> {
        self.vault_ids.get(&path.to_string_lossy().to_lowercase()).cloned()
    }
}

/// Copias extra: `<carpeta elegida>\PoxiPass\<id del llavero>`
pub fn extra_dir(cfg: &Config, vault_id: &str) -> Option<PathBuf> {
    cfg.extra_backup_dir.as_ref().map(|d| Path::new(d).join("PoxiPass").join(vault_id))
}

/// ¿Interfaz en español? (para los pocos textos que muestra Windows, como el de Hello)
pub fn is_spanish(cfg: &Config) -> bool {
    match cfg.lang.as_deref() {
        Some(l) => l == "es",
        None => std::env::var("LANG").map(|l| l.starts_with("es")).unwrap_or(true),
    }
}

pub fn backups_dir(data: &Path, vault_id: &str) -> PathBuf {
    data.join("backups").join(vault_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_vault_ids_and_extra_dir() {
        let mut c = Config::default();
        c.touch_recent(Path::new(r"C:\Docs\Mi Llavero.poxipass"), "abc");
        assert_eq!(c.vault_id_for(Path::new(r"c:\docs\mi llavero.poxipass")).as_deref(), Some("abc"));
        assert!(extra_dir(&c, "abc").is_none());
        c.extra_backup_dir = Some(r"E:\USB".into());
        assert_eq!(extra_dir(&c, "abc").unwrap(), PathBuf::from(r"E:\USB\PoxiPass\abc"));
        // La interfaz no puede meter rutas relativas ni tocar los ids
        let mut from_ui = c.clone();
        from_ui.extra_backup_dir = Some("relativa".into());
        from_ui.vault_ids.clear();
        let s = from_ui.sanitized(&c);
        assert_eq!(s.extra_backup_dir.as_deref(), Some(r"E:\USB"));
        assert_eq!(s.vault_ids.len(), 1);
    }
}
