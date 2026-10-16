import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { api } from './lib/api'
import { enterVault, getState, onLocked, refreshEntries, refreshInfo, setState, toast, useStore } from './lib/store'
import { attempt, copyField, fail, leaveEditor, lockNow, trashEntry } from './lib/actions'
import { errorText, t, useT } from './i18n'
import { TitleBar, Toasts } from './components/TitleBar'
import { Sidebar } from './components/Sidebar'
import { EntryList, useVisible } from './components/EntryList'
import { EntryDetail } from './components/EntryDetail'
import { EntryEditor } from './components/EntryEditor'
import { BulkPanel } from './components/Bulk'
import { Conflict, PairModal, PickModal, Question, SaveBanner, UpdateModal, WhatsNew } from './components/Overlays'
import { FilePicker } from './components/FilePicker'
import { ContextMenu } from './components/ContextMenu'
import { PasswordPrompt, Viewer } from './components/Viewers'
import { Create, MustChange, Recover, Unlock, Welcome } from './pages/Start'
import { Generator } from './pages/Generator'
import { Health } from './pages/Health'
import { Settings } from './pages/Settings'

/** Atajos de la bóveda. Sin animaciones: se usan cientos de veces. */
function useVaultKeys(): void {
  const visible = useVisible()
  useEffect(() => {
    const onKey = async (e: KeyboardEvent): Promise<void> => {
      const s = getState()
      if (s.screen !== 'vault' || s.settingsOpen || document.querySelector('.modal-backdrop')) return
      if (document.querySelector('.dd.open, .ctx')) return
      const typing = /^(INPUT|TEXTAREA|SELECT)$/.test((document.activeElement as HTMLElement | null)?.tagName ?? '')
      const k = e.key.toLowerCase()
      if (!typing && !s.editing && e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey && e.key !== ' ') {
        const search = document.querySelector<HTMLInputElement>('.search input')
        if (search) {
          e.preventDefault()
          search.focus()
          setState({ query: s.query + e.key })
          return
        }
      }
      if (e.ctrlKey && k === 'l') {
        e.preventDefault()
        return lockNow()
      }
      if (e.ctrlKey && k === 'n') {
        e.preventDefault()
        const kind = s.section === 'card' || s.section === 'note' || s.section === 'identity' ? s.section : 'login'
        return leaveEditor({ editing: { id: null, kind }, selected: null })
      }
      if (typing || s.editing) return
      if (e.key === 'Escape' && s.multi.length) return setState({ multi: [] })
      if (e.ctrlKey && k === 'a') {
        e.preventDefault()
        return setState({ multi: visible.map((x) => x.id) })
      }
      const cur = s.selected ? s.entries.find((x) => x.id === s.selected) : undefined
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const i = visible.findIndex((x) => x.id === s.selected)
        const next = visible[Math.max(0, Math.min(visible.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))]
        if (next) {
          setState({ selected: next.id })
          document.querySelector(`[data-id="${next.id}"]`)?.scrollIntoView({ block: 'nearest' })
        }
        return
      }
      if (!cur) return
      if (e.ctrlKey && k === 'e' && !cur.deleted) {
        e.preventDefault()
        return setState({ editing: { id: cur.id, kind: cur.kind } })
      }
      if (e.key === 'Delete' && !cur.deleted) return trashEntry(cur.id)
      // Enter: copiar la contraseña de la entrada elegida
      if (e.key === 'Enter' && !e.ctrlKey && !e.altKey) {
        const v = await attempt(api.entryGet(cur.id))
        const f = v?.fields.find((x) => x.key === 'password' && !x.empty)
        if (f) await copyField(cur.id, f.id)
        return
      }
      if (e.ctrlKey && (k === 'c' || k === 'b') && !window.getSelection()?.toString()) {
        e.preventDefault()
        const v = await attempt(api.entryGet(cur.id))
        const f = v?.fields.find((x) => x.key === (k === 'c' ? 'password' : 'username') && !x.empty)
        if (f) await copyField(cur.id, f.id)
      }
    }
    const h = (e: KeyboardEvent): void => void onKey(e)
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [visible])
}

