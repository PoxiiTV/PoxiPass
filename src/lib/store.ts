import { create } from 'zustand'
import { api, type AppInfo, type Config, type EntrySummary, type Kind, type VaultInfo } from './api'

export type Screen = 'boot' | 'welcome' | 'unlock' | 'create' | 'recover' | 'mustChange' | 'vault' | 'generatorOnly'
export type Section = 'all' | 'favorites' | Kind | 'trash' | 'archive' | `folder:${string}` | 'generator' | 'health'
export type Sort = 'title' | 'modified' | 'used' | 'created'

export interface Toast {
  id: number
  text: string
  kind: 'ok' | 'err' | 'info'
  /** Barra de cuenta atrás (portapapeles) */
  secs?: number
  action?: { label: string; run: () => void }
}

export interface PickReq {
  mode: 'open' | 'save' | 'folder'
  title: string
  /** Extensiones que se enseñan (vacío = todas) */
  exts: string[]
  /** Guardar: nombre propuesto y extensión que se fuerza */
  name?: string
  ext?: string
  dir?: string
  resolve: (path: string | null) => void
}

export interface Pick {
  ids: string[]
  title: string
  locked: boolean
}

interface State {
  screen: Screen
  app: AppInfo | null
  config: Config | null
  info: VaultInfo | null
  entries: EntrySummary[]
  section: Section
  selected: string | null
  query: string
  sort: Sort
  /** Editor abierto: `id` null = entrada nueva */
  editing: { id: string | null; kind: Kind } | null
  settingsOpen: boolean
  toasts: Toast[]
  pick: Pick | null
  conflict: boolean
  /** Bóveda que se va a desbloquear */
  unlockPath: string | null
  /** Kit que se está enseñando o imprimiendo */
  kit: { words: string[]; vault: string; date: number } | null
  /** Visor de un secreto en grande, un QR o un adjunto */
  viewer:
    | { type: 'large'; title: string; text: string }
    | { type: 'qr'; title: string; svg: string }
    | { type: 'image'; title: string; src: string }
    | { type: 'text'; title: string; text: string }
    | null
  /** Petición de la contraseña maestra (para confirmar sin Windows Hello) */
  pwPrompt: { reason: string; resolve: (pw: string | null) => void } | null
  /** Varias entradas elegidas a la vez (Ctrl/Mayús + clic) */
  multi: string[]
  /** El editor tiene cambios sin guardar */
  dirty: boolean
  /** Explorador de archivos abierto */
  picker: PickReq | null
  /** Pregunta de confirmación abierta */
  question: { text: string; yes: string; resolve: (ok: boolean) => void } | null
  /** Un navegador pide conectarse con la extensión */
  pairing: { browser: string; code: string } | null
  /** Sube cada vez que cambia la entrada seleccionada por fuera (para recargar el detalle) */
  rev: number
}

export const useStore = create<State>(() => ({
  screen: 'boot',
  app: null,
  config: null,
  info: null,
  entries: [],
  section: 'all',
  selected: null,
  query: '',
  sort: 'title',
  editing: null,
  settingsOpen: false,
  toasts: [],
  pick: null,
  conflict: false,
  unlockPath: null,
  kit: null,
  question: null,
  pairing: null,
  picker: null,
  dirty: false,
  multi: [],
  viewer: null,
  pwPrompt: null,
  rev: 0
}))

export const setState = useStore.setState
export const getState = useStore.getState

let toastId = 0
export function toast(text: string, kind: Toast['kind'] = 'ok', secs?: number, action?: Toast['action']): void {
  const id = ++toastId
  setState((s) => ({ toasts: [...s.toasts.slice(-3), { id, text, kind, secs, action }] }))
  setTimeout(() => dismiss(id), Math.max(action ? 6000 : 2600, (secs ?? 0) * 1000))
}

export function dismiss(id: number): void {
  setState((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }))
}

export async function refreshEntries(): Promise<void> {
  const entries = await api.entriesList()
  setState((s) => ({ entries, rev: s.rev + 1, selected: entries.some((e) => e.id === s.selected) ? s.selected : null }))
}

export async function refreshInfo(): Promise<void> {
  setState({ info: await api.vaultInfo() })
}

export async function enterVault(info: VaultInfo): Promise<void> {
  setState({ info, screen: info.mustChangePassword ? 'mustChange' : 'vault', unlockPath: info.path })
  if (!info.mustChangePassword) {
    await refreshEntries()
  }
}

/** Bloqueada (por Rust o a mano): fuera todo lo de la bóveda de la memoria de la interfaz. */
export function onLocked(): void {
  const s = getState()
  setState({
    screen: s.unlockPath ? 'unlock' : 'welcome',
    info: null,
    entries: [],
    selected: null,
    editing: null,
    settingsOpen: false,
    query: '',
    kit: null,
    conflict: false,
    dirty: false,
    pick: null,
    viewer: null,
    pwPrompt: null
  })
}

export async function updateConfig(patch: Partial<Config>): Promise<void> {
  const cur = getState().config
  if (!cur) return
  const next = await api.configSet({ ...cur, ...patch })
  setState({ config: next })
}

/** Confirmación con el diseño de la app (en vez del `confirm()` del navegador) */
export function ask(text: string, yes: string): Promise<boolean> {
  return new Promise((resolve) =>
    setState({
      question: {
        text,
        yes,
        resolve: (ok) => {
          setState({ question: null })
          resolve(ok)
        }
      }
    })
  )
}

/** Explorador de archivos propio. Devuelve la ruta elegida o `null` si se cancela. */
export function pickFile(req: Omit<PickReq, 'resolve'>): Promise<string | null> {
  return new Promise((resolve) => setState({ picker: { ...req, resolve } }))
}

export function promptPassword(reason: string): Promise<string | null> {
  return new Promise((resolve) =>
    setState({
      pwPrompt: {
        reason,
        resolve: (pw) => {
          setState({ pwPrompt: null })
          resolve(pw)
        }
      }
    })
  )
}
