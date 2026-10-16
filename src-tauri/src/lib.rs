pub mod audit;
pub mod autotype;
pub mod bridge;
pub mod browser;
pub mod clipboard;
pub mod commands;
pub mod config;
pub mod crypto;
pub mod error;
pub mod fs;
pub mod generator;
pub mod hello;
pub mod icons;
pub mod import;
pub mod pdf;
pub mod recovery;
pub mod relay;
pub mod session;
pub mod totp;
pub mod util;
pub mod vault;
pub mod win;

use commands::{AppState, TrayLabels};
use config::Config;
use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Bloquea: se suelta la sesión (y con ella la Vault Key y todos los secretos, que se borran al liberarse).
pub fn lock(app: &AppHandle, st: &AppState, reason: &str) {
    let mut taken = st.sess().take();
    // Las marcas de «usado» que quedaban en memoria se escriben antes de soltar la sesión
    if let Some(s) = taken.as_mut().filter(|s| s.used_dirty) {
        let _ = st.persist(s, false);
    }
    let was_open = taken.is_some();
    drop(taken);
    st.autotype_target.lock().unwrap_or_else(|e| e.into_inner()).take();
    st.gen_history.lock().unwrap_or_else(|e| e.into_inner()).clear();
    st.verified.lock().unwrap_or_else(|e| e.into_inner()).clear();
    if let Some(q) = app.get_webview_window("quick") {
        let _ = q.hide();
    }
    if was_open {
        let _ = app.emit("locked", reason);
    }
}

/// Lo que depende de los ajustes y vive fuera de la interfaz: los atajos globales (auto-type y acceso rápido).
pub fn apply_config(app: &AppHandle, cfg: &Config) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut failed = vec![];
    if cfg.autotype_enabled {
        let ok = gs.on_shortcut(cfg.autotype_hotkey.as_str(), |app, _, ev| {
            if ev.state == ShortcutState::Pressed {
                autotype_trigger(app);
            }
        });
        if ok.is_err() {
            failed.push(cfg.autotype_hotkey.clone());
        }
    }
    if cfg.quick_enabled {
        let ok = gs.on_shortcut(cfg.quick_hotkey.as_str(), |app, _, ev| {
            if ev.state == ShortcutState::Pressed {
                quick_toggle(app);
            }
        });
        if ok.is_err() {
            failed.push(cfg.quick_hotkey.clone());
        }
    }
    // Se guarda, además de avisar: al arrancar la interfaz aún no escucha y el aviso se perdería
    for h in &failed {
        let _ = app.emit("hotkey-failed", h);
    }
    let st = app.state::<AppState>();
    *st.hotkey_failed.lock().unwrap_or_else(|e| e.into_inner()) = failed;
    bridge::register(&st.data_dir, cfg.bridge_enabled);
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Pick {
    ids: Vec<String>,
    title: String,
    locked: bool,
}

fn hwnd_of(app: &AppHandle, label: &str) -> Option<isize> {
    app.get_webview_window(label).and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize)
}

/// Escribe una entrada en la ventana activa y, si tiene 2FA y la secuencia no lo escribe, deja el código copiado
/// (la siguiente pantalla de la web casi siempre lo pide).
pub fn run_autotype(app: &AppHandle, job: commands::AutotypeJob) {
    let app = app.clone();
    std::thread::spawn(move || {
        autotype::perform(&job.actions);
        let Some(secret) = job.totp else { return };
        let Ok(code) = totp::code(&secret) else { return };
        let secs = app.state::<AppState>().cfg().clipboard_secs;
        if clipboard::copy_secret(&code.code, (secs > 0).then(|| Duration::from_secs(secs as u64))).is_ok() {
            let _ = app.emit("totp-copied", secs);
        }
    });
}

