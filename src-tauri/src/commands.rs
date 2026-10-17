//! La única puerta entre la interfaz y Rust. Todo lo que entra se valida aquí; lo que sale nunca lleva secretos
//! salvo `reveal_field` (lo que la persona ha pedido ver).

use crate::audit::{self, Report};
use crate::autotype::{self, Action, Token};
use crate::config::{self, backups_dir, Config};
use crate::crypto::KdfParams;
use crate::error::{AppError, Res};
use crate::generator::{self, GenOptions, Generated, Strength};
use crate::import::{self, ExportRow, Preview};
use crate::session::{Session, MAX_HISTORY};
use crate::totp::{self, Code};
use crate::util::now;
use crate::vault::file;
use crate::vault::model::*;
use crate::{clipboard, hello, icons};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use zeroize::Zeroizing;

const MAX_ENTRIES: usize = 20_000;
const MAX_FIELDS: usize = 60;
const MAX_VALUE: usize = 64 * 1024;
pub(crate) const MAX_ATTACHMENT: u64 = 10 * 1024 * 1024;
const MAX_ATTACHMENTS_TOTAL: u64 = 50 * 1024 * 1024;

pub struct AppState {
    pub session: Mutex<Option<Session>>,
    pub config: Mutex<Config>,
    pub data_dir: PathBuf,
    pub http: reqwest::Client,
    /// Ventana en la que escribir cuando el auto-type necesita que elijas la entrada
    pub autotype_target: Mutex<Option<isize>>,
    /// Último archivo importado (el único que la interfaz puede pedir borrar)
    pub last_import: Mutex<Option<PathBuf>>,
    pub start_hidden: bool,
    /// Últimas contraseñas generadas y copiadas (solo en memoria; se vacía al bloquear)
    pub gen_history: Mutex<Vec<(i64, Zeroizing<String>)>>,
    /// Atajos globales que Windows no dejó registrar (los tiene otra app)
    pub hotkey_failed: Mutex<Vec<String>>,
    /// Hay un kit de emergencia en pantalla: no se bloquea por inactividad mientras se apunta
    pub busy: std::sync::atomic::AtomicBool,
    /// Entradas sensibles ya confirmadas con Hello (id → cuándo): no se vuelve a pedir durante un minuto
    pub verified: Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

impl AppState {
    pub fn cfg(&self) -> MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn sess(&self) -> MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn backups(&self, s: &Session) -> PathBuf {
        backups_dir(&self.data_dir, s.vault_id())
    }

    /// Guarda en disco y deja también la copia en la carpeta extra, si hay una (si no está, p. ej. un USB
    /// desenchufado, el guardado normal no falla por eso).
    pub(crate) fn backups_of(&self, s: &Session) -> PathBuf {
        self.backups(s)
    }

    pub(crate) fn persist(&self, s: &mut Session, force: bool) -> Res<()> {
        s.save(&self.backups(s), force)?;
        self.mirror(&s.path, s.vault_id(), false);
        Ok(())
    }

    fn mirror(&self, path: &Path, vault_id: &str, force: bool) {
        let extra = config::extra_dir(&self.cfg(), vault_id);
        if let (Some(dir), Ok(bytes)) = (extra, std::fs::read(path)) {
            let _ = file::backup(&bytes, &dir, force);
        }
    }
}

/// Con la bóveda abierta (y con contraseña ya cambiada si se abrió con el kit).
pub(crate) fn with<T>(st: &AppState, f: impl FnOnce(&mut Session) -> Res<T>) -> Res<T> {
    let mut g = st.sess();
    let s = g.as_mut().ok_or_else(|| AppError::new("vault.locked"))?;
    if s.must_change_password {
        return Err(AppError::new("vault.mustChangePassword"));
    }
    f(s)
}

/// Cambia algo y lo guarda en el acto. Si no se puede guardar, el cambio queda en memoria y se avisa.
pub(crate) fn mutate<T>(st: &AppState, f: impl FnOnce(&mut Session) -> Res<T>) -> Res<T> {
    with(st, |s| {
        let out = f(s)?;
        st.persist(s, false)?;
        Ok(out)
    })
}

pub(crate) fn check_len(s: &str, max: usize) -> Res<()> {
    if s.chars().count() > max {
        return Err(AppError::new("input.tooLong"));
    }
    Ok(())
}

// ————————————————————————————————————————————————————————————— vistas para la interfaz

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub name: String,
    pub path: String,
    pub vault_id: String,
    pub folders: Vec<Folder>,
    pub must_change_password: bool,
    pub save_failed: bool,
    pub hello_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub subtitle: String,
    pub url: String,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub icon: Option<String>,
    pub has_totp: bool,
    pub modified: i64,
    pub used: Option<i64>,
    pub deleted: Option<i64>,
    pub created: i64,
    pub archived: bool,
    pub protected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldView {
    pub id: String,
    pub key: String,
    pub label: String,
    /// `None` si es secreto (se pide con `reveal_field`)
    pub value: Option<String>,
    pub secret: bool,
    pub empty: bool,
    pub kind: FieldKind,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentView {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub index: usize,
    pub at: i64,
    pub title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    #[serde(flatten)]
    pub summary: EntrySummary,
    pub fields: Vec<FieldView>,
    pub attachments: Vec<AttachmentView>,
    pub history: Vec<HistoryView>,
    pub autotype: Option<String>,
    pub strength: Option<Strength>,
    /// Veces que sale en filtraciones (solo si se comprobó con la contraseña actual)
    pub breach: Option<u32>,
}

fn icon_url(e: &Entry) -> Option<String> {
    e.icon.as_ref().map(|b| format!("data:image/png;base64,{}", B64.encode(b)))
}

fn plain_of(e: &Entry, key: &str) -> String {
    e.field(key).map(|f| f.value.plain_str().to_string()).unwrap_or_default()
}

fn summary(s: &Session, e: &Entry) -> EntrySummary {
    let subtitle = match e.kind {
        Kind::Login => plain_of(e, "username"),
        Kind::Identity => plain_of(e, "fullname"),
        Kind::Card => e
            .field("number")
            .map(|f| {
                let n: String = f.value.reveal_string(&s.sealer).chars().filter(char::is_ascii_digit).collect();
                if n.len() >= 4 { format!("•••• {}", &n[n.len() - 4..]) } else { String::new() }
            })
            .unwrap_or_default(),
        Kind::Note => String::new(),
        Kind::Wifi => plain_of(e, "ssid"),
        Kind::Ssh => {
            let (u, h) = (plain_of(e, "username"), plain_of(e, "host"));
            if u.is_empty() { h } else { format!("{u}@{h}") }
        }
        Kind::License => plain_of(e, "product"),
        Kind::Bank => plain_of(e, "bank"),
        Kind::Document => plain_of(e, "doctype"),
    };
    EntrySummary {
        id: e.id.clone(),
        kind: e.kind,
        title: e.title.clone(),
        subtitle,
        url: plain_of(e, "url"),
        folder: e.folder.clone(),
        tags: e.tags.clone(),
        favorite: e.favorite,
        icon: icon_url(e),
        has_totp: e.field("totp").is_some_and(|f| !f.value.is_empty()),
        modified: e.modified,
        used: e.used,
        deleted: e.deleted,
        created: e.created,
        archived: e.archived,
        protected: e.protected,
    }
}

fn view(s: &Session, e: &Entry) -> EntryView {
    let pw = e.field("password").filter(|f| !f.value.is_empty()).map(|f| f.value.reveal(&s.sealer));
    EntryView {
        summary: summary(s, e),
        fields: e
            .fields
            .iter()
            .map(|f| FieldView {
                id: f.id.clone(),
                key: f.key.clone(),
                label: f.label.clone(),
                value: (!f.secret).then(|| f.value.plain_str().to_string()),
                secret: f.secret,
                empty: f.value.is_empty(),
                kind: f.kind,
            })
            .collect(),
        attachments: e.attachments.iter().map(|a| AttachmentView { id: a.id.clone(), name: a.name.clone(), mime: a.mime.clone(), size: a.size }).collect(),
        history: e.history.iter().enumerate().map(|(index, h)| HistoryView { index, at: h.at, title: h.title.clone() }).collect(),
        autotype: e.autotype.clone(),
        strength: pw.as_ref().map(|p| generator::strength(&String::from_utf8_lossy(p))),
        breach: match (&e.breach, &pw) {
            (Some(b), Some(p)) if b.fp.as_slice() == audit::breach_fp(&s.key, p) => Some(b.count),
            _ => None,
        },
    }
}

fn info(st: &AppState, s: &Session) -> VaultInfo {
    VaultInfo {
        name: s.vault.name.clone(),
        path: s.path.to_string_lossy().to_string(),
        vault_id: s.vault_id().to_string(),
        folders: s.vault.folders.clone(),
        must_change_password: s.must_change_password,
        save_failed: s.save_failed,
        hello_enabled: config::load_hello(&st.data_dir).contains_key(s.vault_id()),
    }
}

// ————————————————————————————————————————————————————————————— app y ajustes

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub portable: bool,
    pub start_hidden: bool,
    pub default_dir: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle, st: State<'_, AppState>) -> AppInfo {
    // El instalador NSIS deja `uninstall.exe` junto al exe; el portable no
    let portable = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| !d.join("uninstall.exe").exists())).unwrap_or(true);
    let docs = app.path().document_dir().unwrap_or_else(|_| st.data_dir.clone());
    AppInfo {
        version: app.package_info().version.to_string(),
        portable: portable && !cfg!(debug_assertions),
        start_hidden: st.start_hidden,
        default_dir: docs.join("PoxiPass").to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn config_get(st: State<'_, AppState>) -> Config {
    st.cfg().clone()
}

#[tauri::command]
pub fn config_set(app: AppHandle, st: State<'_, AppState>, cfg: Config) -> Config {
    let new = {
        let mut cur = st.cfg();
        let new = cfg.sanitized(&cur);
        *cur = new.clone();
        new
    };
    new.save(&st.data_dir);
    crate::apply_config(&app, &new);
    new
}

#[tauri::command]
pub fn seen_version(st: State<'_, AppState>, version: String) {
    let mut c = st.cfg();
    c.seen_version = version.chars().take(20).collect();
    c.save(&st.data_dir);
}

// ————————————————————————————————————————————————————————————— abrir, crear, bloquear

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub path: String,
    pub name: String,
    pub exists: bool,
    pub hello: bool,
}

