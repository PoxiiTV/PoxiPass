import { useEffect, useMemo, useRef, useState } from 'react'
import { Copy, FileDown, FilePlus2, FolderOpen, Fingerprint, KeyRound, LifeBuoy, Lock, RefreshCw, ShieldCheck, Sparkles, Wand2, X } from 'lucide-react'
import { api, type Backup, type Recent } from '../lib/api'
import { ask, enterVault, getState, pickFile, setState, toast, useStore } from '../lib/store'
import { attempt, copyText, fail, saveKitPdf } from '../lib/actions'
import { ago, bytes, date, norm } from '../lib/format'
import { errorText, t as tr, useT } from '../i18n'
import { Logo, PasswordInput, StrengthBar, useStrength } from '../components/ui'

function pickVault(): Promise<string | null> {
  return pickFile({ mode: 'open', title: tr('picker.openVault'), exts: ['poxipass'] })
}

// ————————————————————————————————————————————————————————————— bienvenida

export function Welcome(): React.JSX.Element {
  const t = useT()
  const openExisting = async (): Promise<void> => {
    const p = await pickVault()
    if (p) setState({ unlockPath: p, screen: 'unlock' })
  }
  return (
    <div className="center-screen">
      <div className="card">
        <Logo size={64} />
        <h1 style={{ marginTop: 18 }}>
          Poxi<span style={{ color: 'var(--accent)' }}>Pass</span>
        </h1>
        <p className="lead">{t('welcome.tagline')}</p>
        <button className="choice" onClick={() => setState({ screen: 'create' })}>
          <span className="ico">
            <FilePlus2 size={20} />
          </span>
          <span>
            <b>{t('welcome.create')}</b>
            <small>{t('welcome.createHint')}</small>
          </span>
        </button>
        <button className="choice" onClick={() => void openExisting()}>
          <span className="ico">
            <FolderOpen size={20} />
          </span>
          <span>
            <b>{t('welcome.open')}</b>
            <small>{t('welcome.openHint')}</small>
          </span>
        </button>
        <button className="choice" onClick={() => setState({ screen: 'recover' })}>
          <span className="ico">
            <LifeBuoy size={20} />
          </span>
          <span>
            <b>{t('welcome.recover')}</b>
            <small>{t('welcome.recoverHint')}</small>
          </span>
        </button>
        <div style={{ textAlign: 'center', marginTop: 8 }}>
          <button className="link muted" onClick={() => setState({ screen: 'generatorOnly' })}>
            <Wand2 size={13} style={{ verticalAlign: -2, marginRight: 6 }} />
            {t('welcome.generator')}
          </button>
        </div>
      </div>
    </div>
  )
}

// ————————————————————————————————————————————————————————————— desbloquear

