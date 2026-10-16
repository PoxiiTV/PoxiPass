//! La bóveda abierta. Mientras existe, los secretos están sellados con el `Sealer`; al bloquear se suelta todo y
//! cada pieza se borra al liberarse (`Zeroizing` / `Drop` de `Sv`).

use crate::crypto::{Key, Sealer};
use crate::error::{AppError, Res};
use crate::util::now;
use crate::vault::file::{self, Fingerprint, Header, Opened};
use crate::vault::merge;
use crate::vault::model::{Entry, Vault};
use std::path::{Path, PathBuf};

pub const TRASH_DAYS: i64 = 30;
pub const MAX_HISTORY: usize = 20;

pub struct Session {
    pub path: PathBuf,
    pub header: Header,
    pub key: Key,
    pub vault: Vault,
    pub fp: Fingerprint,
    pub sealer: Sealer,
    /// Abierta con el kit de emergencia: hasta poner contraseña nueva no se enseña nada
    pub must_change_password: bool,
    /// El último guardado falló (la interfaz enseña «Reintentar»)
    pub save_failed: bool,
    /// Hay marcas de «usado» sin escribir (copiar no reescribe el archivo; se guardan con el siguiente cambio o al bloquear)
    pub used_dirty: bool,
    /// Tamaño y fecha del archivo la última vez que lo miramos (para ver barato si ha cambiado por fuera)
    disk_stamp: Option<(u64, Option<std::time::SystemTime>)>,
    /// Las cerraduras tal y como están en disco la última vez que sincronizamos (para saber si las hemos tocado aquí)
    header_sync: String,
}

impl Session {
    pub fn new(path: PathBuf, o: Opened) -> Self {
        let sealer = Sealer::new();
        let mut vault = o.vault;
        vault.seal(&sealer);
        let cutoff = now() - TRASH_DAYS * 86_400;
        vault.purge(|e| e.deleted.is_none_or(|d| d > cutoff));
        Self {
            path,
            header: o.header,
            key: o.key,
            vault,
            fp: o.fp,
            sealer,
            must_change_password: false,
            save_failed: false,
            used_dirty: false,
            disk_stamp: None,
            header_sync: String::new(),
        }
        .synced()
    }

    pub fn vault_id(&self) -> &str {
        &self.header.vault_id
    }

    pub fn entry(&self, id: &str) -> Res<&Entry> {
        self.vault.entries.iter().find(|e| e.id == id).ok_or_else(|| AppError::new("entry.notFound"))
    }

    pub fn mark_used(&mut self, id: &str) {
        if let Ok(e) = self.entry_mut(id) {
            e.used = Some(now());
            self.used_dirty = true;
        }
    }