#[tauri::command]
pub fn recent_list(st: State<'_, AppState>) -> Vec<Recent> {
    let hello_map = config::load_hello(&st.data_dir);
    st.cfg()
        .recent
        .iter()
        .map(|p| {
            let path = Path::new(p);
            // Para saber si tiene Hello hace falta el id de la bóveda (está en la cabecera, sin cifrar)
            let id = std::fs::read(path).ok().and_then(|b| file::read_header(&b).ok()).map(|(h, _)| h.vault_id);
            Recent {
                path: p.clone(),
                name: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                exists: path.exists(),
                hello: id.is_some_and(|id| hello_map.contains_key(&id)),
            }
        })
        .collect()
}

#[tauri::command]
pub fn recent_forget(st: State<'_, AppState>, path: String) {
    let mut c = st.cfg();
    c.recent.retain(|r| r != &path);
    c.save(&st.data_dir);
}

fn vault_path(path: &str) -> Res<PathBuf> {
    check_len(path, 1000)?;
    let mut p = PathBuf::from(path.trim());
    if p.as_os_str().is_empty() || !p.is_absolute() {
        return Err(AppError::new("io.badPath"));
    }
    if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("poxipass")) {
        p.set_extension("poxipass");
    }
    Ok(p)
}

fn start_session(st: &AppState, path: PathBuf, opened: file::Opened) -> VaultInfo {
    let s = Session::new(path.clone(), opened);
    let out = info(st, &s);
    *st.sess() = Some(s);
    let mut c = st.cfg();
    c.touch_recent(&path, &out.vault_id);
    c.save(&st.data_dir);
    out
}

#[tauri::command]
pub async fn vault_create(st: State<'_, AppState>, path: String, name: String, password: String) -> Res<Vec<String>> {
    let path = vault_path(&path)?;
    let password = Zeroizing::new(password);
    check_len(&name, 80)?;
    check_len(&password, 1024)?;
    if generator::strength(&password).score < 3 {
        return Err(AppError::new("vault.weakPassword"));
    }
    let t = now();
    let name = if name.trim().is_empty() { "Mi llavero".to_string() } else { name.trim().to_string() };
    let vault = Vault { name, created: t, modified: t, ..Default::default() };
    // Las copias van a la carpeta de la bóveda nueva, que aún no tiene id: se usa una provisional
    let tmp_bk = st.data_dir.join("backups").join("_new");
    let (opened, phrase) = file::create(&path, vault, &password, KdfParams::default(), &tmp_bk)?;
    let _ = std::fs::remove_dir_all(tmp_bk);
    let info = start_session(&st, path.clone(), opened);
    // Primera copia desde el minuto 0 (y en la carpeta extra, si hay)
    file::backup(&std::fs::read(&path)?, &backups_dir(&st.data_dir, &info.vault_id), true)?;
    st.mirror(&path, &info.vault_id, true);
    Ok(phrase.words)
}

#[tauri::command]
pub async fn vault_open(st: State<'_, AppState>, path: String, password: String) -> Res<VaultInfo> {
    let path = vault_path(&path)?;
    let password = Zeroizing::new(password);
    check_len(&password, 1024)?;
    let opened = file::open_with_password(&path, &password)?;
    // Abrir con la contraseña renueva el plazo de Windows Hello
    let mut hello_map = config::load_hello(&st.data_dir);
    if let Some(slot) = hello_map.get_mut(&opened.header.vault_id) {
        slot.last_password = now();
        config::save_hello(&st.data_dir, &hello_map);
    }
    Ok(start_session(&st, path, opened))
}

#[tauri::command]
pub async fn vault_open_hello(st: State<'_, AppState>, path: String) -> Res<VaultInfo> {
    let path = vault_path(&path)?;
    let bytes = std::fs::read(&path)?;
    let (header, _) = file::read_header(&bytes)?;
    let slot = config::load_hello(&st.data_dir).remove(&header.vault_id).ok_or_else(|| AppError::new("hello.notFound"))?;
    let key = tauri::async_runtime::spawn_blocking(move || hello::unlock(&header.vault_id, &slot))
        .await
        .map_err(|_| AppError::new("hello.failed"))??;
    let opened = file::open_with_key(&path, key)?;
    Ok(start_session(&st, path, opened))
}

#[tauri::command]
pub async fn vault_recover(st: State<'_, AppState>, path: String, phrase: String) -> Res<VaultInfo> {
    let path = vault_path(&path)?;
    let phrase = Zeroizing::new(phrase);
    check_len(&phrase, 2000)?;
    let entropy = crate::recovery::parse(&phrase)?;
    let opened = file::open_with_recovery(&path, &entropy)?;
    let mut s = Session::new(path.clone(), opened);
    s.must_change_password = true;
    let out = info(&st, &s);
    *st.sess() = Some(s);
    Ok(out)
}

