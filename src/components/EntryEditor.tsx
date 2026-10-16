import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Eye, EyeOff, Plus, QrCode, Wand2, X } from 'lucide-react'
import { api, type FieldKind, type Kind } from '../lib/api'
import { ask, getState, pickFile, refreshEntries, setState, useStore } from '../lib/store'
import { attempt, fail, leaveEditor, secured } from '../lib/actions'
import { KIND_ICON, KINDS, PRIMARY, TEMPLATES } from '../lib/templates'
import { fullUrl, looksLikeUrl, siteTitle } from '../lib/format'
import { useT } from '../i18n'
import { Avatar, Modal, StrengthBar, useStrength } from './ui'
import { Select } from './controls'
import { fieldLabel } from './EntryDetail'

interface Draft {
  uid: number
  id: string | null
  key: string
  label: string
  value: string
  secret: boolean
  kind: FieldKind
  show?: boolean
  /** Contraseña generada al abrir y aún sin tocar: al entrar en el campo se selecciona entera (pegar la sustituye) */
  fresh?: boolean
}

let uid = 0
const blank = (kind: Kind): Draft[] => TEMPLATES[kind].map((f) => ({ uid: ++uid, id: null, key: f.key, label: '', value: '', secret: f.secret, kind: f.kind }))

function PasswordStrength({ value }: { value: string }): React.JSX.Element | null {
  const s = useStrength(value)
  return value ? <StrengthBar s={s} /> : null
}

function QrModal({ onClose, onFound }: { onClose: () => void; onFound: (uri: string) => void }): React.JSX.Element {
  const t = useT()
  useEffect(() => {
    const onPaste = (e: ClipboardEvent): void => {
      const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith('image/'))
      const file = item?.getAsFile()
      if (!file) return
      e.preventDefault()
      const r = new FileReader()
      r.onload = async () => {
        try {
          onFound(await api.totpFromImage(String(r.result)))
        } catch (err) {
          fail(err)
        }
      }
      r.readAsDataURL(file)
    }
    window.addEventListener('paste', onPaste)
    return () => window.removeEventListener('paste', onPaste)
  }, [onFound])
  return (
    <Modal onClose={onClose}>
      <h2>{t('editor.readQr')}</h2>
      <p>{t('editor.qrHint')}</p>
      <div className="callout">
        <QrCode size={18} /> {t('editor.qrPaste')}
      </div>
      <div className="modal-actions">
        <button className="btn btn-ghost" onClick={onClose}>
          {t('common.cancel')}
        </button>
        <button
          className="btn btn-primary"
          onClick={async () => {
            const p = await pickFile({ mode: 'open', title: t('picker.qr'), exts: ['png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp'] })
            if (!p) return
            const uri = await attempt(api.totpFromFile(p))
            if (uri) onFound(uri)
          }}
        >
          {t('editor.qrFile')}
        </button>
      </div>
    </Modal>
  )
}