    pub fn entry_mut(&mut self, id: &str) -> Res<&mut Entry> {
        self.vault.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| AppError::new("entry.notFound"))
    }

    /// Guarda en disco. Si el archivo cambió por fuera devuelve `vault.changedOnDisk` y no toca nada
    /// (salvo con `force`, que sobrescribe).
    fn synced(mut self) -> Self {
        self.header_sync = serde_json::to_string(&self.header).unwrap_or_default();
        self.disk_stamp = stamp(&self.path);
        self
    }

    /// Guarda. Si el archivo cambió por fuera (Drive, OneDrive, otro PC), primero fusiona esa versión con la de
    /// aquí, entrada a entrada, y guarda el resultado. Solo si no se puede abrir con nuestra clave (es otro
    /// llavero) devuelve `vault.changedOnDisk` y la interfaz pregunta qué hacer.
    pub fn save(&mut self, backups: &Path, force: bool) -> Res<()> {
        match self.write_once(backups, force) {
            Err(e) if e.code == "vault.changedOnDisk" => {
                self.merge_from_disk()?;
                self.write_once(backups, false)
            }
            r => r,
        }
    }

    /// Fusiona con lo que hay en disco. Las cerraduras del disco (p. ej. contraseña cambiada en otro PC) se
    /// adoptan, salvo que aquí se hayan tocado desde la última sincronización.
    pub fn merge_from_disk(&mut self) -> Res<merge::Report> {
        let bytes = std::fs::read(&self.path)?;
        let (header, theirs) = file::decode_with_key(&bytes, &self.key).map_err(|_| AppError::new("vault.changedOnDisk"))?;
        if header.vault_id != self.header.vault_id {
            return Err(AppError::new("vault.changedOnDisk"));
        }
        let report = merge::merge(&mut self.vault, theirs, &self.sealer);
        let ours_touched = serde_json::to_string(&self.header).unwrap_or_default() != self.header_sync;
        if !ours_touched {
            self.header = header;
            self.header_sync = serde_json::to_string(&self.header).unwrap_or_default();
        }
        self.fp = file::fingerprint(&bytes);
        self.disk_stamp = stamp(&self.path);
        Ok(report)
    }

    /// Para la vigilancia periódica: si el archivo ha cambiado por fuera, se fusiona (y se escribe si aquí había
    /// algo que el disco no tiene). Devuelve cuántas cosas han llegado de fuera.
    pub fn sync_from_disk(&mut self, backups: &Path) -> Res<usize> {
        let now_stamp = stamp(&self.path);
        if now_stamp.is_none() || now_stamp == self.disk_stamp {
            return Ok(0);
        }
        let bytes = std::fs::read(&self.path)?;
        if file::fingerprint(&bytes) == self.fp {
            self.disk_stamp = now_stamp;
            return Ok(0);
        }
        let r = self.merge_from_disk()?;
        if r.ours_newer {
            self.write_once(backups, false)?;
        }
        Ok(r.incoming)
    }

    fn write_once(&mut self, backups: &Path, force: bool) -> Res<()> {
        self.vault.modified = now();
        let bytes = {
            let plain = self.vault.to_plain(&self.sealer);
            file::encode(&self.header, &self.key, &plain)?
        };
        match file::write(&self.path, &bytes, if force { None } else { Some(self.fp) }, backups) {
            Ok(fp) => {
                self.fp = fp;
                self.save_failed = false;
                self.used_dirty = false;
                self.disk_stamp = stamp(&self.path);
                self.header_sync = serde_json::to_string(&self.header).unwrap_or_default();
                Ok(())
            }
            Err(e) => {
                self.save_failed = true;
                Err(e)
            }
        }
    }

    /// Guarda una copia aparte (para el conflicto «Guardar como copia»). La sesión pasa a usar la copia.
    pub fn save_as(&mut self, path: PathBuf, backups: &Path) -> Res<()> {
        if path.exists() {
            return Err(AppError::new("vault.exists"));
        }
        self.path = path;
        self.save(backups, true)
    }
}

fn stamp(path: &Path) -> Option<(u64, Option<std::time::SystemTime>)> {
    std::fs::metadata(path).ok().map(|m| (m.len(), m.modified().ok()))
}

pub struct LockRules {
    /// Minutos sin tocar el PC (0 = nunca)
    pub idle_min: u32,
    pub on_session_lock: bool,
    pub on_sleep: bool,
}

