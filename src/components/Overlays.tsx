import { useEffect, useMemo, useState } from 'react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { AlertTriangle, Download, Globe, Keyboard, Sparkles } from 'lucide-react'
import { api } from '../lib/api'
import { onLocked, pickFile, refreshInfo, setState, useStore } from '../lib/store'
import { attempt, fail, secured } from '../lib/actions'
import { changesFor } from '../lib/changelog'
import { byUse, search } from '../lib/search'
import { installUpdate, isPortable, RELEASES, useUpdate } from '../lib/update'
import { currentLang, useT } from '../i18n'
import { Avatar, Logo, Modal } from './ui'

export function Question(): React.JSX.Element | null {
  const t = useT()
  const q = useStore((s) => s.question)
  if (!q) return null
  return (
    <Modal onClose={() => q.resolve(false)}>
      <p style={{ color: 'var(--text)', fontSize: 15 }}>{q.text}</p>
      <div className="modal-actions">
        <button className="btn btn-ghost" autoFocus onClick={() => q.resolve(false)}>
          {t('common.cancel')}
        </button>
        <button className="btn btn-danger" onClick={() => q.resolve(true)}>
          {q.yes}
        </button>
      </div>
    </Modal>
  )
}

/** «¿Conectar este navegador?» con el código que también enseña la extensión */
export function PairModal(): React.JSX.Element | null {
  const t = useT()
  const p = useStore((s) => s.pairing)
  if (!p) return null
  const answer = (ok: boolean): void => {
    setState({ pairing: null })
    void api.bridgePairAnswer(ok).catch(fail)
  }
  return (
    <Modal onClose={() => answer(false)}>
      <div className="row" style={{ marginBottom: 6 }}>
        <Globe size={20} color="var(--accent)" />
        <h2 style={{ margin: 0 }}>{t('bridge.pairTitle', { b: p.browser })}</h2>
      </div>
      <p className="small muted">{t('bridge.pairText')}</p>
      <div className="pair-code" aria-label={p.code.split('').join(' ')}>
        {p.code.split('').map((d, i) => (
          <span key={i}>{d}</span>
        ))}
      </div>
      <div className="modal-actions">
        <button className="btn btn-ghost" autoFocus onClick={() => answer(false)}>
          {t('bridge.deny')}
        </button>
        <button className="btn btn-primary" onClick={() => answer(true)}>
          {t('bridge.allow')}
        </button>
      </div>
    </Modal>
  )
}

export function UpdateModal(): React.JSX.Element | null {
  const t = useT()
  const { update, version, progress } = useUpdate()
  if (!update) return null
  const portable = isPortable()
  const items = changesFor(version, currentLang())
  const close = (): void => useUpdate.setState({ update: null })
  return (
    <Modal onClose={progress === null ? close : undefined} wide>
      <div className="row" style={{ marginBottom: 6 }}>
        <Sparkles size={20} color="var(--accent)" />
        <h2 style={{ margin: 0 }}>{t('update.title', { v: version })}</h2>
      </div>
      {items.length > 0 ? <ChangeList items={items} /> : <p className="small">{useUpdate.getState().notes}</p>}
      {portable && <div className="callout">{t('update.portable')}</div>}
      <div className="modal-actions">
        <button className="btn btn-ghost" disabled={progress !== null} onClick={close}>
          {t('update.later')}
        </button>
        {portable ? (
          <button className="btn btn-primary" onClick={() => void openUrl(RELEASES)}>
            <Download size={16} /> {t('update.download')}
          </button>
        ) : (
          <button className="btn btn-primary" disabled={progress !== null} onClick={() => void installUpdate().catch(fail)}>
            {progress !== null ? t('update.downloading', { p: progress }) : t('update.install')}
          </button>
        )}
      </div>
    </Modal>
  )
}

function ChangeList({ items }: { items: { emoji: string; title: string; text: string }[] }): React.JSX.Element {
  return (
    <div className="col" style={{ gap: 10, margin: '14px 0' }}>
      {items.map((c, i) => (
        <div key={i} className="row" style={{ alignItems: 'flex-start' }}>
          <span style={{ fontSize: 18 }}>{c.emoji}</span>
          <div className="small" style={{ lineHeight: 1.5 }}>
            <b>{c.title}</b>
            {c.text && <span className="muted"> — {c.text}</span>}
          </div>
        </div>
      ))}
    </div>
  )
}

/** Novedades: tras actualizar (una vez) o desde Ajustes → Acerca de */
export function WhatsNew(): React.JSX.Element | null {
  const t = useT()
  const app = useStore((s) => s.app)
  const seen = useStore((s) => s.config?.seenVersion)
  const [open, setOpen] = useState(false)
  useEffect(() => {
    if (!app || seen === undefined) return
    if (seen && seen !== app.version && changesFor(app.version, currentLang()).length) setOpen(true)
    if (seen !== app.version) void api.seenVersion(app.version)
  }, [app, seen])
  useEffect(() => {
    const on = (): void => setOpen(true)
    window.addEventListener('poxi:whatsnew', on)
    return () => window.removeEventListener('poxi:whatsnew', on)
  }, [])
  if (!open || !app) return null
  return (
    <Modal onClose={() => setOpen(false)} wide>
      <div className="row" style={{ marginBottom: 6 }}>
        <Logo size={26} />
        <h2 style={{ margin: 0 }}>{t('whatsNew.title', { v: app.version })}</h2>
      </div>
      <ChangeList items={changesFor(app.version, currentLang())} />
      <div className="modal-actions">
        <button className="btn btn-primary" onClick={() => setOpen(false)}>
          {t('common.done')}
        </button>
      </div>
    </Modal>
  )
}