export function EntryEditor({ id, kind: initialKind }: { id: string | null; kind: Kind }): React.JSX.Element {
  const t = useT()
  const folders = useStore((s) => s.info?.folders ?? [])
  const section = useStore((s) => s.section)
  const [kind, setKind] = useState<Kind>(initialKind)
  const [title, setTitle] = useState('')
  /** Alta de un inicio de sesión: «Web o nombre» en un solo campo */
  const [head, setHead] = useState('')
  const [folder, setFolder] = useState<string | null>(section.startsWith('folder:') ? section.slice(7) : null)
  const [tags, setTags] = useState('')
  const [favorite, setFavorite] = useState(section === 'favorites')
  const [autotype, setAutotype] = useState('')
  const [prot, setProt] = useState(false)
  const [fields, setFields] = useState<Draft[]>(() => (id ? [] : blank(initialKind)))
  const [more, setMore] = useState(false)
  const [loading, setLoading] = useState(!!id)
  const [saving, setSaving] = useState(false)
  const [qrFor, setQrFor] = useState<number | null>(null)
  const quick = !id && kind === 'login'
  const generated = useRef(false)
  const snap = (): string =>
    JSON.stringify([kind, title, head, folder, tags, favorite, autotype, prot, fields.map((f) => [f.key, f.label, f.fresh ? '' : f.value, f.secret])])
  const baseline = useRef<string | null>(null)
  useEffect(() => {
    if (loading) return
    if (baseline.current === null) baseline.current = snap()
    setState({ dirty: snap() !== baseline.current })
  })
  useEffect(() => () => setState({ dirty: false }), [])

  // Editar: se cargan también los secretos (los vas a ver y tocar), y se olvidan al cerrar
  useEffect(() => {
    if (!id) return
    let live = true
    void (async () => {
      try {
        const e = await api.entryGet(id)
        const drafts: Draft[] = []
        for (const f of e.fields) {
          const value = f.secret ? (f.empty ? '' : await secured(id, () => api.revealField(id, f.id))) : (f.value ?? '')
          drafts.push({ uid: ++uid, id: f.id, key: f.key, label: f.label, value, secret: f.secret, kind: f.kind })
        }
        if (!live) return
        setKind(e.kind)
        setTitle(e.title)
        setFolder(e.folder)
        setTags(e.tags.join(', '))
        setFavorite(e.favorite)
        setAutotype(e.autotype ?? '')
        setProt(e.protected)
        setFields(drafts)
        setLoading(false)
      } catch (err) {
        fail(err)
        setState({ editing: null })
      }
    })()
    return () => {
      live = false
    }
  }, [id])

  const patch = (u: number, p: Partial<Draft>): void => setFields((fs) => fs.map((f) => (f.uid === u ? { ...f, ...p } : f)))

  const generate = async (u: number, fresh = false): Promise<void> => {
    const cfg = getState().config
    if (!cfg) return
    const g = await attempt(api.generate(cfg.generator))
    if (g) patch(u, { value: g.value, show: true, fresh })
  }

  // Entrada nueva con contraseña: ya viene generada (un clic menos)
  useEffect(() => {
    if (id || generated.current) return
    const pw = fields.find((f) => f.key === 'password' && !f.value)
    if (pw) {
      generated.current = true
      void generate(pw.uid, true)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fields, id])

  const save = async (): Promise<void> => {
    if (saving || loading) return
    setSaving(true)
    let finalTitle = title.trim()
    let list = fields
    if (quick && head.trim()) {
      if (looksLikeUrl(head)) {
        finalTitle = siteTitle(head)
        list = list.map((f) => (f.key === 'url' ? { ...f, value: fullUrl(head) } : f))
      } else {
        finalTitle = head.trim()
      }
    }
    try {
      const v = await api.entrySave({
        id,
        kind,
        title: finalTitle || t('common.untitled'),
        folder,
        tags: tags
          .split(',')
          .map((x) => x.trim())
          .filter(Boolean),
        favorite,
        autotype: autotype.trim() || null,
        protected: prot,
        fields: list
          .filter((f) => f.key || f.label.trim() || f.value)
          .map((f) => ({ id: f.id, key: f.key, label: f.label.trim(), value: f.value, secret: f.secret, kind: f.kind }))
      })
      setFields([])
      setState({ dirty: false })
      await refreshEntries()
      setState({ editing: null, selected: v.id, section: getState().section === 'trash' ? 'all' : getState().section })
    } catch (e) {
      fail(e)
    } finally {
      setSaving(false)
    }
  }

  // Ctrl+S guarda, Esc cancela
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (document.querySelector('.modal-backdrop')) return
      if (e.ctrlKey && e.key === 's') {
        e.preventDefault()
        void save()
      } else if (e.key === 'Escape') {
        void leaveEditor()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  if (loading) return <section className="detail" />


  // Arriba: lo principal del tipo y, al editar, todo lo que ya tiene datos. El resto, plegado.
  const primary = (f: Draft): boolean =>
    (!!f.key && PRIMARY[kind].includes(f.key)) || (!!id && (!!f.value || !f.key)) || (quick && f.key === 'url' && !hiddenUrl(f))
  const hiddenUrl = (f: Draft): boolean => quick && f.key === 'url' && (looksLikeUrl(head) || !head.trim())
  const top = fields.filter((f) => primary(f) && !hiddenUrl(f))
  const rest = fields.filter((f) => !primary(f) && !hiddenUrl(f))
  const enter = (e: React.KeyboardEvent): void => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      void save()
    }
  }
  const preview = quick ? (looksLikeUrl(head) ? siteTitle(head) : head) : title

  const field = (f: Draft): React.JSX.Element => (
    <div className="fieldedit" key={f.uid}>
      <div>
        {f.key ? (
          <label className="label">{fieldLabel(f, t)}</label>
        ) : (
          <input
            className="input small"
            style={{ height: 30, marginBottom: 6 }}
            value={f.label}
            placeholder={t('editor.fieldName')}
            maxLength={100}
            onChange={(e) => patch(f.uid, { label: e.target.value })}
          />
        )}
        {f.kind === 'multiline' ? (
          <textarea className="textarea" value={f.value} onChange={(e) => patch(f.uid, { value: e.target.value })} />
        ) : (
          <div className="input-wrap">
            <input
              className={`input ${f.kind === 'password' || f.kind === 'totp' ? 'mono' : ''}`}
              style={f.kind === 'password' ? { paddingRight: 140 } : undefined}
              type={f.secret && !f.show ? 'password' : 'text'}
              value={f.value}
              spellCheck={false}
              autoComplete="off"
              onFocus={(e) => f.fresh && e.target.select()}
              onKeyDown={enter}
              onChange={(e) => patch(f.uid, { value: e.target.value, fresh: false })}
            />
            <div className="actions">
              {f.kind === 'password' && (
                <button type="button" className="btn btn-sm btn-ghost" onClick={() => void generate(f.uid)}>
                  <Wand2 size={14} /> {t('editor.generate')}
                </button>
              )}
              {f.kind === 'totp' && (
                <button type="button" className="icon-btn" data-tip={t('editor.readQr')} aria-label={t('editor.readQr')} onClick={() => setQrFor(f.uid)}>
                  <QrCode size={16} />
                </button>
              )}
              {f.secret && (
                <button type="button" className="icon-btn" onClick={() => patch(f.uid, { show: !f.show })} aria-label={f.show ? t('common.hide') : t('common.show')}>
                  {f.show ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              )}
            </div>
          </div>
        )}
        {f.key === 'password' && <PasswordStrength value={f.value} />}
      </div>
      {!f.key ? (
        <div className="col" style={{ paddingTop: 36, gap: 2 }}>
          <button
            className={`icon-btn ${f.secret ? 'on' : ''}`}
            data-tip={t('editor.secret')}
            aria-label={t('editor.secret')}
            onClick={() => patch(f.uid, { secret: !f.secret, kind: !f.secret ? 'password' : 'text' })}
          >
            <EyeOff size={15} />
          </button>
          <button className="icon-btn" aria-label={t('editor.remove')} onClick={() => setFields((fs) => fs.filter((x) => x.uid !== f.uid))}>
            <X size={15} />
          </button>
        </div>
      ) : (
        <span />
      )}
    </div>
  )

  return (
    <section className="detail editor">
      <div className="detail-inner">
        <h1 style={{ fontSize: 22, fontWeight: 800, margin: '0 0 18px' }}>{id ? t('editor.editTitle') : t('editor.newTitle')}</h1>
        {!id && (
          <div className="kind-pick">
            {KINDS.map((k) => {
              const Icon = KIND_ICON[k]
              return (
                <button
                  key={k}
                  className={kind === k ? 'on' : ''}
                  onClick={async () => {
                    if (k === kind) return
                    const typed = fields.some((f) => f.value && !f.fresh) || !!head.trim() || !!title.trim()
                    if (typed && !(await ask(t('editor.changeKind'), t('editor.discardYes')))) return
                    setKind(k)
                    generated.current = false
                    setFields(blank(k))
                  }}
                >
                  <Icon size={20} />
                  {t(`kind.${k}`)}
                </button>
              )
            })}
          </div>
        )}

        <div className="field-group">
          <label className="label">{quick ? t('editor.head') : t('editor.title')}</label>
          <div className="row" style={{ gap: 10 }}>
            <input
              className="input"
              value={quick ? head : title}
              autoFocus
              maxLength={quick ? 500 : 200}
              spellCheck={false}
              placeholder={quick ? t('editor.headHint') : ''}
              onKeyDown={enter}
              onChange={(e) => (quick ? setHead(e.target.value) : setTitle(e.target.value))}
            />
            {preview.trim() && <Avatar name={preview} />}
          </div>
        </div>

        {top.map(field)}

        <button className="btn btn-sm btn-ghost" style={{ margin: '4px 0 8px', paddingLeft: 6 }} onClick={() => setMore(!more)} aria-expanded={more}>
          <ChevronDown size={15} style={{ transform: more ? 'none' : 'rotate(-90deg)', transition: 'transform 180ms var(--ease-out)' }} />
          {t('editor.more')}
          {!more && <span className="muted small" style={{ fontWeight: 600 }}>· {t('editor.moreHint')}</span>}
        </button>

        {more && (
          <div>
            {rest.map(field)}
            <button
              className="btn btn-sm btn-ghost"
              onClick={() => setFields((fs) => [...fs, { uid: ++uid, id: null, key: '', label: '', value: '', secret: false, kind: 'text' }])}
            >
              <Plus size={14} /> {t('editor.addField')}
            </button>
            <div className="grid2" style={{ marginTop: 16 }}>
              <div className="field-group">
                <label className="label">{t('editor.folder')}</label>
                <Select
                  block
                  value={folder ?? ''}
                  label={t('editor.folder')}
                  onChange={(v) => setFolder(v || null)}
                  options={[{ value: '', label: t('editor.noFolder') }, ...folders.map((f) => ({ value: f.id, label: f.name }))]}
                />
              </div>
              <div className="field-group">
                <label className="label">{t('editor.tags')}</label>
                <input className="input" value={tags} placeholder={t('editor.tagsHint')} onChange={(e) => setTags(e.target.value)} />
              </div>
            </div>
            {kind === 'login' && (
              <div className="field-group">
                <label className="label">{t('editor.autotype')}</label>
                <input
                  className="input mono small"
                  value={autotype}
                  spellCheck={false}
                  maxLength={500}
                  placeholder="{USERNAME}{TAB}{PASSWORD}{ENTER}"
                  onChange={(e) => setAutotype(e.target.value)}
                />
                <div className="hint">{t('editor.autotypeHint')}</div>
              </div>
            )}
            <label className="check">
              <input type="checkbox" checked={favorite} onChange={(e) => setFavorite(e.target.checked)} />
              {t('detail.favorite')}
            </label>
            <label className="check">
              <input type="checkbox" checked={prot} onChange={(e) => setProt(e.target.checked)} />
              {t('protect.option')}
            </label>
            {id && (
              <div className="field-group" style={{ marginTop: 12 }}>
                <label className="label">{t('editor.kind')}</label>
                <Select
                  block
                  value={kind}
                  onChange={(k: Kind) => {
                    // Cambiar de tipo no borra nada: se conservan los campos y se añaden los que le falten
                    setKind(k)
                    setFields((fs) => [...fs, ...blank(k).filter((b) => !fs.some((x) => x.key === b.key))])
                  }}
                  options={KINDS.map((k) => ({ value: k, label: t(`kind.${k}`) }))}
                />
              </div>
            )}
          </div>
        )}

        <div className="editor-actions">
          <button className="btn btn-ghost" onClick={() => void leaveEditor()}>
            {t('common.cancel')}
          </button>
          <button className="btn btn-primary" disabled={saving} onClick={() => void save()}>
            {t('common.save')}
          </button>
        </div>
      </div>
      {qrFor !== null && (
        <QrModal
          onClose={() => setQrFor(null)}
          onFound={(uri) => {
            patch(qrFor, { value: uri })
            setQrFor(null)
            setMore(true)
          }}
        />
      )}
    </section>
  )
}
