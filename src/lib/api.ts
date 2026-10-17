import { invoke } from '@tauri-apps/api/core'

// Tipos espejo de los de Rust (commands.rs). Los secretos nunca vienen en estas estructuras.

export type Kind = 'login' | 'card' | 'note' | 'identity' | 'wifi' | 'ssh' | 'license' | 'bank' | 'document'
export type FieldKind = 'text' | 'password' | 'url' | 'email' | 'totp' | 'multiline' | 'date'

export interface AppError {
  code: string
  detail?: string
}

export interface GenOptions {
  mode: 'password' | 'passphrase' | 'username'
  exclude: string
  length: number
  lower: boolean
  upper: boolean
  digits: boolean
  symbols: boolean
  avoidAmbiguous: boolean
  words: number
  separator: string
  capitalize: boolean
  number: boolean
}

export interface Config {
  lang: 'es' | 'en' | null
  theme: 'dark' | 'light' | 'system'
  accent: 'violet' | 'blue' | 'emerald' | 'pink' | 'amber' | 'red'
  recent: string[]
  extraBackupDir: string | null
  lockIdleMin: number
  lockOnSessionLock: boolean
  lockOnSleep: boolean
  lockOnMinimize: boolean
  clipboardSecs: number
  closeToTray: boolean
  autotypeEnabled: boolean
  autotypeHotkey: string
  autotypeSequence: string
  quickEnabled: boolean
  quickHotkey: string
  favicons: boolean
  bridgeEnabled: boolean
  generator: GenOptions
  seenVersion: string
}

/** Navegador con la extensión conectada (la clave nunca llega aquí) */
export interface PairedBrowser {
  id: string
  name: string
  created: number
  used: number
}

export interface AppInfo {
  version: string
  portable: boolean
  startHidden: boolean
  defaultDir: string
}

export interface Folder {
  id: string
  name: string
  icon?: string | null
}

export interface VaultInfo {
  name: string
  path: string
  vaultId: string
  folders: Folder[]
  mustChangePassword: boolean
  saveFailed: boolean
  helloEnabled: boolean
}

export interface Recent {
  path: string
  name: string
  exists: boolean
  hello: boolean
}

export interface EntrySummary {
  id: string
  kind: Kind
  title: string
  subtitle: string
  url: string
  folder: string | null
  tags: string[]
  favorite: boolean
  icon: string | null
  hasTotp: boolean
  modified: number
  used: number | null
  deleted: number | null
  created: number
  archived: boolean
  protected: boolean
}

export interface FieldView {
  id: string
  key: string
  label: string
  value: string | null
  secret: boolean
  empty: boolean
  kind: FieldKind
}

export interface Strength {
  score: number
  crackSecs: number
  bits: number
}

export interface EntryView extends EntrySummary {
  fields: FieldView[]
  attachments: { id: string; name: string; mime: string; size: number }[]
  history: { index: number; at: number; title: string }[]
  autotype: string | null
  strength: Strength | null
  breach: number | null
}

export interface FieldInput {
  id: string | null
  key: string
  label: string
  value: string | null
  secret: boolean
  kind: FieldKind
}

export interface EntryInput {
  id: string | null
  kind: Kind
  title: string
  folder: string | null
  tags: string[]
  favorite: boolean
  fields: FieldInput[]
  autotype: string | null
  protected: boolean
}

export interface Report {
  score: number
  total: number
  weak: string[]
  reused: string[][]
  old: string[]
  no2fa: string[]
  breached: [string, number][]
  duplicates: string[][]
  expiring: [string, number][]
  breachChecked: number
}

export interface Backup {
  name: string
  size: number
  modified: number
  extra: boolean
}

export interface Preview {
  format: string
  count: number
  sample: string[]
  files: number
  /** Va en claro: al acabar se ofrece borrarlo (un .kdbx no) */
  plain: boolean
}

/** Contraseña y archivo llave de un .kdbx */
export interface ImportKey {
  password: string
  keyfile: string | null
}

export const isAppError = (e: unknown): e is AppError => typeof e === 'object' && e !== null && 'code' in e