#[tauri::command]
pub fn vault_lock(app: AppHandle, st: State<'_, AppState>) {
    crate::lock(&app, &st, "manual");
}

#[tauri::command]
pub fn vault_info(st: State<'_, AppState>) -> Option<VaultInfo> {
    st.sess().as_ref().map(|s| info(&st, s))
}

#[tauri::command]
pub fn vault_rename(st: State<'_, AppState>, name: String) -> Res<VaultInfo> {
    check_len(&name, 80)?;
    mutate(&st, |s| {
        s.vault.name = name.trim().to_string();
        Ok(())
    })?;
    with(&st, |s| Ok(info(&st, s)))
}

/// Tras un fallo al guardar: `retry`, `overwrite` (el archivo cambió por fuera y se pisa), `copy` (se guarda
/// en otro sitio) o `reload` (se descartan los cambios y se vuelve a pedir la contraseña).
#[tauri::command]
pub fn vault_resolve_save(app: AppHandle, st: State<'_, AppState>, mode: String, path: Option<String>) -> Res<Option<VaultInfo>> {
    match mode.as_str() {
        "reload" => {
            crate::lock(&app, &st, "reload");
            Ok(None)
        }
        "copy" => {
            let p = vault_path(path.as_deref().unwrap_or(""))?;
            let id = with(&st, |s| {
                let bk = st.backups(s);
                s.save_as(p.clone(), &bk)?;
                Ok(s.vault_id().to_string())
            })?;
            let mut c = st.cfg();
            c.touch_recent(&p, &id);
            c.save(&st.data_dir);
            drop(c);
            Ok(vault_info(st))
        }
        m => {
            with(&st, |s| st.persist(s, m == "overwrite"))?;
            Ok(vault_info(st))
        }
    }
}