function Vault(): React.JSX.Element {
  const section = useStore((s) => s.section)
  const editing = useStore((s) => s.editing)
  const multi = useStore((s) => s.multi.length)
  useVaultKeys()
  const page = section === 'generator' || section === 'health'
  return (
    <div className={`vault ${page ? 'page' : ''}`}>
      <Sidebar />
      {page ? (
        <div className="page-body">{section === 'generator' ? <Generator /> : <Health />}</div>
      ) : (
        <>
          <EntryList />
          {editing ? (
            <EntryEditor key={editing.id ?? `new-${editing.kind}`} id={editing.id} kind={editing.kind} />
          ) : multi > 1 ? (
            <BulkPanel />
          ) : (
            <EntryDetail />
          )}
        </>
      )}
    </div>
  )
}

export function App(): React.JSX.Element {
  const screen = useStore((s) => s.screen)
  const settingsOpen = useStore((s) => s.settingsOpen)
  const lang = useStore((s) => s.config?.lang)
  useT()

  // Eventos de Rust
  useEffect(() => {
    const uns = [
      listen<string>('locked', (e) => {
        onLocked()
        toast(t(`toast.locked.${e.payload}`), 'info')
      }),
      listen<{ ids: string[]; title: string; locked: boolean }>('autotype-pick', (e) => {
        setState({ pick: e.payload })
        if (e.payload.locked) toast(t('pick.locked'), 'info')
      }),
      listen<number>('generated-copied', (e) => toast(t('toast.generatedCopied'), 'ok', e.payload || undefined)),
      listen<string>('hotkey-failed', (e) => toast(t('settings.hotkeyFailed', { k: e.payload }), 'err')),
      listen<number>('vault-synced', async (e) => {
        await refreshInfo()
        await refreshEntries()
        toast(t('toast.synced', { n: e.payload }), 'info')
      }),
      listen<number>('totp-copied', (e) => toast(t('toast.totpCopied'), 'ok', e.payload || undefined)),
      listen('vault-opened', async () => {
        if (getState().screen === 'vault') return
        const info = await api.vaultInfo()
        if (info) await enterVault(info)
      }),
      listen('autotype-error', (e) => toast(errorText(e.payload), 'err')),
      listen<{ browser: string; code: string }>('bridge-pair', (e) => setState({ pairing: e.payload })),
      listen('bridge-pair-end', () => setState({ pairing: null })),
      listen('bridge-saved', async () => {
        await refreshEntries()
        toast(t('bridge.saved'), 'ok')
      })
    ]
    // Los atajos que fallaron al arrancar (antes de que la interfaz escuchara)
    void api.hotkeyFailures().then((hs) => hs.forEach((k) => toast(t('settings.hotkeyFailed', { k }), 'err')))
    return () => uns.forEach((u) => void u.then((f) => f()))
  }, [])

  // Menú de la bandeja en el idioma de la app
  useEffect(() => {
    void api.trayLabels({ open: t('tray.open'), generate: t('tray.generate'), lock: t('tray.lock'), quit: t('tray.quit') }).catch(fail)
  }, [lang])

  // Al desbloquear con una petición de auto-type pendiente, la lista se pide otra vez para el selector
  useEffect(() => {
    if (screen === 'vault' && getState().pick?.locked) setState((s) => ({ pick: s.pick && { ...s.pick, locked: false } }))
  }, [screen])

  return (
    <>
      <div className="ambient" />
      <div className="app">
        <TitleBar />
        <div className="screen-wrap">
          <SaveBanner />
          {screen === 'welcome' && <Welcome />}
          {screen === 'unlock' && <Unlock />}
          {screen === 'create' && <Create />}
          {screen === 'recover' && <Recover />}
          {screen === 'mustChange' && <MustChange />}
          {screen === 'generatorOnly' && (
            <div className="page-body">
              <Generator standalone />
            </div>
          )}
          {screen === 'vault' && <Vault />}
        </div>
      </div>
      {settingsOpen && <Settings />}
      <PickModal />
      <Conflict />
      <UpdateModal />
      <WhatsNew />
      <FilePicker />
      <ContextMenu />
      <Viewer />
      <PasswordPrompt />
      <Question />
      <PairModal />
      <Toasts />
    </>
  )
}