/** Atajo de auto-type sin una coincidencia clara: ¿qué entrada escribo? */
export function PickModal(): React.JSX.Element | null {
  const t = useT()
  const pick = useStore((s) => s.pick)
  const screen = useStore((s) => s.screen)
  const entries = useStore((s) => s.entries)
  const [q, setQ] = useState('')
  const [idx, setIdx] = useState(0)
  const list = useMemo(() => {
    if (!pick) return []
    const live = entries.filter((e) => !e.deleted && !e.archived && e.kind === 'login')
    const hit = q.trim() ? search(live, q) : byUse(live)
    const first = hit.filter((e) => pick.ids.includes(e.id))
    return [...first, ...hit.filter((e) => !pick.ids.includes(e.id))].slice(0, 50)
  }, [pick, entries, q])

  if (!pick || screen !== 'vault') return null
  const cancel = (): void => {
    setState({ pick: null })
    setQ('')
    void api.autotypeCancel()
  }
  const choose = async (id: string): Promise<void> => {
    setState({ pick: null })
    setQ('')
    await attempt(secured(id, () => api.autotypeEntry(id)))
  }
  return (
    <Modal onClose={cancel}>
      <div className="row" style={{ marginBottom: 4 }}>
        <Keyboard size={20} color="var(--accent)" />
        <h2 style={{ margin: 0 }}>{t('pick.title')}</h2>
      </div>
      <p className="small">{t('pick.in', { t: pick.title })}</p>
      <input
        className="input"
        autoFocus
        value={q}
        placeholder={t('common.search')}
        onChange={(e) => {
          setQ(e.target.value)
          setIdx(0)
        }}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault()
            const next = e.key === 'ArrowDown' ? Math.min(idx + 1, list.length - 1) : Math.max(idx - 1, 0)
            setIdx(next)
            document.querySelectorAll('.pick-list .row-entry')[next]?.scrollIntoView({ block: 'nearest' })
          }
          if (e.key === 'Enter' && list[idx]) void choose(list[idx].id)
        }}
      />
      <div className="pick-list">
        {list.map((e, i) => (
          <button key={e.id} className={`row-entry ${i === idx ? 'on' : ''}`} onMouseEnter={() => setIdx(i)} onClick={() => void choose(e.id)}>
            <Avatar name={e.title} icon={e.icon} />
            <span className="txt">
              <b>{e.title}</b>
              <small>{e.subtitle}</small>
            </span>
          </button>
        ))}
      </div>
    </Modal>
  )
}

/** El archivo cambió por fuera mientras estaba abierta */
export function Conflict(): React.JSX.Element | null {
  const t = useT()
  const open = useStore((s) => s.conflict)
  if (!open) return null
  const run = async (mode: 'reload' | 'overwrite' | 'copy'): Promise<void> => {
    let path: string | undefined
    if (mode === 'copy') {
      const p = await pickFile({ mode: 'save', title: t('conflict.copy'), exts: ['poxipass'], ext: 'poxipass' })
      if (!p) return
      path = p
    }
    try {
      await api.vaultResolveSave(mode, path)
      setState({ conflict: false })
      if (mode === 'reload') onLocked()
      else await refreshInfo()
    } catch (e) {
      fail(e)
    }
  }
  const opt = (mode: 'reload' | 'overwrite' | 'copy'): React.JSX.Element => (
    <button className="choice" onClick={() => void run(mode)}>
      <span>
        <b>{t(`conflict.${mode}`)}</b>
        <small>{t(`conflict.${mode}Hint`)}</small>
      </span>
    </button>
  )
  return (
    <Modal>
      <div className="row" style={{ marginBottom: 6 }}>
        <AlertTriangle size={20} color="var(--warning)" />
        <h2 style={{ margin: 0 }}>{t('conflict.title')}</h2>
      </div>
      <p>{t('conflict.lead')}</p>
      {opt('copy')}
      {opt('overwrite')}
      {opt('reload')}
    </Modal>
  )
}

export function SaveBanner(): React.JSX.Element | null {
  const t = useT()
  const failed = useStore((s) => s.info?.saveFailed)
  if (!failed) return null
  return (
    <div className="banner">
      <AlertTriangle size={16} color="var(--danger)" />
      <span className="spacer">{t('saveFailed')}</span>
      <button
        className="btn btn-sm"
        onClick={async () => {
          try {
            setState({ info: await api.vaultResolveSave('retry') })
          } catch (e) {
            fail(e)
          }
        }}
      >
        {t('common.retry')}
      </button>
    </div>
  )
}