/// Atajo de auto-type. La URL de la pestaña se lee en otro hilo: UI Automation puede tardar y no debe congelar nada.
fn autotype_trigger(app: &AppHandle) {
    let target = win::foreground();
    if target == 0 || Some(target) == hwnd_of(app, "main") || Some(target) == hwnd_of(app, "quick") {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let st = app.state::<AppState>();
        let page = browser::url(target);
        let title = win::window_title(target);
        *st.autotype_target.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
        let seq = st.cfg().autotype_sequence.clone();
        let guard = st.sess();
        let Some(s) = guard.as_ref().filter(|s| !s.must_change_password) else {
            drop(guard);
            show_window(&app);
            let _ = app.emit("autotype-pick", Pick { ids: vec![], title, locked: true });
            return;
        };
        let live = || {
            s.vault.entries.iter().filter(|e| e.deleted.is_none() && !e.archived).map(|e| {
                let urls: Vec<String> = e.fields.iter().filter(|f| f.kind == vault::model::FieldKind::Url).map(|f| f.value.plain_str().to_string()).collect();
                (e.id.as_str(), e.title.as_str(), urls)
            })
        };
        // Anti-phishing: solo se escribe sin preguntar si el dominio real de la página casa con una única entrada.
        // Con URL nunca se mira el título (una web falsa puede titularse «Netflix»); sin URL (no es un
        // navegador), el título solo sugiere y siempre se confirma en el selector.
        let (ids, sure) = match page.as_deref() {
            Some(p) => {
                let ids = autotype::matches_url(p, live().map(|(id, _, u)| (id, u)));
                let sure = ids.len() == 1;
                (ids, sure)
            }
            None => (autotype::matches(&title, live()), false),
        };
        if sure {
            let protected = s.entry(&ids[0]).is_ok_and(|e| e.protected) && !commands::is_verified(&st, &ids[0]);
            let job = commands::autotype_job(s, &ids[0], &seq);
            drop(guard);
            if protected {
                if let Err(e) = hello::consent(if config::is_spanish(&st.cfg()) { "Confirma que eres tú para escribir esta entrada" } else { "Confirm it's you to type this entry" }) {
                    let _ = app.emit("autotype-error", e);
                    return;
                }
                commands::grant(&st, &ids[0]);
                // El diálogo de Hello se ha llevado el foco: se devuelve a la ventana donde escribir
                win::focus(target);
                std::thread::sleep(Duration::from_millis(200));
            }
            st.autotype_target.lock().unwrap_or_else(|e| e.into_inner()).take();
            match job {
                Ok(j) => {
                    commands::mark_used(&st, &ids[0]);
                    run_autotype(&app, j);
                }
                Err(e) => {
                    let _ = app.emit("autotype-error", e);
                }
            }
        } else {
            drop(guard);
            show_window(&app);
            let _ = app.emit("autotype-pick", Pick { ids, title: page.unwrap_or(title), locked: false });
        }
    });
}

/// Acceso rápido: una barra flotante para buscar y escribir/copiar sin abrir la ventana principal.
fn quick_toggle(app: &AppHandle) {
    let Some(w) = app.get_webview_window("quick") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    let target = win::foreground();
    if target != 0 && Some(target) != hwnd_of(app, "main") {
        *app.state::<AppState>().autotype_target.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    }
    // Justo debajo del ratón (en su pantalla); si no se sabe dónde está, al centro
    match (win::cursor_and_work_area(), w.outer_size()) {
        (Some((cursor, area)), Ok(size)) => {
            let (x, y) = win::place_below(cursor, (size.width as i32, size.height as i32), area);
            let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        }
        _ => {
            let _ = w.center();
        }
    }
    let _ = w.show();
    let _ = w.set_focus();
    let _ = app.emit_to("quick", "quick-show", ());
}

