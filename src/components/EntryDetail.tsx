import { useEffect, useRef, useState } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  Lock,
  QrCode,
  Type,
  Archive,
  ArchiveRestore,
  Copy,
  Download,
  ExternalLink,
  Eye,
  EyeOff,
  History,
  ImageDown,
  KeySquare,
  Paperclip,
  Pencil,
  ShieldAlert,
  ShieldCheck,
  Star,
  Trash2,
  X
} from 'lucide-react'
import { api, isAppError, type EntryView, type FieldView } from '../lib/api'
import { ask, pickFile, refreshEntries, setState, toast, useStore } from '../lib/store'
import { attempt, copyField, secured, trashEntry } from '../lib/actions'
import { Markdown } from '../lib/markdown'
import { ago, bytes, date } from '../lib/format'
import { useT } from '../i18n'
import { Avatar, StrengthBar } from './ui'
import { bulk } from './Bulk'

const REVEAL_SECS = 30

export function fieldLabel(f: { key: string; label: string }, t: (k: string) => string): string {
  return f.label || (f.key ? t(`field.${f.key}`) : t('field.custom'))
}

function safeUrl(u: string): string | null {
  const s = u.trim()
  if (!s) return null
  const full = /^[a-z][a-z0-9+.-]*:\/\//i.test(s) ? s : `https://${s}`
  return /^https?:\/\//i.test(full) ? full : null
}

function Totp({ entry, field }: { entry: string; field: string }): React.JSX.Element {
  const t = useT()
  const [c, setC] = useState<{ code: string; remaining: number; period: number } | null>(null)
  const [locked, setLocked] = useState(false)
  const [round, setRound] = useState(0)
  useEffect(() => {
    let live = true
    const tick = (): void => {
      void api
        .totpCode(entry, field)
        .then((r) => {
          if (!live) return
          setC(r)
          setLocked(false)
        })
        .catch((e: unknown) => {
          if (!live) return
          setC(null)
          setLocked(isAppError(e) && e.code === 'entry.protected')
        })
    }
    tick()
    const id = setInterval(tick, 1000)
    return () => {
      live = false
      clearInterval(id)
    }
  }, [entry, field, round])
  if (locked)
    return (
      <button className="btn btn-sm" onClick={async () => (await attempt(secured(entry, () => api.totpCode(entry, field)))) && setRound((r) => r + 1)}>
        <Lock size={14} /> {t('protect.show')}
      </button>
    )
  if (!c) return <span className="muted">—</span>
  const r = 9
  const len = 2 * Math.PI * r
  return (
    <span className="totp">
      <span className="code num">
        {c.code.slice(0, Math.ceil(c.code.length / 2))} {c.code.slice(Math.ceil(c.code.length / 2))}
      </span>
      <svg className={`ring ${c.remaining <= 5 ? 'low' : ''}`} viewBox="0 0 22 22">
        <circle className="bg" cx="11" cy="11" r={r} />
        <circle className="fg" cx="11" cy="11" r={r} strokeDasharray={len} strokeDashoffset={len * (1 - c.remaining / c.period)} />
      </svg>
      <span className="muted small num">{c.remaining}s</span>
    </span>
  )
}