export function Unlock(): React.JSX.Element {
  const t = useT()
  const path = useStore((s) => s.unlockPath)
  const [recents, setRecents] = useState<Recent[]>([])
  const [pw, setPw] = useState('')
  const [busy, setBusy] = useState(false)
  const [shake, setShake] = useState(false)
  const [error, setError] = useState('')
  const helloTried = useRef<string | null>(null)

  useEffect(() => {
    void api.recentList().then(setRecents)
  }, [])

  const current = recents.find((r) => r.path === path)
  const name = current?.name ?? path?.split(/[\\/]/).pop()?.replace(/\.poxipass$/i, '') ?? ''

  const unlock = async (): Promise<void> => {
    if (!path || !pw || busy) return
    setBusy(true)
    setError('')
    try {
      const info = await api.vaultOpen(path, pw)
      setPw('')
      await enterVault(info)
    } catch (e) {
      setError(errorText(e))
      setShake(true)
      setTimeout(() => setShake(false), 400)
    } finally {
      setBusy(false)
    }
  }

  const hello = async (): Promise<void> => {
    if (!path) return
    setBusy(true)
    setError('')
    try {
      await enterVault(await api.vaultOpenHello(path))
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  // Con Windows Hello activo, su ventana sale sola (una vez por bóveda)
  useEffect(() => {
    if (current?.hello && current.exists && helloTried.current !== current.path && document.hasFocus()) {
      helloTried.current = current.path
      void hello()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.path, current?.hello])

  const other = async (): Promise<void> => {
    const p = await pickVault()
    if (p) {
      setState({ unlockPath: p })
      setPw('')
      setError('')
    }
  }

  return (
    <div className="center-screen">
      <div className="card">
        <div className={`lock-anim ${shake ? 'shake' : ''}`}>
          <Lock size={26} />
        </div>
        <h1>{name}</h1>
        <p className="lead small" style={{ marginBottom: 18 }}>
          {current && !current.exists ? <span className="danger-text">{t('unlock.missing')}</span> : <span className="muted">{path}</span>}
        </p>
        {current && !current.exists && path && (
          <RestorePanel
            path={path}
            onDone={() => {
              toast(t('restore.done'))
              void api.recentList().then(setRecents)
            }}
          />
        )}
        <div className="field-group">
          <label className="label">{t('unlock.password')}</label>
          <PasswordInput value={pw} onChange={setPw} autoFocus bad={!!error} onEnter={() => void unlock()} />
          {error && <div className="hint danger-text">{error}</div>}
        </div>
        <button className="btn btn-primary btn-lg btn-block" disabled={!pw || busy} onClick={() => void unlock()}>
          {busy ? t('unlock.unlocking') : t('unlock.submit')}
        </button>
        {current?.hello && (
          <button className="btn btn-lg btn-block" style={{ marginTop: 10 }} disabled={busy} onClick={() => void hello()}>
            <Fingerprint size={18} /> {t('hello.unlock')}
          </button>
        )}
        {recents.length > 1 && (
          <div className="recent-list" style={{ marginTop: 20 }}>
            {recents
              .filter((r) => r.path !== path)
              .map((r) => (
                <div key={r.path} className="row">
                  <button
                    className="recent"
                    onClick={() => {
                      setState({ unlockPath: r.path })
                      setPw('')
                      setError('')
                    }}
                  >
                    <KeyRound size={16} />
                    <div>
                      <b>{r.name}</b>
                      <div className="path">{r.path}</div>
                    </div>
                  </button>
                  <button
                    className="icon-btn"
                    data-tip={t('unlock.forget')}
                    aria-label={t('unlock.forget')}
                    onClick={() => void api.recentForget(r.path).then(() => api.recentList().then(setRecents))}
                  >
                    <X size={15} />
                  </button>
                </div>
              ))}
          </div>
        )}
        <div className="row" style={{ justifyContent: 'space-between', marginTop: 16, flexWrap: 'wrap' }}>
          <button className="link" onClick={() => void other()}>
            {t('unlock.openFile')}
          </button>
          <button className="link" onClick={() => setState({ screen: 'create' })}>
            {t('unlock.create')}
          </button>
        </div>
        <div style={{ marginTop: 6 }}>
          <button className="link muted" onClick={() => setState({ screen: 'recover' })}>
            {t('unlock.forgot')}
          </button>
        </div>
      </div>
    </div>
  )
}

/** El archivo del llavero no está: sus copias (AppData y carpeta extra) siguen ahí */
function RestorePanel({ path, onDone }: { path: string; onDone: () => void }): React.JSX.Element {
  const t = useT()
  const [list, setList] = useState<Backup[] | null>(null)
  useEffect(() => {
    void api
      .backupsFor(path)
      .then(setList)
      .catch(() => setList([]))
  }, [path])
  if (!list) return <div />
  return (
    <div className="panel" style={{ padding: 14, marginBottom: 16 }}>
      <b>{t('restore.title')}</b>
      <div className="hint" style={{ marginTop: 4, marginBottom: 10 }}>
        {list.length ? t('restore.lead') : t('restore.none')}
      </div>
      <div className="col" style={{ gap: 4, maxHeight: 220, overflowY: 'auto' }}>
        {list.slice(0, 20).map((b) => (
          <div key={`${b.extra}-${b.name}`} className="att">
            <span className="name">{date(b.modified)}</span>
            <span className="muted small">{ago(b.modified)}</span>
            {b.extra && <span className="chip">{t('restore.extra')}</span>}
            <span className="muted small num">{bytes(b.size)}</span>
            <button
              className="btn btn-sm btn-primary"
              onClick={async () => {
                if (!(await ask(t('restore.confirm', { d: date(b.modified) }), t('settings.restore')))) return
                if ((await attempt(api.backupRestoreTo(path, b.name, b.extra))) !== undefined) onDone()
              }}
            >
              {t('settings.restore')}
            </button>
          </div>
        ))}
      </div>
    </div>
  )
}

// ————————————————————————————————————————————————————————————— crear

type Step = 'form' | 'kit' | 'confirm' | 'hello'

export function Create(): React.JSX.Element {
  const t = useT()
  const app = useStore((s) => s.app)
  const [step, setStep] = useState<Step>('form')
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [pw, setPw] = useState('')
  const [pw2, setPw2] = useState('')
  const [busy, setBusy] = useState(false)
  const [words, setWords] = useState<string[]>([])
  const s = useStrength(pw)

  const fileName = (name.trim() || t('create.namePlaceholder')).replace(/[<>:"/\\|?*]/g, '')
  const finalPath = path || (app ? `${app.defaultDir}\\${fileName}.poxipass` : '')
  const ok = pw.length > 0 && pw === pw2 && (s?.score ?? 0) >= 3

  const choose = async (): Promise<void> => {
    const p = await pickFile({ mode: 'save', title: t('picker.newVault'), exts: ['poxipass'], ext: 'poxipass', name: `${fileName}.poxipass` })
    if (p) setPath(p)
  }

  const suggest = async (): Promise<void> => {
    const cfg = getState().config
    if (!cfg) return
    const g = await api.generate({ ...cfg.generator, mode: 'passphrase', words: 5, capitalize: true, number: true, separator: '-' })
    setPw(g.value)
    setPw2(g.value)
  }

  const create = async (): Promise<void> => {
    if (!ok || busy) return
    setBusy(true)
    try {
      const w = await api.vaultCreate(finalPath, name.trim() || t('create.namePlaceholder'), pw)
      setPw('')
      setPw2('')
      setWords(w)
      const info = await api.vaultInfo()
      setState({ info, unlockPath: info?.path ?? finalPath, kit: { words: w, vault: info?.name ?? name, date: Date.now() / 1000 } })
      setStep('kit')
    } catch (e) {
      fail(e)
    } finally {
      setBusy(false)
    }
  }

  const finish = async (): Promise<void> => {
    const info = await api.vaultInfo()
    setState({ kit: null })
    if (info) await enterVault(info)
  }

  if (step === 'kit') {
    return (
      <KitView
        words={words}
        onAnother={async () => {
          const w = await attempt(api.recoveryRegenerate())
          if (w) {
            setWords(w)
            setState((st) => ({ kit: st.kit && { ...st.kit, words: w } }))
          }
        }}
        onNext={() => setStep('confirm')}
      />
    )
  }
  if (step === 'confirm') {
    return (
      <KitConfirm
        words={words}
        onBack={() => setStep('kit')}
        onDone={async () => {
          const avail = await api.helloAvailable().catch(() => false)
          if (avail) setStep('hello')
          else await finish()
        }}
      />
    )
  }
  if (step === 'hello') return <HelloOffer onDone={() => void finish()} />

  return (
    <div className="center-screen">
      <div className="card">
        <h1>{t('create.title')}</h1>
        <p className="lead small">{t('create.passwordHint')}</p>
        <div className="field-group">
          <label className="label">{t('create.name')}</label>
          <input className="input" value={name} placeholder={t('create.namePlaceholder')} maxLength={80} onChange={(e) => setName(e.target.value)} />
        </div>
        <div className="field-group">
          <label className="label">{t('create.location')}</label>
          <div className="row">
            <input className="input small" value={finalPath} readOnly data-tip={finalPath} style={{ direction: 'rtl', textAlign: 'left' }} />
            <button className="btn" onClick={() => void choose()}>
              {t('common.change')}
            </button>
          </div>
        </div>
        <div className="field-group">
          <label className="label">{t('create.password')}</label>
          <PasswordInput
            value={pw}
            onChange={setPw}
            autoFocus
            extra={
              <button type="button" className="icon-btn" data-tip={t('create.suggest')} aria-label={t('create.suggest')} onClick={() => void suggest()}>
                <Sparkles size={16} />
              </button>
            }
          />
          <StrengthBar s={s} />
          {pw && s && s.score < 3 && <div className="hint">{t('create.tooWeak')}</div>}
        </div>
        <div className="field-group">
          <label className="label">{t('create.confirm')}</label>
          <PasswordInput value={pw2} onChange={setPw2} bad={!!pw2 && pw2 !== pw} onEnter={() => void create()} />
          {pw2 && pw2 !== pw && <div className="hint danger-text">{t('create.mismatch')}</div>}
        </div>
        <button className="btn btn-primary btn-lg btn-block" disabled={!ok || busy} onClick={() => void create()}>
          {busy ? t('create.creating') : t('create.submit')}
        </button>
        <div style={{ marginTop: 12 }}>
          <button className="link muted" onClick={() => setState({ screen: getState().unlockPath ? 'unlock' : 'welcome' })}>
            {t('common.back')}
          </button>
        </div>
      </div>
    </div>
  )
}

const JOKERS = ['maricon', 'z']
export const isJoker = (w: string): boolean => JOKERS.includes(norm(w))

export function WordGrid({ words }: { words: string[] }): React.JSX.Element {
  return (
    <div className="words">
      {words.map((w, i) => (
        <div key={i} className="word">
          <i className="num">{i + 1}</i>
          <span>{w}</span>
        </div>
      ))}
    </div>
  )
}

/** Copiar y PDF del kit (con el portapapeles seguro y el PDF propio de la app) */
export function KitActions({ words, vault }: { words: string[]; vault: string }): React.JSX.Element {
  const t = useT()
  return (
    <>
      <button className="btn" onClick={() => void copyText(words.join(' '))}>
        <Copy size={16} /> {t('kit.copy')}
      </button>
      <button className="btn" onClick={() => void saveKitPdf(words, vault)}>
        <FileDown size={16} /> {t('kit.pdf')}
      </button>
    </>
  )
}

export function KitView({
  words,
  onAnother,
  onNext,
  nextLabel
}: {
  words: string[]
  onAnother: () => Promise<void>
  onNext: () => void
  nextLabel?: string
}): React.JSX.Element {
  const t = useT()
  const [saved, setSaved] = useState(false)
  const [spin, setSpin] = useState(false)
  useBusy()
  const vault = useStore((s) => s.info?.name ?? '')
  return (
    <div className="center-screen">
      <div className="card wide">
        <div className="row" style={{ marginBottom: 6 }}>
          <ShieldCheck size={22} color="var(--accent)" />
          <h1 style={{ margin: 0 }}>{t('kit.title')}</h1>
        </div>
        <p className="lead small">{t('kit.lead')}</p>
        <WordGrid words={words} />
        <div className="row" style={{ flexWrap: 'wrap' }}>
          <button
            className="btn"
            disabled={spin}
            onClick={async () => {
              setSpin(true)
              await onAnother()
              setSpin(false)
              setSaved(false)
            }}
          >
            <RefreshCw size={16} /> {t('kit.another')}
          </button>
          <KitActions words={words} vault={vault} />
          <div className="spacer" />
          <label className="check">
            <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} />
            {t('kit.saved')}
          </label>
        </div>
        <button className="btn btn-primary btn-lg btn-block" style={{ marginTop: 18 }} disabled={!saved} onClick={onNext}>
          {nextLabel ?? t('common.next')}
        </button>
      </div>
    </div>
  )
}

/** Mientras se ve un kit no se bloquea por inactividad (apuntarlo a mano lleva su rato) */
function useBusy(): void {
  useEffect(() => {
    void api.setBusy(true)
    return () => void api.setBusy(false)
  }, [])
}

export function KitConfirm({ words, onBack, onDone }: { words: string[]; onBack: () => void; onDone: () => Promise<void> | void }): React.JSX.Element {
  const t = useT()
  useBusy()
  const real = words
  // 3 posiciones al azar de las que se ven, ordenadas
  const ask = useMemo(() => {
    const ok = words.map((w, i) => (isJoker(w) ? -1 : i)).filter((i) => i >= 0)
    const idx = new Set<number>()
    const r = new Uint32Array(8)
    while (idx.size < 3) {
      crypto.getRandomValues(r)
      for (const v of r) if (idx.size < 3) idx.add(ok[v % ok.length])
    }
    return [...idx].sort((a, b) => a - b)
  }, [words])
  const [vals, setVals] = useState<string[]>(['', '', ''])
  const good = (i: number): boolean => {
    const v = norm(vals[i].trim())
    return v.length >= Math.min(4, norm(real[ask[i]]).length) && norm(real[ask[i]]).startsWith(v)
  }
  const allGood = ask.every((_, i) => good(i))
  return (
    <div className="center-screen">
      <div className="card">
        <h1>{t('kit.confirmTitle')}</h1>
        <p className="lead small">{t('kit.confirmLead')}</p>
        {ask.map((pos, i) => (
          <div className="field-group" key={pos}>
            <label className="label">{t('kit.word', { n: pos + 1 })}</label>
            <input
              className={`input ${vals[i] && !good(i) && norm(vals[i]).length >= 4 ? 'bad' : ''}`}
              value={vals[i]}
              autoFocus={i === 0}
              spellCheck={false}
              onChange={(e) => setVals((v) => v.map((x, j) => (j === i ? e.target.value : x)))}
              onKeyDown={(e) => e.key === 'Enter' && allGood && void onDone()}
            />
          </div>
        ))}
        <button className="btn btn-primary btn-lg btn-block" disabled={!allGood} onClick={() => void onDone()}>
          {t('common.next')}
        </button>
        <div style={{ marginTop: 12 }}>
          <button className="link muted" onClick={onBack}>
            {t('common.back')}
          </button>
        </div>
      </div>
    </div>
  )
}

function HelloOffer({ onDone }: { onDone: () => void }): React.JSX.Element {
  const t = useT()
  const [busy, setBusy] = useState(false)
  return (
    <div className="center-screen">
      <div className="card">
        <div className="lock-anim">
          <Fingerprint size={28} />
        </div>
        <h1>{t('hello.offerTitle')}</h1>
        <p className="lead small">{t('hello.offerLead')}</p>
        <button
          className="btn btn-primary btn-lg btn-block"
          disabled={busy}
          onClick={async () => {
            setBusy(true)
            try {
              await api.helloEnable()
              toast(t('hello.enabled'))
              onDone()
            } catch (e) {
              fail(e)
            } finally {
              setBusy(false)
            }
          }}
        >
          <Fingerprint size={18} /> {t('hello.enable')}
        </button>
        <button className="btn btn-ghost btn-lg btn-block" style={{ marginTop: 8 }} onClick={onDone}>
          {t('hello.skip')}
        </button>
      </div>
    </div>
  )
}

// ————————————————————————————————————————————————————————————— recuperar

export function Recover(): React.JSX.Element {
  const t = useT()
  const [path, setPath] = useState(useStore.getState().unlockPath ?? '')
  const [phrase, setPhrase] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const recover = async (): Promise<void> => {
    if (!path || !phrase.trim() || busy) return
    setBusy(true)
    setError('')
    try {
      const info = await api.vaultRecover(path, phrase)
      setPhrase('')
      await enterVault(info)
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="center-screen">
      <div className="card wide">
        <div className="lock-anim">
          <LifeBuoy size={26} />
        </div>
        <h1>{t('recover.title')}</h1>
        <div className="field-group" style={{ marginTop: 16 }}>
          <label className="label">{t('recover.file')}</label>
          <div className="row">
            <input className="input small" value={path} readOnly style={{ direction: 'rtl', textAlign: 'left' }} />
            <button
              className="btn"
              onClick={async () => {
                const p = await pickVault()
                if (p) setPath(p)
              }}
            >
              {t('recover.choose')}
            </button>
          </div>
        </div>
        <div className="field-group">
          <label className="label">{t('recover.words')}</label>
          <textarea className="textarea mono" rows={5} value={phrase} spellCheck={false} onChange={(e) => setPhrase(e.target.value)} />
          <div className="hint">{t('recover.wordsHint')}</div>
          {error && <div className="hint danger-text">{error}</div>}
        </div>
        <button className="btn btn-primary btn-lg btn-block" disabled={!path || !phrase.trim() || busy} onClick={() => void recover()}>
          {t('recover.submit')}
        </button>
        <div style={{ marginTop: 12 }}>
          <button className="link muted" onClick={() => setState({ screen: getState().unlockPath ? 'unlock' : 'welcome' })}>
            {t('common.back')}
          </button>
        </div>
      </div>
    </div>
  )
}

export function MustChange(): React.JSX.Element {
  const t = useT()
  const [pw, setPw] = useState('')
  const [pw2, setPw2] = useState('')
  const [busy, setBusy] = useState(false)
  const s = useStrength(pw)
  const ok = pw.length > 0 && pw === pw2 && (s?.score ?? 0) >= 3
  const save = async (): Promise<void> => {
    if (!ok || busy) return
    setBusy(true)
    try {
      const info = await api.passwordChange(null, pw)
      setPw('')
      setPw2('')
      toast(t('settings.passwordChanged'))
      await enterVault(info)
    } catch (e) {
      fail(e)
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="center-screen">
      <div className="card">
        <h1>{t('recover.newTitle')}</h1>
        <p className="lead small">{t('recover.newLead')}</p>
        <div className="field-group">
          <label className="label">{t('settings.newPassword')}</label>
          <PasswordInput value={pw} onChange={setPw} autoFocus />
          <StrengthBar s={s} />
        </div>
        <div className="field-group">
          <label className="label">{t('create.confirm')}</label>
          <PasswordInput value={pw2} onChange={setPw2} bad={!!pw2 && pw2 !== pw} onEnter={() => void save()} />
        </div>
        <button className="btn btn-primary btn-lg btn-block" disabled={!ok || busy} onClick={() => void save()}>
          {t('common.save')}
        </button>
      </div>
    </div>
  )
}