#[tauri::command]
pub async fn verify_password(st: State<'_, AppState>, password: String) -> Res<bool> {
    let password = Zeroizing::new(password);
    let path = st.sess().as_ref().map(|s| s.path.clone()).ok_or_else(|| AppError::new("vault.locked"))?;
    match file::open_with_password(&path, &password) {
        Ok(_) => Ok(true),
        Err(e) if e.code == "vault.wrongPassword" => Ok(false),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn password_change(st: State<'_, AppState>, current: Option<String>, new: String) -> Res<VaultInfo> {
    let new = Zeroizing::new(new);
    check_len(&new, 1024)?;
    if generator::strength(&new).score < 3 {
        return Err(AppError::new("vault.weakPassword"));
    }
    let must = st.sess().as_ref().map(|s| s.must_change_password).ok_or_else(|| AppError::new("vault.locked"))?;
    if !must {
        let ok = verify_password(st.clone(), current.unwrap_or_default()).await?;
        if !ok {
            return Err(AppError::new("vault.wrongPassword"));
        }
    }
    let mut g = st.sess();
    let s = g.as_mut().ok_or_else(|| AppError::new("vault.locked"))?;
    file::set_password(&mut s.header, &s.key, &new)?;
    st.persist(s, false)?;
    s.must_change_password = false;
    let out = info(&st, s);
    let id = s.vault_id().to_string();
    let path = s.path.clone();
    drop(g);
    let mut c = st.cfg();
    c.touch_recent(&path, &id);
    c.save(&st.data_dir);
    drop(c);
    let mut hello_map = config::load_hello(&st.data_dir);
    if let Some(slot) = hello_map.get_mut(&id) {
        slot.last_password = now();
        config::save_hello(&st.data_dir, &hello_map);
    }
    Ok(out)
}

/// Kit de emergencia nuevo: el anterior deja de valer.
#[tauri::command]
pub fn recovery_regenerate(st: State<'_, AppState>) -> Res<Vec<String>> {
    mutate(&st, |s| Ok(file::set_recovery(&mut s.header, &s.key).words.clone()))
}

// ————————————————————————————————————————————————————————————— entradas

#[tauri::command]
pub fn entries_list(st: State<'_, AppState>) -> Res<Vec<EntrySummary>> {
    with(&st, |s| Ok(s.vault.entries.iter().map(|e| summary(s, e)).collect()))
}

#[tauri::command]
pub fn entry_get(st: State<'_, AppState>, id: String) -> Res<EntryView> {
    with(&st, |s| Ok(view(s, s.entry(&id)?)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldInput {
    pub id: Option<String>,
    pub key: String,
    pub label: String,
    /// `None`: se queda el valor que ya tenía (los secretos no hace falta mandarlos si no cambian)
    pub value: Option<String>,
    pub secret: bool,
    pub kind: FieldKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub id: Option<String>,
    pub kind: Kind,
    pub title: String,
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub fields: Vec<FieldInput>,
    pub autotype: Option<String>,
    #[serde(default)]
    pub protected: bool,
}

fn validate(input: &EntryInput) -> Res<()> {
    check_len(&input.title, 200)?;
    if input.fields.len() > MAX_FIELDS || input.tags.len() > 30 {
        return Err(AppError::new("input.tooMany"));
    }
    for t in &input.tags {
        check_len(t, 40)?;
    }
    for f in &input.fields {
        check_len(&f.key, 40)?;
        check_len(&f.label, 100)?;
        if f.value.as_ref().is_some_and(|v| v.len() > MAX_VALUE) {
            return Err(AppError::new("input.tooLong"));
        }
        if f.kind == FieldKind::Totp && f.value.as_ref().is_some_and(|v| !v.trim().is_empty() && totp::parse(v).is_err()) {
            return Err(AppError::new("totp.invalid"));
        }
    }
    if let Some(a) = &input.autotype {
        check_len(a, 500)?;
    }
    Ok(())
}

pub(crate) fn snapshot(e: &Entry) -> Snapshot {
    Snapshot { at: e.modified, title: e.title.clone(), fields: e.fields.iter().map(Field::duplicate).collect() }
}

#[tauri::command]
pub fn entry_save(st: State<'_, AppState>, input: EntryInput) -> Res<EntryView> {
    validate(&input)?;
    mutate(&st, |s| {
        if input.folder.as_ref().is_some_and(|f| !s.vault.folders.iter().any(|x| &x.id == f)) {
            return Err(AppError::new("folder.notFound"));
        }
        let t = now();
        let existing = match &input.id {
            Some(id) => Some(s.vault.entries.iter().position(|e| &e.id == id).ok_or_else(|| AppError::new("entry.notFound"))?),
            None if s.vault.entries.len() >= MAX_ENTRIES => return Err(AppError::new("input.tooMany")),
            None => None,
        };
        let old_fields: Vec<Field> = existing.map(|i| s.vault.entries[i].fields.iter().map(Field::duplicate).collect()).unwrap_or_default();
        let mut fields = vec![];
        for fi in &input.fields {
            let old = fi.id.as_ref().and_then(|id| old_fields.iter().find(|f| &f.id == id));
            let mut value = match (&fi.value, old) {
                (Some(v), _) => Sv::text(v.trim_end_matches(['\r', '\n'])),
                (None, Some(o)) => o.value.duplicate(),
                (None, None) => Sv::text(""),
            };
            if fi.secret {
                value.seal(&s.sealer);
            } else if value.is_sealed() {
                value = value.to_plain(&s.sealer);
            }
            fields.push(Field {
                id: old.map(|o| o.id.clone()).unwrap_or_else(new_id),
                key: fi.key.trim().to_string(),
                label: fi.label.trim().to_string(),
                value,
                secret: fi.secret,
                kind: fi.kind,
            });
        }
        let title = if input.title.trim().is_empty() { "Sin título".to_string() } else { input.title.trim().to_string() };
        let mut tags: Vec<String> = input.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        tags.dedup();
        let autotype = input.autotype.as_ref().map(|a| a.trim().to_string()).filter(|a| !a.is_empty());
        let id = match existing {
            Some(i) => {
                let e = &mut s.vault.entries[i];
                let snap = snapshot(e);
                e.history.insert(0, snap);
                e.history.truncate(MAX_HISTORY);
                e.kind = input.kind;
                e.title = title;
                e.folder = input.folder.clone();
                e.tags = tags;
                e.favorite = input.favorite;
                e.fields = fields;
                e.autotype = autotype;
                e.protected = input.protected;
                e.modified = t;
                e.id.clone()
            }
            None => {
                let id = new_id();
                s.vault.entries.push(Entry {
                    id: id.clone(),
                    kind: input.kind,
                    title,
                    folder: input.folder.clone(),
                    tags,
                    favorite: input.favorite,
                    fields,
                    attachments: vec![],
                    icon: None,
                    autotype,
                    history: vec![],
                    created: t,
                    modified: t,
                    used: None,
                    deleted: None,
                    breach: None,
                    archived: false,
                    protected: input.protected,
                });
                id
            }
        };
        Ok(view(s, s.entry(&id)?))
    })
}

#[tauri::command]
pub fn entry_favorite(st: State<'_, AppState>, id: String, on: bool) -> Res<()> {
    entries_bulk(st, vec![id], if on { "favorite" } else { "unfavorite" }.into(), None).map(|_| ())
}

#[tauri::command]
pub fn entry_delete(st: State<'_, AppState>, id: String) -> Res<()> {
    entries_bulk(st, vec![id], "trash".into(), None).map(|_| ())
}

#[tauri::command]
pub fn entry_restore(st: State<'_, AppState>, id: String) -> Res<()> {
    entries_bulk(st, vec![id], "restore".into(), None).map(|_| ())
}

/// Acción sobre varias entradas a la vez (selección múltiple, arrastrar a una carpeta…). Devuelve cuántas.
#[tauri::command]
pub fn entries_bulk(st: State<'_, AppState>, ids: Vec<String>, action: String, folder: Option<String>) -> Res<usize> {
    if ids.len() > MAX_ENTRIES {
        return Err(AppError::new("input.tooMany"));
    }
    mutate(&st, |s| {
        if action == "move" && folder.as_ref().is_some_and(|f| !s.vault.folders.iter().any(|x| &x.id == f)) {
            return Err(AppError::new("folder.notFound"));
        }
        let t = now();
        let mut n = 0;
        for e in s.vault.entries.iter_mut().filter(|e| ids.contains(&e.id)) {
            match action.as_str() {
                "trash" => e.deleted = Some(t),
                "restore" => e.deleted = None,
                "archive" => e.archived = true,
                "unarchive" => e.archived = false,
                "favorite" => e.favorite = true,
                "unfavorite" => e.favorite = false,
                "move" => e.folder = folder.clone(),
                _ => return Err(AppError::new("input.empty")),
            }
            e.modified = t;
            n += 1;
        }
        Ok(n)
    })
}

/// Duplicados: se queda `keep` (con las etiquetas de todas) y las demás van a la papelera.
#[tauri::command]
pub fn duplicates_merge(st: State<'_, AppState>, keep: String, others: Vec<String>) -> Res<usize> {
    mutate(&st, |s| {
        let t = now();
        let tags: Vec<String> = s.vault.entries.iter().filter(|e| others.contains(&e.id)).flat_map(|e| e.tags.clone()).collect();
        let k = s.entry_mut(&keep)?;
        for tag in tags {
            if !k.tags.contains(&tag) {
                k.tags.push(tag);
            }
        }
        k.modified = t;
        let mut n = 0;
        for e in s.vault.entries.iter_mut().filter(|e| others.contains(&e.id) && e.id != keep) {
            e.deleted = Some(t);
            e.modified = t;
            n += 1;
        }
        Ok(n)
    })
}

#[tauri::command]
pub fn entry_purge(st: State<'_, AppState>, id: String) -> Res<()> {
    mutate(&st, |s| {
        s.vault.purge(|e| !(e.id == id && e.deleted.is_some()));
        Ok(())
    })
}

#[tauri::command]
pub fn trash_empty(st: State<'_, AppState>) -> Res<()> {
    mutate(&st, |s| {
        s.vault.purge(|e| e.deleted.is_none());
        Ok(())
    })
}

#[tauri::command]
pub fn entry_history_restore(st: State<'_, AppState>, id: String, index: usize) -> Res<EntryView> {
    mutate(&st, |s| {
        let e = s.entry_mut(&id)?;
        let old = e.history.get(index).ok_or_else(|| AppError::new("entry.notFound"))?;
        let (title, fields) = (old.title.clone(), old.fields.iter().map(Field::duplicate).collect());
        let snap = snapshot(e);
        e.history.insert(0, snap);
        e.history.truncate(MAX_HISTORY);
        e.title = title;
        e.fields = fields;
        e.modified = now();
        Ok(view(s, s.entry(&id)?))
    })
}

/// Ventana tras confirmar con Hello una entrada sensible, para no pedirlo en cada clic
const VERIFIED_FOR: Duration = Duration::from_secs(60);

pub fn is_verified(st: &AppState, id: &str) -> bool {
    st.verified.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|t| t.elapsed() < VERIFIED_FOR)
}

pub fn grant(st: &AppState, id: &str) {
    st.verified.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), std::time::Instant::now());
}

/// Entrada sensible sin confirmar → `entry.protected` (la interfaz pide Hello y repite)
fn guard(st: &AppState, s: &Session, id: &str) -> Res<()> {
    if s.entry(id)?.protected && !is_verified(st, id) {
        return Err(AppError::new("entry.protected"));
    }
    Ok(())
}

/// Confirma que eres tú para una entrada sensible: con la contraseña maestra si se da, o con Windows Hello.
#[tauri::command]
pub async fn verify_entry(st: State<'_, AppState>, id: String, password: Option<String>) -> Res<()> {
    match password {
        Some(pw) => {
            if !verify_password(st.clone(), pw).await? {
                return Err(AppError::new("vault.wrongPassword"));
            }
        }
        None => {
            let msg = if crate::config::is_spanish(&st.cfg()) { "Confirma que eres tú para ver esta entrada" } else { "Confirm it's you to view this entry" };
            let msg = msg.to_string();
            tauri::async_runtime::spawn_blocking(move || hello::consent(&msg)).await.map_err(|_| AppError::new("hello.failed"))??;
        }
    }
    grant(&st, &id);
    Ok(())
}

fn find_field<'a>(s: &'a Session, entry: &str, field: &str) -> Res<&'a Field> {
    s.entry(entry)?.fields.iter().find(|f| f.id == field).ok_or_else(|| AppError::new("entry.notFound"))
}

#[tauri::command]
pub fn reveal_field(st: State<'_, AppState>, entry: String, field: String) -> Res<String> {
    with(&st, |s| {
        let f = find_field(s, &entry, &field)?;
        if f.secret {
            guard(&st, s, &entry)?;
        }
        Ok(f.value.reveal_string(&s.sealer).to_string())
    })
}

