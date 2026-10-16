import { useEffect, useState } from 'react'
import { setState, useStore } from '../lib/store'
import { useT } from '../i18n'
import { PasswordInput, Modal } from './ui'

const CLOSE_SECS = 60

/** Visor para enseñar un secreto en grande, como QR o un adjunto. Se cierra solo al minuto (no se queda en pantalla). */
export function Viewer(): React.JSX.Element | null {
  const t = useT()
  const v = useStore((s) => s.viewer)
  useEffect(() => {
    if (!v) return
    const id = setTimeout(() => setState({ viewer: null }), CLOSE_SECS * 1000)
    return () => clearTimeout(id)
  }, [v])
  if (!v) return null
  const close = (): void => setState({ viewer: null })
  return (
    <Modal onClose={close} wide className="viewer">
      <h2>{v.title}</h2>
      {v.type === 'large' && (
        <div className="large-type">
          {[...v.text].map((c, i) => (
            <span key={i} className={/\d/.test(c) ? 'd' : /[^\p{L}\d\s]/u.test(c) ? 's' : undefined}>
              <b>{c === ' ' ? '␣' : c}</b>
              <i>{i + 1}</i>
            </span>
          ))}
        </div>
      )}
      {/* El SVG lo genera la app (crate qrcode): solo rectángulos, sin texto ni scripts */}
      {v.type === 'qr' && <div className="qr" dangerouslySetInnerHTML={{ __html: v.svg }} />}
      {v.type === 'image' && <img className="att-preview" src={v.src} alt={v.title} />}
      {v.type === 'text' && <pre className="att-text selectable">{v.text}</pre>}
      <p className="hint">{t('viewer.autoClose', { n: CLOSE_SECS })}</p>
      <div className="modal-actions">
        <button className="btn btn-primary" onClick={close}>
          {t('common.close')}
        </button>
      </div>
    </Modal>
  )
}

/** Pide la contraseña maestra cuando hace falta confirmar y este PC no tiene Windows Hello */
export function PasswordPrompt(): React.JSX.Element | null {
  const t = useT()
  const p = useStore((s) => s.pwPrompt)
  const [pw, setPw] = useState('')
  if (!p) return null
  const done = (v: string | null): void => {
    setPw('')
    p.resolve(v)
  }
  return (
    <Modal onClose={() => done(null)}>
      <h2>{t('protect.title')}</h2>
      <p>{p.reason}</p>
      <PasswordInput value={pw} onChange={setPw} autoFocus onEnter={() => pw && done(pw)} />
      <div className="modal-actions">
        <button className="btn btn-ghost" onClick={() => done(null)}>
          {t('common.cancel')}
        </button>
        <button className="btn btn-primary" disabled={!pw} onClick={() => done(pw)}>
          {t('common.next')}
        </button>
      </div>
    </Modal>
  )
}