/// ¿Hay que bloquear? `gap_secs`: segundos reales desde la última comprobación (cada 2 s); si son muchos más, el
/// PC se ha suspendido.
pub fn should_lock(r: &LockRules, idle_ms: u32, session_locked: bool, gap_secs: u64) -> Option<&'static str> {
    if r.on_sleep && gap_secs > 30 {
        Some("sleep")
    } else if r.on_session_lock && session_locked {
        Some("windows")
    } else if r.idle_min > 0 && idle_ms as u64 >= r.idle_min as u64 * 60_000 {
        Some("idle")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::file::tests::{tmpdir, FAST};
    use crate::vault::model::*;

    pub fn login(title: &str, pw: &str, deleted: Option<i64>) -> Entry {
        Entry {
            id: new_id(),
            kind: Kind::Login,
            title: title.into(),
            folder: None,
            tags: vec![],
            favorite: false,
            fields: vec![Field {
                id: new_id(),
                key: "password".into(),
                label: String::new(),
                value: Sv::text(pw),
                secret: true,
                kind: FieldKind::Password,
            }],
            attachments: vec![],
            icon: None,
            autotype: None,
            history: vec![],
            created: 1,
            modified: 1,
            used: None,
            deleted,
            breach: None,
            archived: false,
            protected: false,
        }
    }

    #[test]
    fn two_pcs_on_the_same_file_merge_instead_of_conflict() {
        let d = tmpdir("twopcs");
        let path = d.join("b.poxipass");
        let bk = d.join("bk");
        let (o, _) = file::create(&path, Vault { name: "x".into(), ..Default::default() }, "pw", FAST, &bk).unwrap();
        drop(o);
        // Dos PCs abren el mismo archivo (Drive)
        let mut pc1 = Session::new(path.clone(), file::open_with_password(&path, "pw").unwrap());
        let mut pc2 = Session::new(path.clone(), file::open_with_password(&path, "pw").unwrap());
        pc1.vault.entries.push(login("del-pc1", "uno", None));
        pc1.save(&bk, false).unwrap();
        pc2.vault.entries.push(login("del-pc2", "dos", None));
        pc2.save(&bk, false).expect("se fusiona en vez de dar conflicto");
        // El primero se entera solo (vigilancia) y ve lo del segundo
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(pc1.sync_from_disk(&bk).unwrap(), 1);
        let again = Session::new(path.clone(), file::open_with_password(&path, "pw").unwrap());
        let mut titles: Vec<&str> = again.vault.entries.iter().map(|e| e.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, ["del-pc1", "del-pc2"]);
        // Un archivo de otro llavero en su sitio no se fusiona: conflicto
        let other = d.join("otro.poxipass");
        file::create(&other, Vault::default(), "pw", FAST, &bk).unwrap();
        std::fs::copy(&other, &path).unwrap();
        pc1.vault.entries.push(login("mas", "x", None));
        assert_eq!(pc1.save(&bk, false).err().unwrap().code, "vault.changedOnDisk");
    }

    #[test]
    fn health_finds_duplicates_and_skips_archived() {
        let d = tmpdir("dupes");
        let path = d.join("b.poxipass");
        let with_user = |title: &str, user: &str, url: &str, pw: &str| {
            let mut e = login(title, pw, None);
            e.fields.push(Field { id: new_id(), key: "username".into(), label: String::new(), value: Sv::text(user), secret: false, kind: FieldKind::Text });
            e.fields.push(Field { id: new_id(), key: "url".into(), label: String::new(), value: Sv::text(url), secret: false, kind: FieldKind::Url });
            e
        };
        let mut archived = with_user("Netflix viejo", "yo", "https://netflix.com", "a");
        archived.archived = true;
        let vault = Vault {
            entries: vec![
                with_user("Netflix", "yo", "https://www.netflix.com", "1"),
                with_user("netflix (importada)", "YO", "netflix.com/login", "2"),
                with_user("Netflix", "otra", "https://netflix.com", "3"),
                archived,
            ],
            ..Default::default()
        };
        let (o, _) = file::create(&path, vault, "pw", FAST, &d.join("bk")).unwrap();
        let s = Session::new(path, o);
        let r = crate::audit::report(&s);
        assert_eq!(r.duplicates.len(), 1, "misma web y usuario (sin mirar mayúsculas)");
        assert_eq!(r.duplicates[0].len(), 2);
        assert_eq!(r.total, 3, "lo archivado no cuenta en la salud");
    }

    #[test]
    fn lock_rules() {
        let r = LockRules { idle_min: 5, on_session_lock: true, on_sleep: true };
        assert_eq!(should_lock(&r, 1000, false, 2), None);
        assert_eq!(should_lock(&r, 300_000, false, 2), Some("idle"));
        assert_eq!(should_lock(&r, 0, true, 2), Some("windows"));
        assert_eq!(should_lock(&r, 0, false, 600), Some("sleep"));
        let off = LockRules { idle_min: 0, on_session_lock: false, on_sleep: false };
        assert_eq!(should_lock(&off, u32::MAX, true, 600), None);
    }

    #[test]
    fn opening_seals_secrets_and_purges_old_trash() {
        let d = tmpdir("session");
        let path = d.join("b.poxipass");
        let vault = Vault {
            name: "x".into(),
            entries: vec![login("a", "uno", None), login("b", "dos", Some(1)), login("c", "tres", Some(now()))],
            ..Default::default()
        };
        let (o, _) = file::create(&path, vault, "pw", FAST, &d.join("bk")).unwrap();
        let mut s = Session::new(path.clone(), o);
        assert_eq!(s.vault.entries.len(), 2, "la de hace 30+ días se purga");
        let f = &s.vault.entries[0].fields[0];
        assert!(f.value.is_sealed());
        assert_eq!(&*f.value.reveal_string(&s.sealer), "uno");
        s.save(&d.join("bk"), false).unwrap();
        let again = Session::new(path.clone(), file::open_with_password(&path, "pw").unwrap());
        assert_eq!(&*again.vault.entries[0].fields[0].value.reveal_string(&again.sealer), "uno");
    }
}