/// Copia un campo (o el código 2FA si es un campo TOTP). Devuelve los segundos hasta que se borra (0 = nunca).
#[tauri::command]
pub fn copy_field(st: State<'_, AppState>, entry: String, field: String) -> Res<u32> {
    let secs = st.cfg().clipboard_secs;
    with(&st, |s| {
        let f = find_field(s, &entry, &field)?;
        if f.secret {
            guard(&st, s, &entry)?;
        }
        let value = f.value.reveal_string(&s.sealer);
        let text = if f.kind == FieldKind::Totp { Zeroizing::new(totp::code(&value)?.code) } else { value };
        let secret = f.secret;
        clipboard::copy_secret(&text, (secret && secs > 0).then(|| Duration::from_secs(secs as u64)))?;
        s.mark_used(&entry);
        Ok(if secret { secs } else { 0 })
    })
}

#[tauri::command]
pub fn totp_code(st: State<'_, AppState>, entry: String, field: String) -> Res<Code> {
    with(&st, |s| {
        guard(&st, s, &entry)?;
        totp::code(&find_field(s, &entry, &field)?.value.reveal_string(&s.sealer))
    })
}

/// Código QR (SVG) de un texto. Se usa con lo que ya está en pantalla (el generador).
#[tauri::command]
pub fn qr_svg(text: String) -> Res<String> {
    check_len(&text, 2000)?;
    qr(&text)
}

fn qr(text: &str) -> Res<String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|_| AppError::new("input.tooLong"))?;
    Ok(code.render::<svg::Color>().min_dimensions(260, 260).quiet_zone(true).dark_color(svg::Color("#0a0e17")).light_color(svg::Color("#ffffff")).build())
}

/// QR de un campo, o en una entrada Wi-Fi, el QR estándar para conectarse (`WIFI:T:WPA;S:…;P:…;;`)
#[tauri::command]
pub fn field_qr(st: State<'_, AppState>, entry: String, field: Option<String>) -> Res<String> {
    with(&st, |s| {
        guard(&st, s, &entry)?;
        let e = s.entry(&entry)?;
        let text = match field {
            Some(f) => find_field(s, &entry, &f)?.value.reveal_string(&s.sealer),
            None => {
                let esc = |v: &str| v.chars().flat_map(|c| if "\\;,:\"".contains(c) { vec!['\\', c] } else { vec![c] }).collect::<String>();
                let ssid = plain_of(e, "ssid");
                let pw = e.field("password").map(|f| f.value.reveal_string(&s.sealer)).unwrap_or_default();
                let sec = plain_of(e, "security");
                let t = if pw.is_empty() { "nopass" } else if sec.to_uppercase().contains("WEP") { "WEP" } else { "WPA" };
                Zeroizing::new(format!("WIFI:T:{t};S:{};P:{};;", esc(&ssid), esc(&pw)))
            }
        };
        qr(&text)
    })
}

#[tauri::command]
pub fn totp_from_image(base64: String) -> Res<String> {
    if base64.len() > 20 * 1024 * 1024 {
        return Err(AppError::new("input.tooLong"));
    }
    let raw = base64.split_once(',').map(|(_, b)| b).unwrap_or(&base64);
    totp::from_image(&B64.decode(raw.trim()).map_err(|_| AppError::new("totp.badImage"))?)
}

#[tauri::command]
pub fn totp_from_file(path: String) -> Res<String> {
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 20 * 1024 * 1024 {
        return Err(AppError::new("input.tooLong"));
    }
    totp::from_image(&std::fs::read(&path)?)
}

#[tauri::command]
pub fn attachment_add(st: State<'_, AppState>, entry: String, path: String) -> Res<EntryView> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p)?;
    if !meta.is_file() {
        return Err(AppError::new("io.notFound"));
    }
    if meta.len() > MAX_ATTACHMENT {
        return Err(AppError::new("attachment.tooBig"));
    }
    mutate(&st, |s| {
        let total: u64 = s.vault.entries.iter().flat_map(|e| &e.attachments).map(|a| a.size).sum();
        if total + meta.len() > MAX_ATTACHMENTS_TOTAL {
            return Err(AppError::new("attachment.vaultFull"));
        }
        let mut data = Sv::Plain(std::fs::read(&p)?);
        data.seal(&s.sealer);
        let name: String = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().chars().take(200).collect();
        let mime = mime_of(&name);
        let e = s.entry_mut(&entry)?;
        e.attachments.push(Attachment { id: new_id(), name, mime, size: meta.len(), data });
        e.modified = now();
        Ok(view(s, s.entry(&entry)?))
    })
}

fn mime_of(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
    .into()
}

#[tauri::command]
pub fn attachment_save(st: State<'_, AppState>, entry: String, attachment: String, dest: String) -> Res<()> {
    with(&st, |s| {
        guard(&st, s, &entry)?;
        let a = s.entry(&entry)?.attachments.iter().find(|a| a.id == attachment).ok_or_else(|| AppError::new("entry.notFound"))?;
        std::fs::write(&dest, &*a.data.reveal(&s.sealer))?;
        Ok(())
    })
}

#[derive(Serialize)]
pub struct AttachmentPreview {
    pub mime: String,
    /// base64 (imágenes) o el texto tal cual
    pub data: String,
}

/// Ver un adjunto sin guardarlo en disco (imágenes y texto). Nada se escribe: todo va en memoria.
#[tauri::command]
pub fn attachment_preview(st: State<'_, AppState>, entry: String, attachment: String) -> Res<AttachmentPreview> {
    with(&st, |s| {
        guard(&st, s, &entry)?;
        let a = s.entry(&entry)?.attachments.iter().find(|a| a.id == attachment).ok_or_else(|| AppError::new("entry.notFound"))?;
        let bytes = a.data.reveal(&s.sealer);
        let data = if a.mime.starts_with("image/") {
            B64.encode(&*bytes)
        } else if a.mime == "text/plain" || a.name.to_lowercase().ends_with(".md") {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            return Err(AppError::new("attachment.noPreview"));
        };
        Ok(AttachmentPreview { mime: a.mime.clone(), data })
    })
}

#[tauri::command]
pub fn attachment_delete(st: State<'_, AppState>, entry: String, attachment: String) -> Res<EntryView> {
    mutate(&st, |s| {
        let e = s.entry_mut(&entry)?;
        e.attachments.retain(|a| a.id != attachment);
        e.modified = now();
        Ok(view(s, s.entry(&entry)?))
    })
}

#[tauri::command]
pub fn folder_save(st: State<'_, AppState>, id: Option<String>, name: String) -> Res<Vec<Folder>> {
    check_len(&name, 60)?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::new("input.empty"));
    }
    mutate(&st, |s| {
        match id {
            Some(id) => s.vault.folders.iter_mut().find(|f| f.id == id).ok_or_else(|| AppError::new("folder.notFound"))?.name = name,
            None => s.vault.folders.push(Folder { id: new_id(), name, icon: None }),
        }
        Ok(s.vault.folders.clone())
    })
}

#[tauri::command]
pub fn folder_delete(st: State<'_, AppState>, id: String) -> Res<Vec<Folder>> {
    mutate(&st, |s| {
        s.vault.folders.retain(|f| f.id != id);
        for e in s.vault.entries.iter_mut().filter(|e| e.folder.as_deref() == Some(id.as_str())) {
            e.folder = None;
        }
        Ok(s.vault.folders.clone())
    })
}

// ————————————————————————————————————————————————————————————— generador

#[tauri::command]
pub fn generate(st: State<'_, AppState>, opts: GenOptions) -> Generated {
    let opts = opts.clamped();
    let out = generator::generate(&opts);
    let mut c = st.cfg();
    if c.generator != opts {
        c.generator = opts;
        c.save(&st.data_dir);
    }
    out
}

#[tauri::command]
pub fn strength(text: String) -> Strength {
    let text = Zeroizing::new(text);
    generator::strength(&text)
}