function FieldRow({ entry, f, revealed, onReveal }: { entry: string; f: FieldView; revealed?: string; onReveal: (v: string | null) => void }): React.JSX.Element | null {
  const t = useT()
  if (f.empty) return null
  const isTotp = f.kind === 'totp'
  const value = f.secret ? revealed : (f.value ?? '')
  const url = f.kind === 'url' && value ? safeUrl(value) : null
  return (
    <div className="fieldrow">
      <div className="body">
        <div className="k">{fieldLabel(f, t)}</div>
        {isTotp ? (
          <Totp entry={entry} field={f.id} />
        ) : value !== undefined ? (
          <div className={`v ${f.kind === 'password' ? 'mono' : ''}`}>
            {url ? (
              <a
                href={url}
                onClick={(e) => {
                  e.preventDefault()
                  void openUrl(url)
                }}
              >
                {value}
              </a>
            ) : f.kind === 'multiline' ? (
              <Markdown text={value} />
            ) : (
              value
            )}
          </div>
        ) : (
          <div className="v dots">••••••••••••</div>
        )}
      </div>
      <div className="acts">
        {f.secret && !isTotp && (
          <button
            className="icon-btn"
            data-tip={revealed !== undefined ? t('common.hide') : t('common.show')}
            aria-label={revealed !== undefined ? t('common.hide') : t('common.show')}
            onClick={async () => {
              if (revealed !== undefined) return onReveal(null)
              const v = await attempt(secured(entry, () => api.revealField(entry, f.id)))
              if (v !== undefined) onReveal(v)
            }}
          >
            {revealed !== undefined ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        )}
        {f.secret && f.kind === 'password' && (
          <button
            className="icon-btn"
            data-tip={t('viewer.large')}
            aria-label={t('viewer.large')}
            onClick={async () => {
              const v = await attempt(secured(entry, () => api.revealField(entry, f.id)))
              if (v !== undefined) setState({ viewer: { type: 'large', title: fieldLabel(f, t), text: v } })
            }}
          >
            <Type size={16} />
          </button>
        )}
        {url && (
          <button className="icon-btn" data-tip={t('detail.openUrl')} aria-label={t('detail.openUrl')} onClick={() => void openUrl(url)}>
            <ExternalLink size={16} />
          </button>
        )}
        <button className="icon-btn" data-tip={t('common.copy')} aria-label={t('common.copy')} onClick={() => void copyField(entry, f.id)}>
          <Copy size={16} />
        </button>
      </div>
    </div>
  )
}

export function EntryDetail(): React.JSX.Element {
  const t = useT()
  const selected = useStore((s) => s.selected)
  const rev = useStore((s) => s.rev)
  const favicons = useStore((s) => s.config?.favicons)
  const folders = useStore((s) => s.info?.folders ?? [])
  const [e, setE] = useState<EntryView | null>(null)
  const [revealed, setRevealed] = useState<Record<string, string>>({})
  const [dragging, setDragging] = useState(false)
  const timers = useRef<Record<string, number>>({})

  // Al cambiar de entrada (o al bloquear) se olvida todo lo que se había mostrado
  useEffect(() => {
    setRevealed({})
    Object.values(timers.current).forEach(clearTimeout)
    timers.current = {}
  }, [selected])

  useEffect(() => {
    if (!selected) {
      setE(null)
      return
    }
    let live = true
    void api
      .entryGet(selected)
      .then((v) => live && setE(v))
      .catch(() => live && setE(null))
    return () => {
      live = false
    }
  }, [selected, rev])

  useEffect(() => () => Object.values(timers.current).forEach(clearTimeout), [])

  // Arrastrar archivos encima: adjuntar
  useEffect(() => {
    if (!e || e.deleted) return
    const id = e.id
    const un = getCurrentWebview().onDragDropEvent(async (ev) => {
      if (ev.payload.type === 'over' || ev.payload.type === 'enter') setDragging(true)
      else if (ev.payload.type === 'leave') setDragging(false)
      else if (ev.payload.type === 'drop') {
        setDragging(false)
        for (const p of ev.payload.paths) {
          const v = await attempt(api.attachmentAdd(id, p))
          if (v) setE(v)
        }
      }
    })
    return () => {
      void un.then((f) => f())
    }
  }, [e?.id, e?.deleted])

  const reveal = (fid: string, v: string | null, long = false): void => {
    clearTimeout(timers.current[fid])
    setRevealed((r) => {
      const n = { ...r }
      if (v === null) delete n[fid]
      else n[fid] = v
      return n
    })
    // Un texto largo (notas) no se oculta mientras lo lees
    if (v !== null && !long) timers.current[fid] = window.setTimeout(() => reveal(fid, null), REVEAL_SECS * 1000)
  }

  if (!e) {
    return (
      <section className="detail">
        <div className="empty" style={{ height: '100%' }}>
          <KeySquare size={40} />
          <b>{t('detail.none')}</b>
          <span className="small">{t('detail.noneHint')}</span>
        </div>
      </section>
    )
  }

  const after = async (p: Promise<unknown>): Promise<void> => {
    await attempt(p)
    await refreshEntries()
  }
  const folder = folders.find((f) => f.id === e.folder)
  const hasPw = e.fields.some((f) => f.key === 'password' && !f.empty)

  return (
    <section className="detail">
      {dragging && <div className="drop-zone">{t('detail.dropHint')}</div>}
      <div className="detail-inner">
        <div className="detail-head">
          <Avatar name={e.title} icon={e.icon} lg />
          <div style={{ flex: 1, minWidth: 0 }}>
            <h1 className="selectable">{e.title}</h1>
            <div className="kind">
              {t(`kind.${e.kind}`)}
              {folder && ` · ${folder.name}`}
              {e.deleted && ` · ${t('detail.inTrash')}`}
              {e.archived && ` · ${t('detail.archived')}`}
              {e.protected && (
                <>
                  {' · '}
                  <Lock size={11} style={{ verticalAlign: -1 }} /> {t('protect.badge')}
                </>
              )}
            </div>
          </div>
          {!e.deleted ? (
            <>
              <button
                className={`icon-btn ${e.favorite ? 'on' : ''}`}
                data-tip={t('detail.favorite')}
                aria-label={t('detail.favorite')}
                onClick={() => void after(api.entryFavorite(e.id, !e.favorite))}
              >
                <Star size={18} fill={e.favorite ? 'currentColor' : 'none'} />
              </button>
              {favicons && e.url && (
                <button
                  className="icon-btn"
                  data-tip={t('detail.fetchIcon')}
                  aria-label={t('detail.fetchIcon')}
                  onClick={async () => {
                    const v = await attempt(api.iconFetch(e.id))
                    if (v) {
                      setE(v)
                      void refreshEntries()
                    }
                  }}
                >
                  <ImageDown size={17} />
                </button>
              )}
              {e.kind === 'wifi' && (
                <button
                  className="icon-btn"
                  data-tip={t('viewer.wifiQr')}
                  aria-label={t('viewer.wifiQr')}
                  onClick={async () => {
                    const svg = await attempt(secured(e.id, () => api.fieldQr(e.id, null)))
                    if (svg) setState({ viewer: { type: 'qr', title: t('viewer.wifiQr'), svg } })
                  }}
                >
                  <QrCode size={17} />
                </button>
              )}
              <button className="btn btn-sm" onClick={() => setState({ editing: { id: e.id, kind: e.kind } })} data-tip="Ctrl+E">
                <Pencil size={14} /> {t('common.edit')}
              </button>
              <button
                className="icon-btn"
                data-tip={e.archived ? t('detail.unarchive') : t('detail.archive')}
                aria-label={e.archived ? t('detail.unarchive') : t('detail.archive')}
                onClick={() => void bulk([e.id], e.archived ? 'unarchive' : 'archive')}
              >
                {e.archived ? <ArchiveRestore size={17} /> : <Archive size={17} />}
              </button>
              <button className="icon-btn" data-tip={t('detail.moveToTrash')} aria-label={t('detail.moveToTrash')} onClick={() => void trashEntry(e.id)}>
                <Trash2 size={17} />
              </button>
            </>
          ) : (
            <>
              <button className="btn btn-sm" onClick={() => void after(api.entryRestore(e.id))}>
                <ArchiveRestore size={14} /> {t('detail.restoreFromTrash')}
              </button>
              <button
                className="btn btn-sm btn-danger"
                onClick={async () => {
                  if (await ask(t('detail.purgeConfirm', { t: e.title }), t('detail.purge'))) await after(api.entryPurge(e.id))
                }}
              >
                {t('detail.purge')}
              </button>
            </>
          )}
        </div>

        <div className="fields">
          {e.fields.map((f) => (
            <FieldRow key={f.id} entry={e.id} f={f} revealed={revealed[f.id]} onReveal={(v) => reveal(f.id, v, f.kind === 'multiline')} />
          ))}
        </div>

        {hasPw && (
          <>
            <StrengthBar s={e.strength} />
            <div className="chips">
              {e.breach !== null &&
                (e.breach > 0 ? (
                  <span className="chip bad">
                    <ShieldAlert size={13} /> {t('detail.breached', { n: e.breach.toLocaleString() })}
                  </span>
                ) : (
                  <span className="chip good">
                    <ShieldCheck size={13} /> {t('detail.notBreached')}
                  </span>
                ))}
            </div>
          </>
        )}

        {e.tags.length > 0 && (
          <div className="chips">
            {e.tags.map((tag) => (
              <span key={tag} className="chip">
                #{tag}
              </span>
            ))}
          </div>
        )}

        <div className="section-title">
          <Paperclip size={14} /> {t('detail.attachments')}
          <span className="spacer" />
          {!e.deleted && (
            <button
              className="btn btn-sm btn-ghost"
              onClick={async () => {
                const p = await pickFile({ mode: 'open', title: t('picker.attach'), exts: [] })
                if (p) {
                  const v = await attempt(api.attachmentAdd(e.id, p))
                  if (v) setE(v)
                }
              }}
            >
              {t('detail.addAttachment')}
            </button>
          )}
        </div>
        {e.attachments.length === 0 && <div className="hint" style={{ marginTop: 0 }}>{t('detail.dropEmpty')}</div>}
        {e.attachments.map((a) => (
          <div className="att" key={a.id}>
            <Paperclip size={15} className="muted" />
            <span className="name">{a.name}</span>
            <span className="muted small num">{bytes(a.size)}</span>
            {(a.mime.startsWith('image/') || a.mime === 'text/plain' || a.name.toLowerCase().endsWith('.md')) && (
              <button
                className="icon-btn"
                data-tip={t('viewer.preview')}
                aria-label={t('viewer.preview')}
                onClick={async () => {
                  const p = await attempt(secured(e.id, () => api.attachmentPreview(e.id, a.id)))
                  if (!p) return
                  setState({
                    viewer: p.mime.startsWith('image/') ? { type: 'image', title: a.name, src: `data:${p.mime};base64,${p.data}` } : { type: 'text', title: a.name, text: p.data }
                  })
                }}
              >
                <Eye size={16} />
              </button>
            )}
            <button
              className="icon-btn"
              data-tip={t('common.save')}
              aria-label={t('common.save')}
              onClick={async () => {
                const dot = a.name.lastIndexOf('.')
                const dest = await pickFile({ mode: 'save', title: t('picker.saveAttachment'), exts: [], name: a.name, ext: dot > 0 ? a.name.slice(dot + 1) : '' })
                if (dest && (await attempt(secured(e.id, () => api.attachmentSave(e.id, a.id, dest)))) !== undefined) toast(t('detail.attachmentSaved'))
              }}
            >
              <Download size={16} />
            </button>
            {!e.deleted && (
              <button
                className="icon-btn"
                data-tip={t('editor.remove')}
                aria-label={t('editor.remove')}
                onClick={async () => {
                  if (!(await ask(t('detail.deleteAttachment', { n: a.name }), t('editor.remove')))) return
                  const v = await attempt(api.attachmentDelete(e.id, a.id))
                  if (v) setE(v)
                }}
              >
                <X size={16} />
              </button>
            )}
          </div>
        ))}

        {e.history.length > 0 && (
          <>
            <div className="section-title">
              <History size={14} /> {t('detail.history')}
            </div>
            {e.history.map((h) => (
              <div className="hist" key={h.index}>
                <span className="small" style={{ flex: 1 }}>
                  <b>{h.title}</b> <span className="muted">· {ago(h.at)}</span>
                </span>
                {!e.deleted && (
                  <button
                    className="btn btn-sm btn-ghost"
                    onClick={async () => {
                      const v = await attempt(api.historyRestore(e.id, h.index))
                      if (v) {
                        setE(v)
                        void refreshEntries()
                      }
                    }}
                  >
                    {t('detail.restore')}
                  </button>
                )}
              </div>
            ))}
          </>
        )}

        <div className="meta-line">
          <span>{t('detail.created', { d: date(e.created) })}</span>
          <span>{t('detail.modified', { d: ago(e.modified) })}</span>
        </div>
      </div>
    </section>
  )
}
