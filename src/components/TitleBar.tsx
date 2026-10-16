import { useEffect, useRef } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { Lock, Minus, Search, Square, X } from 'lucide-react'
import { dismiss, setState, useStore } from '../lib/store'
import { lockNow } from '../lib/actions'
import { useT } from '../i18n'
import { Logo } from './ui'

const win = getCurrentWindow()

export function TitleBar(): React.JSX.Element {
  const t = useT()
  const screen = useStore((s) => s.screen)
  const query = useStore((s) => s.query)
  const inVault = screen === 'vault'
  const input = useRef<HTMLInputElement>(null)

  // Ctrl+K / Ctrl+F: al buscador
  useEffect(() => {
    if (!inVault) return
    const onKey = (e: KeyboardEvent): void => {
      if (e.ctrlKey && !e.altKey && (e.key === 'k' || e.key === 'f')) {
        e.preventDefault()
        input.current?.focus()
        input.current?.select()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [inVault])

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="brand" data-tauri-drag-region>
        <Logo />
        <span data-tauri-drag-region>
          Poxi<b>Pass</b>
        </span>
      </div>
      {inVault && (
        <div className="search">
          <Search size={15} />
          <input
            ref={input}
            value={query}
            placeholder={t('common.search')}
            spellCheck={false}
            onChange={(e) => setState({ query: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                setState({ query: '' })
                input.current?.blur()
              }
              if (e.key === 'ArrowDown' || e.key === 'Enter') {
                e.preventDefault()
                input.current?.blur()
                document.querySelector<HTMLButtonElement>('.row-entry')?.click()
              }
            }}
          />
          <kbd>Ctrl K</kbd>
        </div>
      )}
      <div className="spacer" data-tauri-drag-region />
      {inVault && (
        <button className="icon-btn no-drag" onClick={() => void lockNow()} data-tip={t('titlebar.lock')} aria-label={t('titlebar.lock')}>
          <Lock size={16} />
        </button>
      )}
      <div className="win-controls">
        <button onClick={() => void win.minimize()} aria-label={t('titlebar.minimize')}>
          <Minus size={16} />
        </button>
        <button onClick={() => void win.toggleMaximize()} aria-label={t('titlebar.maximize')}>
          <Square size={12} />
        </button>
        <button className="close" onClick={() => void win.close()} aria-label={t('titlebar.close')}>
          <X size={17} />
        </button>
      </div>
    </header>
  )
}

export function Toasts(): React.JSX.Element {
  const toasts = useStore((s) => s.toasts)
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((x) => (
        <div key={x.id} className={`toast ${x.kind}`}>
          <ToastIcon kind={x.kind} />
          {x.text}
          {x.action && (
            <button
              className="btn btn-sm toast-action"
              onClick={() => {
                dismiss(x.id)
                x.action?.run()
              }}
            >
              {x.action.label}
            </button>
          )}
          {x.secs ? <span className="bar" style={{ animationDuration: `${x.secs}s` }} /> : null}
        </div>
      ))}
    </div>
  )
}

function ToastIcon({ kind }: { kind: 'ok' | 'err' | 'info' }): React.JSX.Element {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
      {kind === 'ok' && <path d="M20 6 9 17l-5-5" />}
      {kind === 'err' && (
        <>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 8v4M12 16h.01" />
        </>
      )}
      {kind === 'info' && (
        <>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 16v-4M12 8h.01" />
        </>
      )}
    </svg>
  )
}