/// Copia un texto que ya está en la interfaz (lo generado) con el portapapeles seguro. Con `remember`, entra en
/// el historial del generador.
#[tauri::command]
pub fn copy_text(st: State<'_, AppState>, text: String, remember: bool) -> Res<u32> {
    let text = Zeroizing::new(text);
    check_len(&text, 4096)?;
    let secs = st.cfg().clipboard_secs;
    clipboard::copy_secret(&text, (secs > 0).then(|| Duration::from_secs(secs as u64)))?;
    if remember {
        remember_generated(&st, &text);
    }
    Ok(secs)
}

// ————————————————————————————————————————————————————————————— salud

#[tauri::command]
pub fn audit_report(st: State<'_, AppState>) -> Res<Report> {
    with(&st, |s| Ok(audit::report(s)))
}

/// Comprueba en Have I Been Pwned (solo salen del PC 5 caracteres del SHA-1 de cada contraseña).
#[tauri::command]
pub async fn hibp_check(st: State<'_, AppState>) -> Res<Report> {
    // (id, sha1, huella) sin tener la bóveda bloqueada mientras se espera a la red
    let jobs: Vec<(String, String, [u8; 32])> = with(&st, |s| {
        Ok(s.vault
            .entries
            .iter()
            .filter(|e| e.deleted.is_none())
            .filter_map(|e| {
                let pw = e.field("password").filter(|f| !f.value.is_empty())?.value.reveal(&s.sealer);
                Some((e.id.clone(), audit::sha1_hex(&pw), audit::breach_fp(&s.key, &pw)))
            })
            .collect())
    })?;
    let mut ranges: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut results = vec![];
    for (id, hash, fp) in jobs {
        let prefix = hash[..5].to_string();
        if !ranges.contains_key(&prefix) {
            let body = audit::fetch_range(&st.http, &prefix).await?;
            ranges.insert(prefix.clone(), body);
        }
        results.push((id, audit::count_in_range(&ranges[&prefix], &hash[5..]), fp));
    }
    let t = now();
    mutate(&st, |s| {
        for (id, count, fp) in results {
            if let Ok(e) = s.entry_mut(&id) {
                e.breach = Some(Breach { fp: ByteBuf::from(fp.to_vec()), count, checked: t });
            }
        }
        Ok(audit::report(s))
    })
}

// ————————————————————————————————————————————————————————————— importar / exportar

/// Lee el archivo a importar (hasta 200 MB: un `.kdbx` o un `.1pux` llevan los adjuntos dentro).
fn import_read(p: &PathBuf) -> Res<Zeroizing<Vec<u8>>> {
    if std::fs::metadata(p)?.len() > 200 * 1024 * 1024 {
        return Err(AppError::new("input.tooLong"));
    }
    Ok(Zeroizing::new(std::fs::read(p)?))
}

/// Contraseña y/o archivo llave de un `.kdbx` (no se guardan: cada paso los recibe de la interfaz)
fn import_key(password: Option<String>, keyfile: Option<String>) -> Res<Option<import::Key>> {
    let keyfile = match keyfile {
        Some(k) => {
            if std::fs::metadata(&k)?.len() > 1024 * 1024 {
                return Err(AppError::new("import.badKeyFile"));
            }
            Some(Zeroizing::new(std::fs::read(&k)?))
        }
        None => None,
    };
    let password = Zeroizing::new(password.unwrap_or_default());
    check_len(&password, 1024)?;
    Ok((!password.is_empty() || keyfile.is_some()).then_some(import::Key { password, keyfile }))
}

#[tauri::command]
pub async fn import_preview(st: State<'_, AppState>, path: String, password: Option<String>, keyfile: Option<String>) -> Res<Preview> {
    let p = PathBuf::from(&path);
    let out = import::preview(&import_read(&p)?, import_key(password, keyfile)?.as_ref())?;
    *st.last_import.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
    /// Adjuntos que no caben (10 MB cada uno, 50 MB en todo el llavero)
    pub files_skipped: usize,
}

/// Huella de «la misma entrada»: título, usuario y contraseña (esta, sin abrirla: HMAC con la clave de sesión)
fn dedupe_key(s: &Session, title: &str, user: &str, password: &[u8]) -> (String, String, [u8; 32]) {
    (title.trim().to_lowercase(), user.trim().to_lowercase(), s.sealer.fingerprint(password))
}

#[tauri::command]
pub async fn import_run(st: State<'_, AppState>, password: Option<String>, keyfile: Option<String>) -> Res<ImportResult> {
    let p = st.last_import.lock().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| AppError::new("io.notFound"))?;
    let (_, items) = import::parse(&import_read(&p)?, import_key(password, keyfile)?.as_ref())?;
    mutate(&st, |s| {
        if s.vault.entries.len() + items.len() > MAX_ENTRIES {
            return Err(AppError::new("input.tooMany"));
        }
        let t = now();
        // Lo que ya hay (para no duplicar al importar dos veces el mismo archivo)
        let mut seen: std::collections::HashSet<_> = s
            .vault
            .entries
            .iter()
            .filter(|e| e.deleted.is_none())
            .map(|e| {
                let pw = e.field("password").map(|f| f.value.reveal(&s.sealer)).unwrap_or_default();
                dedupe_key(s, &e.title, &plain_of(e, "username"), &pw)
            })
            .collect();
        let (mut added, mut skipped, mut files_skipped) = (0, 0, 0);
        let mut total: u64 = s.vault.entries.iter().flat_map(|e| &e.attachments).map(|a| a.size).sum();
        for it in items {
            let get = |k: &str| it.fields.iter().find(|f| f.key == k).map(|f| f.value.as_str()).unwrap_or("");
            if !seen.insert(dedupe_key(s, &it.title, get("username"), get("password").as_bytes())) {
                skipped += 1;
                continue;
            }
            added += 1;
            let folder = it.folder.as_ref().map(|name| {
                let name: String = name.chars().take(60).collect();
                match s.vault.folders.iter().find(|f| f.name.eq_ignore_ascii_case(&name)) {
                    Some(f) => f.id.clone(),
                    None => {
                        let id = new_id();
                        s.vault.folders.push(Folder { id: id.clone(), name, icon: None });
                        id
                    }
                }
            });
            let mut e = Entry {
                id: new_id(),
                kind: it.kind,
                title: it.title.chars().take(200).collect(),
                folder,
                tags: it.tags.iter().map(|t| t.trim().chars().take(40).collect::<String>()).filter(|t| !t.is_empty()).take(30).collect(),
                favorite: it.favorite,
                fields: it
                    .fields
                    .iter()
                    .filter(|f| f.value.len() <= MAX_VALUE)
                    .take(MAX_FIELDS)
                    .map(|f| Field { id: new_id(), key: f.key.into(), label: f.label.clone(), value: Sv::text(&f.value), secret: f.secret, kind: f.kind })
                    .collect(),
                attachments: it
                    .files
                    .into_iter()
                    .filter_map(|a| {
                        let size = a.data.len() as u64;
                        if size > MAX_ATTACHMENT || total + size > MAX_ATTACHMENTS_TOTAL {
                            files_skipped += 1;
                            return None;
                        }
                        total += size;
                        let name: String = a.name.chars().take(200).collect();
                        Some(Attachment { id: new_id(), mime: mime_of(&name), name, size, data: Sv::Plain(a.data) })
                    })
                    .collect(),
                icon: None,
                autotype: None,
                history: vec![],
                created: t,
                modified: t,
                used: None,
                deleted: None,
                breach: None,
                archived: it.archived,
                protected: false,
            };
            e.seal(&s.sealer);
            s.vault.entries.push(e);
        }
        Ok(ImportResult { added, skipped, files_skipped })
    })
}