pub fn set_tray_menu(app: &AppHandle, l: &TrayLabels) {
    let build = || -> tauri::Result<Menu<tauri::Wry>> {
        Menu::with_items(
            app,
            &[
                &MenuItem::with_id(app, "open", &l.open, true, None::<&str>)?,
                &MenuItem::with_id(app, "generate", &l.generate, true, None::<&str>)?,
                &MenuItem::with_id(app, "lock", &l.lock, true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", &l.quit, true, None::<&str>)?,
            ],
        )
    };
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build()) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn on_tray_menu(app: &AppHandle, id: &str) {
    let st = app.state::<AppState>();
    match id {
        "open" => show_window(app),
        "lock" => lock(app, &st, "manual"),
        "quit" => app.exit(0),
        "generate" => {
            let (opts, secs) = {
                let c = st.cfg();
                (c.generator.clone(), c.clipboard_secs)
            };
            let g = generator::generate(&opts);
            let pw = zeroize::Zeroizing::new(g.value);
            if clipboard::copy_secret(&pw, (secs > 0).then(|| Duration::from_secs(secs as u64))).is_ok() {
                commands::remember_generated(&st, &pw);
                let _ = app.emit("generated-copied", secs);
            }
        }
        _ => {}
    }
}

/// Cada 2 s: ¿toca bloquear por inactividad, por Win+L o porque el PC se ha suspendido?
fn spawn_lock_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = SystemTime::now();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = SystemTime::now();
            let gap = now.duration_since(last).unwrap_or_default().as_secs();
            last = now;
            let st = app.state::<AppState>();
            if st.sess().is_none() {
                continue;
            }
            // ¿Lo ha cambiado Drive, OneDrive u otro PC? Se fusiona solo
            let synced = {
                let mut g = st.sess();
                match g.as_mut().filter(|s| !s.must_change_password) {
                    Some(s) => {
                        let bk = st.backups_of(s);
                        s.sync_from_disk(&bk).unwrap_or(0)
                    }
                    None => 0,
                }
            };
            if synced > 0 {
                let _ = app.emit("vault-synced", synced);
            }
            let c = st.cfg().clone();
            let rules = session::LockRules { idle_min: c.lock_idle_min, on_session_lock: c.lock_on_session_lock, on_sleep: c.lock_on_sleep };
            let idle = if st.busy.load(std::sync::atomic::Ordering::Relaxed) { 0 } else { win::idle_ms() };
            if let Some(reason) = session::should_lock(&rules, idle, win::session_locked(), gap) {
                lock(&app, &st, reason);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let start_hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        // El relé arranca la app con `--hidden` si no estaba abierta; si ya lo estaba, no se saca la ventana
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if !args.iter().any(|a| a == "--hidden") {
                show_window(app)
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let cfg = Config::load(&data_dir);
            app.manage(AppState {
                session: Mutex::new(None),
                config: Mutex::new(cfg.clone()),
                data_dir,
                http: reqwest::Client::builder().user_agent("PoxiPass").timeout(Duration::from_secs(15)).build()?,
                autotype_target: Mutex::new(None),
                last_import: Mutex::new(None),
                gen_history: Mutex::new(vec![]),
                hotkey_failed: Mutex::new(vec![]),
                busy: std::sync::atomic::AtomicBool::new(false),
                verified: Mutex::new(Default::default()),
                start_hidden,
            });
            // Las ventanas se crean ahora, con el estado ya listo: si la interfaz cargara antes, sus primeras
            // llamadas fallarían y la ventana se quedaría en negro.
            for w in &app.config().app.windows {
                tauri::WebviewWindowBuilder::from_config(app.handle(), w)?.build()?;
            }
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().ok_or("sin icono")?)
                .tooltip("PoxiPass")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| on_tray_menu(app, ev.id().as_ref()))
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;
            set_tray_menu(
                app.handle(),
                &TrayLabels { open: "Abrir PoxiPass".into(), generate: "Generar y copiar".into(), lock: "Bloquear".into(), quit: "Salir".into() },
            );
            apply_config(app.handle(), &cfg);
            spawn_lock_watcher(app.handle().clone());
            bridge::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|w, ev| {
            let app = w.app_handle();
            let st = app.state::<AppState>();
            match ev {
                WindowEvent::CloseRequested { api, .. } if st.cfg().close_to_tray => {
                    api.prevent_close();
                    let _ = w.hide();
                }
                WindowEvent::Resized(_) if st.cfg().lock_on_minimize && w.is_minimized().unwrap_or(false) => {
                    lock(app, &st, "minimize");
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::config_get,
            commands::config_set,
            commands::seen_version,
            commands::recent_list,
            commands::recent_forget,
            commands::vault_create,
            commands::vault_open,
            commands::vault_open_hello,
            commands::vault_recover,
            commands::vault_lock,
            commands::vault_info,
            commands::vault_rename,
            commands::vault_resolve_save,
            commands::verify_password,
            commands::password_change,
            commands::recovery_regenerate,
            commands::entries_list,
            commands::entry_get,
            commands::entry_save,
            commands::entry_favorite,
            commands::entries_bulk,
            commands::duplicates_merge,
            commands::entry_delete,
            commands::entry_restore,
            commands::entry_purge,
            commands::trash_empty,
            commands::entry_history_restore,
            commands::reveal_field,
            commands::copy_field,
            commands::totp_code,
            commands::verify_entry,
            commands::qr_svg,
            commands::field_qr,
            commands::attachment_preview,
            commands::totp_from_image,
            commands::totp_from_file,
            commands::attachment_add,
            commands::attachment_save,
            commands::attachment_delete,
            commands::folder_save,
            commands::folder_delete,
            commands::generate,
            commands::strength,
            commands::copy_text,
            commands::audit_report,
            commands::hibp_check,
            commands::import_preview,
            commands::import_run,
            commands::import_delete_source,
            commands::export_csv,
            commands::export_copy,
            commands::kit_pdf,
            commands::fs_places,
            commands::fs_list,
            commands::fs_mkdir,
            commands::clipboard_read,
            commands::fs_save_path,
            commands::backups_list,
            commands::backup_restore,
            commands::backups_for,
            commands::backup_restore_to,
            commands::icon_fetch,
            commands::icons_fetch_missing,
            commands::hello_available,
            commands::hello_enable,
            commands::hello_disable,
            commands::autotype_entry,
            commands::autotype_cancel,
            commands::gen_history,
            commands::hotkey_failures,
            commands::set_busy,
            commands::autostart_get,
            commands::autostart_set,
            commands::tray_labels,
            commands::app_quit,
            bridge::browsers_list,
            bridge::browser_forget,
            bridge::bridge_pair_answer,
            bridge::extension_targets,
            bridge::extension_install,
        ])
        .run(tauri::generate_context!())
        .expect("no se ha podido arrancar PoxiPass");
}