/** Llamada a Rust. Los errores llegan como `{ code, detail }` (nunca con rutas ni secretos). */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args).catch((e: unknown) => {
    throw isAppError(e) ? e : ({ code: 'unknown' } satisfies AppError)
  })
}

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  configGet: () => call<Config>('config_get'),
  configSet: (cfg: Config) => call<Config>('config_set', { cfg }),
  seenVersion: (version: string) => call<void>('seen_version', { version }),
  recentList: () => call<Recent[]>('recent_list'),
  recentForget: (path: string) => call<void>('recent_forget', { path }),

  vaultCreate: (path: string, name: string, password: string) => call<string[]>('vault_create', { path, name, password }),
  vaultOpen: (path: string, password: string) => call<VaultInfo>('vault_open', { path, password }),
  vaultOpenHello: (path: string) => call<VaultInfo>('vault_open_hello', { path }),
  vaultRecover: (path: string, phrase: string) => call<VaultInfo>('vault_recover', { path, phrase }),
  vaultLock: () => call<void>('vault_lock'),
  vaultInfo: () => call<VaultInfo | null>('vault_info'),
  vaultRename: (name: string) => call<VaultInfo>('vault_rename', { name }),
  vaultResolveSave: (mode: 'retry' | 'overwrite' | 'copy' | 'reload', path?: string) =>
    call<VaultInfo | null>('vault_resolve_save', { mode, path: path ?? null }),
  verifyPassword: (password: string) => call<boolean>('verify_password', { password }),
  passwordChange: (current: string | null, next: string) => call<VaultInfo>('password_change', { current, new: next }),
  recoveryRegenerate: () => call<string[]>('recovery_regenerate'),

  entriesList: () => call<EntrySummary[]>('entries_list'),
  entryGet: (id: string) => call<EntryView>('entry_get', { id }),
  entrySave: (input: EntryInput) => call<EntryView>('entry_save', { input }),
  entriesBulk: (ids: string[], action: string, folder: string | null) => call<number>('entries_bulk', { ids, action, folder }),
  duplicatesMerge: (keep: string, others: string[]) => call<number>('duplicates_merge', { keep, others }),
  entryFavorite: (id: string, on: boolean) => call<void>('entry_favorite', { id, on }),
  entryDelete: (id: string) => call<void>('entry_delete', { id }),
  entryRestore: (id: string) => call<void>('entry_restore', { id }),
  entryPurge: (id: string) => call<void>('entry_purge', { id }),
  trashEmpty: () => call<void>('trash_empty'),
  historyRestore: (id: string, index: number) => call<EntryView>('entry_history_restore', { id, index }),
  revealField: (entry: string, field: string) => call<string>('reveal_field', { entry, field }),
  copyField: (entry: string, field: string) => call<number>('copy_field', { entry, field }),
  verifyEntry: (id: string, password: string | null) => call<void>('verify_entry', { id, password }),
  qrSvg: (text: string) => call<string>('qr_svg', { text }),
  fieldQr: (entry: string, field: string | null) => call<string>('field_qr', { entry, field }),
  attachmentPreview: (entry: string, attachment: string) => call<{ mime: string; data: string }>('attachment_preview', { entry, attachment }),
  totpCode: (entry: string, field: string) => call<{ code: string; remaining: number; period: number }>('totp_code', { entry, field }),
  totpFromImage: (base64: string) => call<string>('totp_from_image', { base64 }),
  totpFromFile: (path: string) => call<string>('totp_from_file', { path }),
  attachmentAdd: (entry: string, path: string) => call<EntryView>('attachment_add', { entry, path }),
  attachmentSave: (entry: string, attachment: string, dest: string) => call<void>('attachment_save', { entry, attachment, dest }),
  attachmentDelete: (entry: string, attachment: string) => call<EntryView>('attachment_delete', { entry, attachment }),
  folderSave: (id: string | null, name: string) => call<Folder[]>('folder_save', { id, name }),
  folderDelete: (id: string) => call<Folder[]>('folder_delete', { id }),

  generate: (opts: GenOptions) => call<{ value: string; bits: number }>('generate', { opts }),
  strength: (text: string) => call<Strength>('strength', { text }),
  copyText: (text: string, remember = false) => call<number>('copy_text', { text, remember }),
  clipboardRead: () => call<string>('clipboard_read'),
  fsMkdir: (dir: string, name: string) => call<string>('fs_mkdir', { dir, name }),
  setBusy: (on: boolean) => call<void>('set_busy', { on }),
  hotkeyFailures: () => call<string[]>('hotkey_failures'),
  genHistory: () => call<{ at: number; value: string }[]>('gen_history'),

  auditReport: () => call<Report>('audit_report'),
  hibpCheck: () => call<Report>('hibp_check'),

  importPreview: (path: string, key?: ImportKey) => call<Preview>('import_preview', { path, password: key?.password || null, keyfile: key?.keyfile ?? null }),
  importRun: (key?: ImportKey) =>
    call<{ added: number; skipped: number; filesSkipped: number }>('import_run', { password: key?.password || null, keyfile: key?.keyfile ?? null }),
  importDeleteSource: () => call<void>('import_delete_source'),
  exportCsv: (path: string) => call<number>('export_csv', { path }),
  exportCopy: (path: string) => call<void>('export_copy', { path }),
  backupsList: () => call<Backup[]>('backups_list'),
  backupsFor: (path: string) => call<Backup[]>('backups_for', { path }),
  backupRestore: (name: string, extra: boolean) => call<void>('backup_restore', { name, extra }),
  backupRestoreTo: (path: string, name: string, extra: boolean) => call<void>('backup_restore_to', { path, name, extra }),
  kitPdf: (path: string, words: string[], texts: Record<string, string>) => call<void>('kit_pdf', { path, words, texts }),
  fsPlaces: () => call<{ kind: string; name: string; path: string }[]>('fs_places'),
  fsList: (dir: string, exts: string[]) =>
    call<{ path: string; parent: string | null; items: { name: string; path: string; dir: boolean; size: number; modified: number }[] }>('fs_list', { dir, exts }),
  fsSavePath: (dir: string, name: string, ext: string) => call<{ path: string; exists: boolean }>('fs_save_path', { dir, name, ext }),

  iconFetch: (entry: string) => call<EntryView>('icon_fetch', { entry }),
  iconsFetchMissing: () => call<number>('icons_fetch_missing'),

  helloAvailable: () => call<boolean>('hello_available'),
  helloEnable: () => call<void>('hello_enable'),
  helloDisable: () => call<void>('hello_disable'),

  autotypeEntry: (id: string) => call<void>('autotype_entry', { id }),
  autotypeCancel: () => call<void>('autotype_cancel'),
  autostartGet: () => call<boolean>('autostart_get'),
  autostartSet: (on: boolean) => call<void>('autostart_set', { on }),
  trayLabels: (labels: { open: string; generate: string; lock: string; quit: string }) => call<void>('tray_labels', { labels }),
  quit: () => call<void>('app_quit'),

  extensionTargets: () => call<{ key: string; name: string }[]>('extension_targets'),
  extensionInstall: (key: string) => call<void>('extension_install', { key }),
  browsersList: () => call<PairedBrowser[]>('browsers_list'),
  browserForget: (id: string) => call<PairedBrowser[]>('browser_forget', { id }),
  bridgePairAnswer: (ok: boolean) => call<void>('bridge_pair_answer', { ok })
}