/// Borra el archivo importado (va en claro). Solo ese: la interfaz no puede borrar otra cosa.
#[tauri::command]
pub fn import_delete_source(st: State<'_, AppState>) -> Res<()> {
    if let Some(p) = st.last_import.lock().unwrap_or_else(|e| e.into_inner()).take() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

#[tauri::command]
pub fn export_csv(st: State<'_, AppState>, path: String) -> Res<usize> {
    with(&st, |s| {
        let rows: Vec<ExportRow> = s
            .vault
            .entries
            .iter()
            .filter(|e| e.deleted.is_none())
            .map(|e| ExportRow {
                folder: e.folder.as_ref().and_then(|id| s.vault.folders.iter().find(|f| &f.id == id)).map(|f| f.name.clone()).unwrap_or_default(),
                favorite: e.favorite,
                kind: e.kind,
                title: e.title.clone(),
                fields: e.fields.iter().map(|f| (f.key.clone(), f.label.clone(), f.value.reveal_string(&s.sealer).to_string())).collect(),
            })
            .collect();
        let bytes = Zeroizing::new(import::export_csv(&rows)?);
        std::fs::write(&path, &*bytes)?;
        Ok(rows.len())
    })
}

/// Copia cifrada de la bóveda (el mismo archivo, recién guardado).
#[tauri::command]
pub fn export_copy(st: State<'_, AppState>, path: String) -> Res<()> {
    with(&st, |s| {
        st.persist(s, false)?;
        std::fs::copy(&s.path, &path)?;
        Ok(())
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupView {
    pub name: String,
    pub size: u64,
    pub modified: i64,
    /// En la carpeta extra (no en AppData)
    pub extra: bool,
}

/// Carpetas de copias de un llavero: la de AppData y, si hay, la extra
fn backup_dirs(st: &AppState, vault_id: &str) -> Vec<(PathBuf, bool)> {
    let mut v = vec![(backups_dir(&st.data_dir, vault_id), false)];
    if let Some(d) = config::extra_dir(&st.cfg(), vault_id) {
        v.push((d, true));
    }
    v
}

fn list_backups_for(st: &AppState, vault_id: &str) -> Vec<BackupView> {
    let mut out: Vec<BackupView> = backup_dirs(st, vault_id)
        .into_iter()
        .flat_map(|(dir, extra)| {
            file::list_backups(&dir).into_iter().filter_map(move |p| {
                let m = p.metadata().ok()?;
                let modified = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
                Some(BackupView { name: p.file_name()?.to_string_lossy().to_string(), size: m.len(), modified, extra })
            })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// El id de un llavero por su ruta: de la cabecera si el archivo está, o de lo que recordamos si ha desaparecido
fn vault_id_of(st: &AppState, path: &Path) -> Option<String> {
    std::fs::read(path).ok().and_then(|b| file::read_header(&b).ok()).map(|(h, _)| h.vault_id).or_else(|| st.cfg().vault_id_for(path))
}

/// Pone una copia en `path`. Si ya hay archivo, antes se guarda como copia (no se pierde nada).
fn restore_backup(st: &AppState, path: &Path, vault_id: &str, name: &str, extra: bool) -> Res<()> {
    let valid = name.len() < 40 && name.ends_with(".poxipass") && name.chars().all(|c| c.is_ascii_digit() || "-b.poxipass".contains(c));
    let dir = backup_dirs(st, vault_id).into_iter().find(|(_, e)| *e == extra).map(|(d, _)| d);
    let (true, Some(dir)) = (valid, dir) else { return Err(AppError::new("io.badPath")) };
    let chosen = std::fs::read(dir.join(name))?;
    file::read_header(&chosen)?;
    let local = backups_dir(&st.data_dir, vault_id);
    if let Ok(current) = std::fs::read(path) {
        file::backup(&current, &local, true)?;
    }
    file::write(path, &chosen, None, &local)?;
    Ok(())
}

#[tauri::command]
pub fn backups_list(st: State<'_, AppState>) -> Res<Vec<BackupView>> {
    let id = with(&st, |s| Ok(s.vault_id().to_string()))?;
    Ok(list_backups_for(&st, &id))
}

/// Copias de un llavero sin abrirlo (por ejemplo, si su archivo ha desaparecido)
#[tauri::command]
pub fn backups_for(st: State<'_, AppState>, path: String) -> Res<Vec<BackupView>> {
    let path = vault_path(&path)?;
    Ok(vault_id_of(&st, &path).map(|id| list_backups_for(&st, &id)).unwrap_or_default())
}

/// Vuelve a una copia con el llavero abierto: se bloquea (se abre con la contraseña que tuviera entonces).
#[tauri::command]
pub fn backup_restore(app: AppHandle, st: State<'_, AppState>, name: String, extra: bool) -> Res<()> {
    let (path, id) = with(&st, |s| Ok((s.path.clone(), s.vault_id().to_string())))?;
    restore_backup(&st, &path, &id, &name, extra)?;
    crate::lock(&app, &st, "restored");
    Ok(())
}

/// Vuelve a una copia sin abrir el llavero (pantalla de desbloqueo)
#[tauri::command]
pub fn backup_restore_to(st: State<'_, AppState>, path: String, name: String, extra: bool) -> Res<()> {
    let path = vault_path(&path)?;
    if st.sess().as_ref().is_some_and(|s| s.path == path) {
        return Err(AppError::new("vault.locked"));
    }
    let id = vault_id_of(&st, &path).ok_or_else(|| AppError::new("io.notFound"))?;
    restore_backup(&st, &path, &id, &name, extra)
}

#[tauri::command]
pub fn fs_places(app: AppHandle) -> Vec<crate::fs::Place> {
    let p = app.path();
    let mut out = vec![];
    for (kind, dir) in [("documents", p.document_dir()), ("desktop", p.desktop_dir()), ("downloads", p.download_dir()), ("home", p.home_dir())] {
        if let Ok(d) = dir {
            out.push(crate::fs::Place { kind: kind.into(), name: String::new(), path: d.to_string_lossy().to_string() });
        }
    }
    out.extend(crate::fs::drives());
    out
}

#[tauri::command]
pub fn clipboard_read() -> Res<String> {
    clipboard::read_text()
}

/// Carpeta nueva desde el explorador propio. Devuelve su ruta.
#[tauri::command]
pub fn fs_mkdir(dir: String, name: String) -> Res<String> {
    check_len(&dir, 1000)?;
    let p = crate::fs::save_path(&dir, &name, "")?;
    if p.exists() {
        return Err(AppError::new("vault.exists"));
    }
    std::fs::create_dir(&p)?;
    Ok(p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn fs_list(dir: String, exts: Vec<String>) -> Res<crate::fs::Listing> {
    check_len(&dir, 1000)?;
    crate::fs::list(Path::new(&dir), &exts)
}

#[derive(Serialize)]
pub struct SavePath {
    pub path: String,
    pub exists: bool,
}

#[tauri::command]
pub fn fs_save_path(dir: String, name: String, ext: String) -> Res<SavePath> {
    check_len(&dir, 1000)?;
    let p = crate::fs::save_path(&dir, &name, &ext)?;
    Ok(SavePath { exists: p.exists(), path: p.to_string_lossy().to_string() })
}

/// PDF del kit de emergencia, con el diseño de la app (los textos llegan ya traducidos).
#[tauri::command]
pub fn kit_pdf(path: String, words: Vec<String>, texts: crate::pdf::KitTexts) -> Res<()> {
    let p = PathBuf::from(&path);
    if !p.is_absolute() || p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("pdf")) || words.len() > 30 || words.iter().any(|w| w.chars().count() > 40) {
        return Err(AppError::new("io.badPath"));
    }
    for s in [&texts.title, &texts.vault, &texts.steps, &texts.warn, &texts.how, &texts.where_label, &texts.vault_label, &texts.date_label, &texts.date] {
        check_len(s, 2000)?;
    }
    std::fs::write(&p, crate::pdf::kit(&texts, &words))?;
    Ok(())
}

// ————————————————————————————————————————————————————————————— iconos

#[tauri::command]
pub async fn icon_fetch(st: State<'_, AppState>, entry: String) -> Res<EntryView> {
    let url = with(&st, |s| Ok(plain_of(s.entry(&entry)?, "url")))?;
    let png = icons::fetch(&st.http, &url).await?;
    mutate(&st, |s| {
        s.entry_mut(&entry)?.icon = Some(ByteBuf::from(png));
        Ok(view(s, s.entry(&entry)?))
    })
}

#[tauri::command]
pub async fn icons_fetch_missing(st: State<'_, AppState>) -> Res<usize> {
    let todo: Vec<(String, String)> = with(&st, |s| {
        Ok(s.vault.entries.iter().filter(|e| e.icon.is_none() && e.deleted.is_none()).map(|e| (e.id.clone(), plain_of(e, "url"))).filter(|(_, u)| icons::host(u).is_some()).collect())
    })?;
    let mut got = vec![];
    for (id, url) in todo {
        if let Ok(png) = icons::fetch(&st.http, &url).await {
            got.push((id, png));
        }
    }
    mutate(&st, |s| {
        let n = got.len();
        for (id, png) in got {
            if let Ok(e) = s.entry_mut(&id) {
                e.icon = Some(ByteBuf::from(png));
            }
        }
        Ok(n)
    })
}

// ————————————————————————————————————————————————————————————— Windows Hello

#[tauri::command]
pub async fn hello_available() -> bool {
    tauri::async_runtime::spawn_blocking(hello::available).await.unwrap_or(false)
}

#[tauri::command]
pub async fn hello_enable(st: State<'_, AppState>) -> Res<()> {
    let (id, key) = with(&st, |s| Ok((s.vault_id().to_string(), Zeroizing::new(*s.key))))?;
    let id2 = id.clone();
    let slot = tauri::async_runtime::spawn_blocking(move || hello::enroll(&id2, &key)).await.map_err(|_| AppError::new("hello.failed"))??;
    let mut map = config::load_hello(&st.data_dir);
    map.insert(id, slot);
    config::save_hello(&st.data_dir, &map);
    Ok(())
}

#[tauri::command]
pub async fn hello_disable(st: State<'_, AppState>) -> Res<()> {
    let id = with(&st, |s| Ok(s.vault_id().to_string()))?;
    let mut map = config::load_hello(&st.data_dir);
    map.remove(&id);
    config::save_hello(&st.data_dir, &map);
    let _ = tauri::async_runtime::spawn_blocking(move || hello::remove(&id)).await;
    Ok(())
}

// ————————————————————————————————————————————————————————————— auto-type

pub struct AutotypeJob {
    pub actions: Vec<Action>,
    /// Clave 2FA si la entrada la tiene y la secuencia no la escribe: el código se copia al terminar
    pub totp: Option<Zeroizing<String>>,
}

/// Resuelve la secuencia de una entrada a teclas (los secretos, en claro solo aquí).
pub fn autotype_job(s: &Session, id: &str, default_seq: &str) -> Res<AutotypeJob> {
    let e = s.entry(id)?;
    let seq = e.autotype.clone().unwrap_or_else(|| default_seq.to_string());
    let tokens = autotype::parse(&seq);
    let totp = (!tokens.contains(&Token::Totp))
        .then(|| e.field("totp").filter(|f| !f.value.is_empty()).map(|f| f.value.reveal_string(&s.sealer)))
        .flatten();
    let mut out = vec![];
    for t in tokens {
        out.push(match t {
            Token::Text(t) => Action::Text(Zeroizing::new(t)),
            Token::Tab => Action::Key(autotype::TAB),
            Token::Enter => Action::Key(autotype::ENTER),
            Token::Delay(ms) => Action::Delay(ms),
            Token::Totp => {
                let f = e.field("totp").ok_or_else(|| AppError::new("autotype.noField"))?;
                Action::Text(Zeroizing::new(totp::code(&f.value.reveal_string(&s.sealer))?.code))
            }
            Token::Field(k) => {
                let f = e
                    .fields
                    .iter()
                    .find(|f| f.key == k || (!f.label.is_empty() && f.label.eq_ignore_ascii_case(&k)))
                    .ok_or_else(|| AppError::with("autotype.noField", k.clone()))?;
                Action::Text(f.value.reveal_string(&s.sealer))
            }
        });
    }
    Ok(AutotypeJob { actions: out, totp })
}

/// Apunta que se ha usado (para ordenar por «más usadas» y la búsqueda). Si no se puede guardar, da igual.
pub fn mark_used(st: &AppState, id: &str) {
    if let Some(s) = st.sess().as_mut() {
        s.mark_used(id);
    }
}

pub fn remember_generated(st: &AppState, value: &str) {
    let mut h = st.gen_history.lock().unwrap_or_else(|e| e.into_inner());
    if h.first().is_some_and(|(_, v)| v.as_str() == value) {
        return;
    }
    h.insert(0, (now(), Zeroizing::new(value.to_string())));
    h.truncate(20);
}

#[derive(Serialize)]
pub struct GenItem {
    pub at: i64,
    pub value: String,
}

#[tauri::command]
pub fn set_busy(st: State<'_, AppState>, on: bool) {
    st.busy.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
pub fn hotkey_failures(st: State<'_, AppState>) -> Vec<String> {
    st.hotkey_failed.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn gen_history(st: State<'_, AppState>) -> Vec<GenItem> {
    st.gen_history.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(at, v)| GenItem { at: *at, value: v.to_string() }).collect()
}

/// Escribe una entrada en la ventana que estaba activa al pulsar el atajo (desde el selector o el acceso
/// rápido). La ventana desde la que se elige se esconde para devolver el foco.
#[tauri::command]
pub fn autotype_entry(app: AppHandle, window: tauri::Window, st: State<'_, AppState>, id: String) -> Res<()> {
    let target = st.autotype_target.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| AppError::new("autotype.noTarget"))?;
    let seq = st.cfg().autotype_sequence.clone();
    let job = with(&st, |s| {
        guard(&st, s, &id)?;
        let j = autotype_job(s, &id, &seq)?;
        s.mark_used(&id);
        Ok(j)
    })?;
    let _ = window.hide();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        if crate::win::focus(target) {
            std::thread::sleep(Duration::from_millis(150));
            crate::run_autotype(&app, job);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn autotype_cancel(st: State<'_, AppState>) {
    if let Some(t) = st.autotype_target.lock().unwrap_or_else(|e| e.into_inner()).take() {
        crate::win::focus(t);
    }
}

#[tauri::command]
pub fn autostart_get(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, on: bool) -> Res<()> {
    use tauri_plugin_autostart::ManagerExt;
    let l = app.autolaunch();
    if on { l.enable() } else { l.disable() }.map_err(|_| AppError::new("io.failed"))
}

#[derive(Deserialize)]
pub struct TrayLabels {
    pub open: String,
    pub generate: String,
    pub lock: String,
    pub quit: String,
}

#[tauri::command]
pub fn tray_labels(app: AppHandle, labels: TrayLabels) {
    crate::set_tray_menu(&app, &labels);
}

#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}
