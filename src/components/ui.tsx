import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import { api, type Strength } from '../lib/api'
import { crackTime, hue, strengthLabel } from '../lib/format'
import { useT } from '../i18n'
import logoUrl from '../assets/logo.svg'

export function Modal({
  children,
  onClose,
  wide,
  className = ''
}: {
  children: ReactNode
  onClose?: () => void
  wide?: boolean
  className?: string
}): React.JSX.Element {
  useEffect(() => {
    if (!onClose) return
    const onKey = (e: KeyboardEvent): void => {
      // Con un kit de emergencia encima no se cierra nada por debajo (se perdería); un desplegable abierto
      // se cierra él solo primero
      if (e.key === 'Escape' && !document.querySelector('.kit-layer, .dd.open, .ctx')) {
        e.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onClose])
  const box = useRef<HTMLDivElement>(null)
  // Tab no se escapa del modal, y al cerrarlo el foco vuelve a donde estaba
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null
    if (!box.current?.contains(document.activeElement)) box.current?.querySelector<HTMLElement>('[autofocus], input, button')?.focus()
    const trap = (e: KeyboardEvent): void => {
      if (e.key !== 'Tab' || !box.current) return
      const all = [...box.current.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea, [tabindex]:not([tabindex="-1"])')]
      if (!all.length) return
      const [first, last] = [all[0], all[all.length - 1]]
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
    window.addEventListener('keydown', trap)
    return () => {
      window.removeEventListener('keydown', trap)
      before?.focus?.()
    }
  }, [])
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div ref={box} className={`modal ${wide ? 'wide' : ''} ${className}`} role="dialog" aria-modal="true">
        {children}
      </div>
    </div>
  )
}

export function Switch({ on, onChange, label }: { on: boolean; onChange: (v: boolean) => void; label?: string }): React.JSX.Element {
  return <button type="button" role="switch" aria-checked={on} aria-label={label} className={`switch ${on ? 'on' : ''}`} onClick={() => onChange(!on)} />
}

export function Avatar({ name: title, icon, lg }: { name: string; icon?: string | null; lg?: boolean }): React.JSX.Element {
  if (icon) {
    return (
      <span className={`avatar ${lg ? 'lg' : ''}`}>
        <img src={icon} alt="" />
      </span>
    )
  }
  const h = hue(title.toLowerCase())
  return (
    <span className={`avatar ${lg ? 'lg' : ''}`} style={{ background: `linear-gradient(135deg, hsl(${h} 70% 52%), hsl(${(h + 40) % 360} 75% 42%))` }}>
      {(title.trim()[0] ?? '?').toUpperCase()}
    </span>
  )
}

const COLORS = ['var(--s0)', 'var(--s1)', 'var(--s2)', 'var(--s3)', 'var(--s4)']

export function StrengthBar({ s, bits }: { s: Strength | null; bits?: number }): React.JSX.Element | null {
  const t = useT()
  if (!s) return null
  const filled = s.score === 0 ? 0 : s.score
  return (
    <div className="strength">
      <div className="bars">
        {[1, 2, 3, 4].map((i) => (
          <i key={i} style={i <= filled ? { background: COLORS[s.score] } : undefined} />
        ))}
      </div>
      <div className="meta">
        <span>
          <b style={{ color: COLORS[s.score] }}>{strengthLabel(s.score)}</b> · {t('strength.crack', { t: crackTime(s.crackSecs) })}
        </span>
        <span className="num">{t('strength.bits', { n: Math.round(bits ?? s.bits) })}</span>
      </div>
    </div>
  )
}

/** Fortaleza de un texto que se está escribiendo (se calcula en Rust, con un pelín de espera) */
export function useStrength(value: string): Strength | null {
  const [s, setS] = useState<Strength | null>(null)
  useEffect(() => {
    if (!value) {
      setS(null)
      return
    }
    let live = true
    const id = setTimeout(() => {
      void api.strength(value).then((r) => live && setS(r))
    }, 120)
    return () => {
      live = false
      clearTimeout(id)
    }
  }, [value])
  return s
}

/** Campo de contraseña con ojo para mostrarla */
export function PasswordInput({
  value,
  onChange,
  placeholder,
  autoFocus,
  bad,
  onEnter,
  extra
}: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  autoFocus?: boolean
  bad?: boolean
  onEnter?: () => void
  extra?: ReactNode
}): React.JSX.Element {
  const t = useT()
  const [show, setShow] = useState(false)
  const [caps, setCaps] = useState(false)
  const ref = useRef<HTMLInputElement>(null)
  const checkCaps = (e: React.KeyboardEvent | React.MouseEvent): void => setCaps(e.getModifierState('CapsLock'))
  return (
    <>
    <div className="input-wrap">
      <input
        ref={ref}
        className={`input ${show ? 'mono' : ''} ${bad ? 'bad' : ''}`}
        type={show ? 'text' : 'password'}
        value={value}
        placeholder={placeholder}
        autoFocus={autoFocus}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          checkCaps(e)
          if (e.key === 'Enter') onEnter?.()
        }}
        onKeyUp={checkCaps}
        onMouseDown={checkCaps}
        onBlur={() => setCaps(false)}
      />
      <div className="actions">
        {extra}
        <button type="button" className="icon-btn" onClick={() => setShow(!show)} aria-label={show ? t('common.hide') : t('common.show')} data-tip={show ? t('common.hide') : t('common.show')}>
          {show ? <EyeOff size={16} /> : <Eye size={16} />}
        </button>
      </div>
    </div>
    {caps && <div className="hint caps">⇪ {t('common.capsLock')}</div>}
    </>
  )
}

/** Contraseña coloreada: números en acento, símbolos en ámbar (se leen mejor) */
export function Colored({ value }: { value: string }): React.JSX.Element {
  return (
    <>
      {[...value].map((c, i) => (
        <span key={i} className={/\d/.test(c) ? 'd' : /[^\p{L}\d\s]/u.test(c) ? 's' : undefined}>
          {c}
        </span>
      ))}
    </>
  )
}

export function Logo({ size = 22 }: { size?: number }): React.JSX.Element {
  return <img src={logoUrl} width={size} height={size} alt="" draggable={false} />
}
