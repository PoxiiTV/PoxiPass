import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { emit, listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Fingerprint, Lock, Search } from 'lucide-react'
import { api, type EntrySummary, type Recent } from '../lib/api'
import { setState } from '../lib/store'
import { applyTheme } from '../lib/theme'
import { byUse, search } from '../lib/search'
import { errorText, useT } from '../i18n'
import { Avatar, PasswordInput } from '../components/ui'
import { PasswordPrompt } from '../components/Viewers'
import { secured } from '../lib/actions'

const win = getCurrentWindow()
const MAX = 8

/** Acceso rápido: barra flotante (atajo global) para buscar y escribir o copiar sin abrir la ventana principal. */
export function Quick(): React.JSX.Element {
  const t = useT()
  const [entries, setEntries] = useState<EntrySummary[] | null>(null)
  const [recent, setRecent] = useState<Recent | null>(null)
  const [q, setQ] = useState('')
  const [idx, setIdx] = useState(0)
  const [pw, setPw] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  /** Aviso en la propia barra (no hay toasts aquí: la ventana se esconde enseguida) */
  const [note, setNote] = useState('')
  const input = useRef<HTMLInputElement>(null)
  /** Con Windows Hello delante la barra pierde el foco: entonces no se esconde */
  const unlocking = useRef(false)

  const load = useCallback(async () => {
    const [cfg, info] = await Promise.all([api.configGet(), api.vaultInfo()])
    setState({ config: cfg })
    applyTheme(cfg)
    setQ('')
    setIdx(0)
    setError('')
    setNote('')
    if (info && !info.mustChangePassword) {
      setEntries((await api.entriesList()).filter((e) => !e.deleted && !e.archived))
    } else {
      setEntries(null)
      setRecent((await api.recentList()).find((r) => r.exists) ?? null)
    }
    requestAnimationFrame(() => input.current?.focus())
  }, [])

  const hide = useCallback((refocus = true) => {
    setPw('')
    void win.hide()
    if (refocus) void api.autotypeCancel()
  }, [])

  useEffect(() => {
    void load()
    const un = [
      listen('quick-show', () => void load()),
      listen('locked', () => setEntries(null)),
      win.onFocusChanged(({ payload }) => {
        if (!payload && !unlocking.current) hide(false)
      })
    ]
    return () => un.forEach((u) => void u.then((f) => f()))
  }, [load, hide])

  const list = useMemo(() => (entries ? (q.trim() ? search(entries, q) : byUse(entries)).slice(0, MAX) : []), [entries, q])

  const unlock = async (hello: boolean): Promise<void> => {
    if (!recent || busy || (!hello && !pw)) return
    setBusy(true)
    unlocking.current = hello
    setError('')
    try {
      await (hello ? api.vaultOpenHello(recent.path) : api.vaultOpen(recent.path, pw))
      setPw('')
      await emit('vault-opened')
      await load()
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
      unlocking.current = false
      void win.setFocus()
    }
  }

  /** Copia un campo de la entrada y devuelve el foco a donde estabas */
  const copy = async (e: EntrySummary, key: 'password' | 'username' | 'totp'): Promise<void> => {
    try {
      const v = await api.entryGet(e.id)
      const f = v.fields.find((x) => x.key === key && !x.empty)
      if (!f) return setNote(t(`quick.no.${key}`))
      await secured(e.id, () => api.copyField(e.id, f.id))
      hide()
    } catch (err) {
      setNote(errorText(err))
    }
  }

  const type = (e: EntrySummary): void => {
    secured(e.id, () => api.autotypeEntry(e.id)).catch((err) => setNote(errorText(err)))
  }

  const onKey = (ev: React.KeyboardEvent): void => {
    const e = list[idx]
    const k = ev.key.toLowerCase()
    if (ev.key === 'Escape') return hide()
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      ev.preventDefault()
      setIdx((i) => (list.length ? (i + (ev.key === 'ArrowDown' ? 1 : -1) + list.length) % list.length : 0))
      return
    }
    if (!e) return
    if (ev.key === 'Enter') {
      ev.preventDefault()
      type(e)
    } else if (ev.ctrlKey && (k === 'c' || k === 'b' || k === 't')) {
      ev.preventDefault()
      void copy(e, k === 'c' ? 'password' : k === 'b' ? 'username' : 'totp')
    }
  }

  return (
    <div className="quick-shell">
      <PasswordPrompt />
      <div className="quick">
        {entries ? (
          <>
            <div className="quick-search">
              <Search size={20} />
              <input
                ref={input}
                value={q}
                spellCheck={false}
                placeholder={t('quick.search')}
                onChange={(e) => {
                  setQ(e.target.value)
                  setIdx(0)
                  setNote('')
                }}
                onKeyDown={onKey}
              />
            </div>
            <div className="quick-list">
              {list.length === 0 && <div className="empty small">{t('quick.none')}</div>}
              {list.map((e, i) => (
                <button
                  key={e.id}
                  className={`row-entry ${i === idx ? 'on' : ''}`}
                  onMouseEnter={() => setIdx(i)}
                  onClick={() => type(e)}
                >
                  <Avatar name={e.title} icon={e.icon} />
                  <span className="txt">
                    <b>{e.title}</b>
                    <small>{e.subtitle || e.url}</small>
                  </span>
                </button>
              ))}
            </div>
            {note && <div className="quick-note">{note}</div>}
            <div className="quick-foot">
              <span>
                <kbd>Enter</kbd> {t('quick.type')}
              </span>
              <span>
                <kbd>Ctrl C</kbd> {t('quick.password')}
              </span>
              <span>
                <kbd>Ctrl B</kbd> {t('quick.user')}
              </span>
              <span>
                <kbd>Ctrl T</kbd> {t('quick.code')}
              </span>
              <span className="spacer" />
              <span>
                <kbd>Esc</kbd> {t('quick.close')}
              </span>
            </div>
          </>
        ) : (
          <div className="quick-locked" onKeyDown={(e) => e.key === 'Escape' && hide()}>
            <Lock size={22} color="var(--accent)" />
            <b>{recent?.name ?? t('quick.locked')}</b>
            {recent && (
              <div style={{ width: '100%', maxWidth: 380 }}>
                <PasswordInput value={pw} onChange={setPw} autoFocus bad={!!error} onEnter={() => void unlock(false)} />
                {error && <div className="hint danger-text">{error}</div>}
                {recent.hello && (
                  <button className="btn btn-block" style={{ marginTop: 10 }} disabled={busy} onClick={() => void unlock(true)}>
                    <Fingerprint size={17} /> {t('hello.unlock')}
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
