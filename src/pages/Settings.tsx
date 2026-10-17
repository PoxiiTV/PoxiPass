import { useEffect, useState } from 'react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { Database, Globe, History, Info, Keyboard, Palette, ShieldCheck, X } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { api, isAppError, type Backup, type Config, type ImportKey, type PairedBrowser, type Preview } from '../lib/api'
import { ask, pickFile, refreshEntries, setState, toast, updateConfig, useStore } from '../lib/store'
import { attempt, fail } from '../lib/actions'
import { applyTheme } from '../lib/theme'
import { checkForUpdates, RELEASES } from '../lib/update'
import { errorText, useT } from '../i18n'
import { ago, bytes, date } from '../lib/format'
import { Select } from '../components/controls'
import { Modal, PasswordInput, StrengthBar, Switch, useStrength } from '../components/ui'
import { KitConfirm, KitView } from './Start'

type Cat = 'general' | 'security' | 'autotype' | 'browser' | 'data' | 'about'
const CATS: [Cat, LucideIcon][] = [
  ['general', Palette],
  ['security', ShieldCheck],
  ['autotype', Keyboard],
  ['browser', Globe],
  ['data', Database],
  ['about', Info]
]
const ACCENTS: Config['accent'][] = ['blue', 'emerald', 'violet', 'pink', 'amber', 'red']
const ACCENT_HEX: Record<Config['accent'], string> = {
  emerald: '#10b981',
  violet: '#8b5cf6',
  blue: '#3b82f6',
  pink: '#ec4899',
  amber: '#f59e0b',
  red: '#ef4444'
}

function Row({ title, hint, bad, children }: { title: string; hint?: string; bad?: boolean; children?: React.ReactNode }): React.JSX.Element {
  return (
    <div className="set-row" role="group" aria-label={title}>
      <div className="txt">
        <b>{title}</b>
        {hint && <small className={bad ? 'danger-text' : undefined}>{hint}</small>}
      </div>
      {children}
    </div>
  )
}

/** Navegadores instalados donde se puede añadir la extensión (abre su tienda); nada hasta que esté publicada */
function Install(): React.JSX.Element | null {
  const t = useT()
  const [list, setList] = useState<{ key: string; name: string }[]>([])
  useEffect(() => {
    void api.extensionTargets().then(setList)
  }, [])
  if (!list.length) return null
  return (
    <>
      <h3 style={{ marginTop: 22 }}>{t('settings.install')}</h3>
      <div className="hint">{t('settings.installHint')}</div>
      {list.map((b) => (
        <Row key={b.key} title={b.name}>
          <button className="btn btn-sm" onClick={() => void attempt(api.extensionInstall(b.key))}>
            {t('settings.installBtn')}
          </button>
        </Row>
      ))}
    </>
  )
}

/** Navegadores con la extensión conectada; se pueden desconectar uno a uno */
function Browsers(): React.JSX.Element {
  const t = useT()
  const [list, setList] = useState<PairedBrowser[] | null>(null)
  useEffect(() => {
    void api.browsersList().then(setList)
  }, [])
  if (!list) return <div />
  if (!list.length) return <div className="empty small">{t('settings.browsersEmpty')}</div>
  return (
    <>
      {list.map((b) => (
        <Row key={b.id} title={b.name} hint={t('settings.browserUsed', { t: ago(b.used) })}>
          <button
            className="btn btn-sm"
            onClick={async () => {
              if (await ask(t('settings.browserForgetAsk', { b: b.name }), t('settings.browserForget'))) setList(await api.browserForget(b.id))
            }}
          >
            {t('settings.browserForget')}
          </button>
        </Row>
      ))}
    </>
  )
}

/** Pide la contraseña maestra antes de algo delicado (exportar en claro, kit nuevo) */
function PasswordGate({ onOk, onClose }: { onOk: () => void; onClose: () => void }): React.JSX.Element {
  const t = useT()
  const [pw, setPw] = useState('')
  const [bad, setBad] = useState(false)
  const [busy, setBusy] = useState(false)
  const go = async (): Promise<void> => {
    if (!pw || busy) return
    setBusy(true)
    const ok = await attempt(api.verifyPassword(pw))
    setBusy(false)
    if (ok) {
      setPw('')
      onOk()
    } else setBad(true)
  }
  return (
    <Modal onClose={onClose}>
      <h2>{t('settings.confirmPassword')}</h2>
      <PasswordInput value={pw} onChange={(v) => (setPw(v), setBad(false))} autoFocus bad={bad} onEnter={() => void go()} />
      {bad && <div className="hint danger-text">{t('err.vault.wrongPassword')}</div>}
      <div className="modal-actions">
        <button className="btn btn-ghost" onClick={onClose}>
          {t('common.cancel')}
        </button>
        <button className="btn btn-primary" disabled={!pw || busy} onClick={() => void go()}>
          {t('common.next')}
        </button>
      </div>
    </Modal>
  )
}

function HotkeyRecorder({ value, onChange }: { value: string; onChange: (v: string) => void }): React.JSX.Element {
  const t = useT()
  const [rec, setRec] = useState(false)
  useEffect(() => {
    if (!rec) return
    const onKey = (e: KeyboardEvent): void => {
      e.preventDefault()
      e.stopPropagation()
      if (e.key === 'Escape') return setRec(false)
      const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : /^F\d{1,2}$/.test(e.code) ? e.code : e.code === 'Space' ? 'Space' : null
      if (!key || !(e.ctrlKey || e.altKey)) return
      onChange([e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', key].filter(Boolean).join('+'))
      setRec(false)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [rec, onChange])
  return (
    <button className={`hotkey ${rec ? 'rec' : ''}`} onClick={() => setRec(true)}>
      {rec ? t('settings.record') : value}
    </button>
  )
}

function ChangePassword(): React.JSX.Element {
  const t = useT()
  const [cur, setCur] = useState('')
  const [pw, setPw] = useState('')
  const [pw2, setPw2] = useState('')
  const [busy, setBusy] = useState(false)
  const s = useStrength(pw)
  const ok = !!cur && !!pw && pw === pw2 && (s?.score ?? 0) >= 3
  return (
    <div className="panel" style={{ marginTop: 10 }}>
      <div className="field-group">
        <label className="label">{t('settings.current')}</label>
        <PasswordInput value={cur} onChange={setCur} />
      </div>
      <div className="field-group">
        <label className="label">{t('settings.newPassword')}</label>
        <PasswordInput value={pw} onChange={setPw} />
        <StrengthBar s={s} />
        {pw && s && s.score < 3 && <div className="hint">{t('create.tooWeak')}</div>}
      </div>
      <div className="field-group">
        <label className="label">{t('create.confirm')}</label>
        <PasswordInput value={pw2} onChange={setPw2} bad={!!pw2 && pw2 !== pw} />
        {pw2 && pw2 !== pw && <div className="hint danger-text">{t('create.mismatch')}</div>}
      </div>
      <button
        className="btn btn-primary"
        disabled={!ok || busy}
        onClick={async () => {
          setBusy(true)
          try {
            await api.passwordChange(cur, pw)
            setCur('')
            setPw('')
            setPw2('')
            toast(t('settings.passwordChanged'))
          } catch (e) {
            fail(e)
          } finally {
            setBusy(false)
          }
        }}
      >
        {t('settings.changePassword')}
      </button>
    </div>
  )
}

export function Settings(): React.JSX.Element {
  const t = useT()
  const cfg = useStore((s) => s.config)
  const app = useStore((s) => s.app)
  const info = useStore((s) => s.info)
  const [cat, setCat] = useState<Cat>('general')
  const [autostart, setAutostart] = useState(false)
  const [hello, setHello] = useState<boolean | null>(null)
  const [gate, setGate] = useState<null | (() => void)>(null)
  const [kitWords, setKitWords] = useState<string[] | null>(null)
  const [kitConfirm, setKitConfirm] = useState(false)
  const [preview, setPreview] = useState<Preview | null>(null)
  // .kdbx: archivo que espera su contraseña (o archivo llave) y lo escrito, que se reenvía al importar
  const [kdbxPath, setKdbxPath] = useState<string | null>(null)
  const [importKey, setImportKey] = useState<ImportKey>({ password: '', keyfile: null })
  const [kdbxErr, setKdbxErr] = useState('')
  const [kdbxBusy, setKdbxBusy] = useState(false)
  const [showPw, setShowPw] = useState(false)
  const [vaultName, setVaultName] = useState(info?.name ?? '')
  const [busyKeys, setBusyKeys] = useState<string[]>([])
  const [backups, setBackups] = useState<Backup[] | null>(null)

  useEffect(() => {
    void api.autostartGet().then(setAutostart)
    void api.helloAvailable().then(setHello)
  }, [])
  // Se vuelve a mirar al cambiar un atajo
  useEffect(() => {
    void api.hotkeyFailures().then(setBusyKeys)
  }, [cfg?.autotypeHotkey, cfg?.quickHotkey, cfg?.autotypeEnabled, cfg?.quickEnabled])
  const keyHint = (k: string, normal?: string): string | undefined => (busyKeys.includes(k) ? t('settings.hotkeyFailed', { k }) : normal)

  if (!cfg) return <div />
  const set = (p: Partial<Config>): void => {
    void updateConfig(p).then(() => {
      if (p.theme || p.accent) applyTheme({ ...cfg, ...p })
    })
  }
  const close = (): void => setState({ settingsOpen: false })

  const doImport = async (): Promise<void> => {
    const p = await pickFile({ mode: 'open', title: t('settings.importTitle'), exts: ['csv', 'json', 'kdbx', '1pux'] })
    if (!p) return
    setImportKey({ password: '', keyfile: null })
    try {
      setPreview(await api.importPreview(p))
    } catch (e) {
      if (!isAppError(e) || e.code !== 'import.needsPassword') return fail(e)
      setKdbxErr('')
      setKdbxPath(p)
    }
  }
  const openKdbx = async (): Promise<void> => {
    if (!kdbxPath || kdbxBusy || (!importKey.password && !importKey.keyfile)) return
    setKdbxBusy(true)
    try {
      setPreview(await api.importPreview(kdbxPath, importKey))
      setKdbxPath(null)
    } catch (e) {
      setKdbxErr(errorText(e))
    } finally {
      setKdbxBusy(false)
    }
  }
  const closeImport = (): void => {
    setPreview(null)
    setImportKey({ password: '', keyfile: null })
  }

  const body: Record<Cat, React.JSX.Element> = {
    general: (
      <>
        <h3>{t('settings.general')}</h3>
        <Row title={t('settings.language')}>
          <Select
            value={cfg.lang ?? ''}
            onChange={(v) => set({ lang: (v || null) as Config['lang'] })}
            options={[
              { value: '', label: t('settings.languageAuto') },
              { value: 'es', label: 'Español' },
              { value: 'en', label: 'English' }
            ]}
          />
        </Row>
        <Row title={t('settings.theme')}>
          <div className="segmented">
            {(['dark', 'light', 'system'] as const).map((th) => (
              <button key={th} className={cfg.theme === th ? 'on' : ''} onClick={() => set({ theme: th })}>
                {t(`settings.theme${th[0].toUpperCase()}${th.slice(1)}`)}
              </button>
            ))}
          </div>
        </Row>
        <Row title={t('settings.accent')}>
          <div className="accents">
            {ACCENTS.map((a) => (
              <button key={a} aria-label={t(`settings.colors.${a}`)} data-tip={t(`settings.colors.${a}`)} className={cfg.accent === a ? 'on' : ''} style={{ background: ACCENT_HEX[a] }} onClick={() => set({ accent: a })} />
            ))}
          </div>
        </Row>
        <Row title={t('settings.autostart')} hint={t('settings.autostartHint')}>
          <Switch
            on={autostart}
            onChange={async (v) => {
              await attempt(api.autostartSet(v))
              setAutostart(await api.autostartGet())
            }}
          />
        </Row>
        <Row title={t('settings.closeToTray')}>
          <Switch on={cfg.closeToTray} onChange={(v) => set({ closeToTray: v })} />
        </Row>
        {info && (
          <Row title={t('settings.renameVault')}>
            <input
              className="input"
              style={{ width: 200, height: 34 }}
              value={vaultName}
              maxLength={80}
              onChange={(e) => setVaultName(e.target.value)}
              onBlur={async () => {
                if (vaultName.trim() && vaultName !== info.name) {
                  const v = await attempt(api.vaultRename(vaultName))
                  if (v) setState({ info: v })
                }
              }}
            />
          </Row>
        )}
      </>
    ),
    security: (
      <>
        <h3>{t('settings.security')}</h3>
        <Row title={t('settings.lockIdle')}>
          <Select
            value={cfg.lockIdleMin}
            onChange={(v) => set({ lockIdleMin: v })}
            options={[...[1, 5, 15, 30, 60].map((n) => ({ value: n, label: t('common.minutes', { n }) })), { value: 0, label: t('common.never') }]}
          />
        </Row>
        <Row title={t('settings.lockSession')}>
          <Switch on={cfg.lockOnSessionLock} onChange={(v) => set({ lockOnSessionLock: v })} />
        </Row>
        <Row title={t('settings.lockSleep')}>
          <Switch on={cfg.lockOnSleep} onChange={(v) => set({ lockOnSleep: v })} />
        </Row>
        <Row title={t('settings.lockMinimize')}>
          <Switch on={cfg.lockOnMinimize} onChange={(v) => set({ lockOnMinimize: v })} />
        </Row>
        <Row title={t('settings.clipboard')} hint={t('settings.clipboardHint')}>
          <Select
            value={cfg.clipboardSecs}
            onChange={(v) => set({ clipboardSecs: v })}
            options={[...[10, 20, 30, 60].map((n) => ({ value: n, label: t('common.seconds', { n }) })), { value: 0, label: t('common.never') }]}
          />
        </Row>
        {info && (
          <>
            <Row title={t('settings.hello')} hint={hello === false ? t('settings.helloUnavailable') : t('settings.helloHint')}>
              <Switch
                on={info.helloEnabled}
                onChange={async (v) => {
                  if (hello === false) return
                  try {
                    await (v ? api.helloEnable() : api.helloDisable())
                    setState({ info: await api.vaultInfo() })
                    if (v) toast(t('hello.enabled'))
                  } catch (e) {
                    fail(e)
                  }
                }}
              />
            </Row>
            <Row title={t('settings.changePassword')}>
              <button className="btn btn-sm" onClick={() => setShowPw(!showPw)}>
                {t('common.change')}
              </button>
            </Row>
            {showPw && <ChangePassword />}
            <Row title={t('settings.regenerateKit')} hint={t('kit.regenerateWarn')}>
              <button
                className="btn btn-sm"
                onClick={() =>
                  setGate(() => async () => {
                    const w = await attempt(api.recoveryRegenerate())
                    if (w) {
                      setKitWords(w)
                      setState({ kit: { words: w, vault: info.name, date: Date.now() / 1000 } })
                    }
                  })
                }
              >
                {t('kit.another')}
              </button>
            </Row>
          </>
        )}
      </>
    ),
    autotype: (
      <>
        <h3>{t('settings.autotype')}</h3>
        <Row title={t('settings.quickEnabled')} hint={t('settings.quickHint')}>
          <Switch on={cfg.quickEnabled} onChange={(v) => set({ quickEnabled: v })} />
        </Row>
        <Row title={t('settings.quickHotkey')} hint={keyHint(cfg.quickHotkey)} bad={busyKeys.includes(cfg.quickHotkey)}>
          <HotkeyRecorder value={cfg.quickHotkey} onChange={(v) => set({ quickHotkey: v })} />
        </Row>
        <Row title={t('settings.autotypeEnabled')}>
          <Switch on={cfg.autotypeEnabled} onChange={(v) => set({ autotypeEnabled: v })} />
        </Row>
        <Row title={t('settings.autotypeHotkey')} hint={keyHint(cfg.autotypeHotkey, t('settings.autotypeHotkeyHint'))} bad={busyKeys.includes(cfg.autotypeHotkey)}>
          <HotkeyRecorder value={cfg.autotypeHotkey} onChange={(v) => set({ autotypeHotkey: v })} />
        </Row>
        <div className="field-group" style={{ marginTop: 14 }}>
          <label className="label">{t('settings.autotypeSequence')}</label>
          <input
            className="input mono small"
            defaultValue={cfg.autotypeSequence}
            spellCheck={false}
            maxLength={500}
            onBlur={(e) => e.target.value.trim() && set({ autotypeSequence: e.target.value.trim() })}
          />
          <div className="hint">{t('editor.autotypeHint')}</div>
        </div>
      </>
    ),
    browser: (
      <>
        <h3>{t('settings.browser')}</h3>
        <Row title={t('settings.bridge')} hint={t('settings.bridgeHint')}>
          <Switch on={cfg.bridgeEnabled} onChange={(v) => set({ bridgeEnabled: v })} />
        </Row>
        <Install />
        <h3 style={{ marginTop: 22 }}>{t('settings.browsers')}</h3>
        <Browsers />
      </>
    ),
    data: (
      <>
        <h3>{t('settings.data')}</h3>
        <Row title={t('settings.import')} hint={t('settings.importHint')}>
          <button className="btn btn-sm" disabled={!info} onClick={() => void doImport()}>
            {t('settings.import')}
          </button>
        </Row>
        <Row title={t('settings.exportCsv')} hint={t('settings.exportCsvWarn')}>
          <button
            className="btn btn-sm btn-danger"
            disabled={!info}
            onClick={() =>
              setGate(() => async () => {
                const p = await pickFile({ mode: 'save', title: t('settings.exportCsvTitle'), exts: ['csv'], ext: 'csv', name: 'poxipass-export.csv' })
                if (p && (await attempt(api.exportCsv(p))) !== undefined) toast(t('settings.exported'))
              })
            }
          >
            CSV
          </button>
        </Row>
        <Row title={t('settings.exportCopy')}>
          <button
            className="btn btn-sm"
            disabled={!info}
            onClick={async () => {
              const p = await pickFile({ mode: 'save', title: t('settings.exportCopyTitle'), exts: ['poxipass'], ext: 'poxipass', name: `${info?.name ?? 'copia'}.poxipass` })
              if (p && (await attempt(api.exportCopy(p))) !== undefined) toast(t('settings.exported'))
            }}
          >
            {t('common.save')}
          </button>
        </Row>
        <Row title={t('settings.backups')} hint={t('settings.backupsHint')}>
          <button
            className="btn btn-sm"
            disabled={!info}
            onClick={async () => {
              const b = await attempt(api.backupsList())
              if (b) setBackups(b)
            }}
          >
            <History size={14} /> {t('settings.backupsOpen')}
          </button>
        </Row>
        <Row title={t('settings.extraBackup')} hint={cfg.extraBackupDir ? t('settings.extraBackupOn', { d: cfg.extraBackupDir }) : t('settings.extraBackupHint')}>
          <div className="row">
            {cfg.extraBackupDir && (
              <button className="btn btn-sm btn-ghost" onClick={() => set({ extraBackupDir: null })}>
                {t('editor.remove')}
              </button>
            )}
            <button
              className="btn btn-sm"
              onClick={async () => {
                const d = await pickFile({ mode: 'folder', title: t('settings.extraBackupPick'), exts: [] })
                if (d) set({ extraBackupDir: d })
              }}
            >
              {cfg.extraBackupDir ? t('common.change') : t('settings.extraBackupPick')}
            </button>
          </div>
        </Row>
        <Row title={t('settings.favicons')} hint={t('settings.faviconsHint')}>
          <Switch on={cfg.favicons} onChange={(v) => set({ favicons: v })} />
        </Row>
        {cfg.favicons && info && (
          <Row title={t('settings.fetchMissing')}>
            <button
              className="btn btn-sm"
              onClick={async () => {
                const n = await attempt(api.iconsFetchMissing())
                if (n !== undefined) {
                  toast(t('settings.fetched', { n }))
                  await refreshEntries()
                }
              }}
            >
              {t('settings.fetchMissing')}
            </button>
          </Row>
        )}
      </>
    ),
    about: (
      <>
        <h3>{t('settings.about')}</h3>
        <Row title="PoxiPass" hint={t('settings.version', { v: app?.version ?? '' })}>
          <button
            className="btn btn-sm"
            onClick={async () => {
              if (app?.portable) return void openUrl(RELEASES)
              if (!(await checkForUpdates(true))) toast(t('settings.upToDate'), 'info')
            }}
          >
            {t('settings.checkUpdates')}
          </button>
        </Row>
        <Row title={t('settings.whatsNew')}>
          <button className="btn btn-sm" onClick={() => window.dispatchEvent(new CustomEvent('poxi:whatsnew'))}>
            {t('settings.whatsNew')}
          </button>
        </Row>
        <Row title={t('settings.github')} hint="github.com/PoxiiTV/PoxiPass">
          <button className="btn btn-sm" onClick={() => void openUrl('https://github.com/PoxiiTV/PoxiPass')}>
            GitHub
          </button>
        </Row>
        <Row title={t('settings.license')} />
        <Row title={t('settings.quit')}>
          <button className="btn btn-sm btn-danger" onClick={() => void api.quit()}>
            {t('settings.quit')}
          </button>
        </Row>
      </>
    )
  }

  return (
    <>
      <Modal onClose={close} className="settings">
        <nav>
          <h2>{t('settings.title')}</h2>
          {CATS.map(([c, Icon]) => (
            <button key={c} className={`nav-item ${cat === c ? 'active' : ''}`} onClick={() => setCat(c)}>
              <Icon size={17} />
              <span className="name">{t(`settings.${c}`)}</span>
            </button>
          ))}
        </nav>
        <div className="body">
          <div className="row" style={{ justifyContent: 'flex-end', marginBottom: -30 }}>
            <button className="icon-btn" onClick={close} aria-label={t('common.close')}>
              <X size={17} />
            </button>
          </div>
          {body[cat]}
        </div>
      </Modal>
      {gate && (
        <PasswordGate
          onClose={() => setGate(null)}
          onOk={() => {
            const fn = gate
            setGate(null)
            fn()
          }}
        />
      )}
      {kitWords && (
        // Sin cerrar con Esc ni clic fuera: el kit anterior ya no vale y este no se puede perder
        <div className="modal-backdrop kit-layer">
          {kitConfirm ? (
            <KitConfirm
              words={kitWords}
              onBack={() => setKitConfirm(false)}
              onDone={() => {
                setKitWords(null)
                setKitConfirm(false)
                setState({ kit: null })
                toast(t('kit.newSaved'))
              }}
            />
          ) : (
            <KitView
              words={kitWords}
              onAnother={async () => {
                const w = await attempt(api.recoveryRegenerate())
                if (w) setKitWords(w)
              }}
              onNext={() => setKitConfirm(true)}
            />
          )}
        </div>
      )}
      {backups && (
        <Modal wide onClose={() => setBackups(null)}>
          <h2>{t('settings.backups')}</h2>
          <p className="small">{t('settings.backupsHint')}</p>
          {backups.length === 0 && <div className="empty small">{t('settings.backupsEmpty')}</div>}
          <div className="col" style={{ gap: 4 }}>
            {backups.map((b) => (
              <div key={`${b.extra}-${b.name}`} className="att">
                <History size={15} className="muted" />
                <span className="name">{date(b.modified)}</span>
                <span className="muted small">{ago(b.modified)}</span>
                {b.extra && <span className="chip">{t('restore.extra')}</span>}
                <span className="muted small num">{bytes(b.size)}</span>
                <button
                  className="btn btn-sm"
                  onClick={async () => {
                    if (!(await ask(t('settings.restoreConfirm', { d: date(b.modified) }), t('settings.restore')))) return
                    setBackups(null)
                    await attempt(api.backupRestore(b.name, b.extra))
                  }}
                >
                  {t('settings.restore')}
                </button>
              </div>
            ))}
          </div>
        </Modal>
      )}
      {kdbxPath && (
        <Modal onClose={() => setKdbxPath(null)}>
          <h2>{t('settings.importKdbx')}</h2>
          <p>{t('settings.importKdbxHint')}</p>
          <div className="field-group">
            <PasswordInput
              value={importKey.password}
              onChange={(password) => setImportKey({ ...importKey, password })}
              autoFocus
              bad={!!kdbxErr}
              onEnter={() => void openKdbx()}
            />
          </div>
          <div className="field-group">
            <label className="label">{t('settings.importKeyfile')}</label>
            <div className="row">
              <button
                className="btn btn-sm"
                onClick={async () => {
                  const k = await pickFile({ mode: 'open', title: t('settings.importKeyfileTitle'), exts: [] })
                  if (k) setImportKey({ ...importKey, keyfile: k })
                }}
              >
                {t('settings.importKeyfilePick')}
              </button>
              {importKey.keyfile && (
                <>
                  <span className="muted small" style={{ minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{importKey.keyfile.split(/[\/]/).pop()}</span>
                  <button className="icon-btn" onClick={() => setImportKey({ ...importKey, keyfile: null })} aria-label={t('editor.remove')} data-tip={t('editor.remove')}>
                    <X size={15} />
                  </button>
                </>
              )}
            </div>
          </div>
          {kdbxErr && <div className="hint danger-text">{kdbxErr}</div>}
          <div className="modal-actions">
            <button className="btn btn-ghost" onClick={() => setKdbxPath(null)}>
              {t('common.cancel')}
            </button>
            <button className="btn btn-primary" disabled={kdbxBusy || (!importKey.password && !importKey.keyfile)} onClick={() => void openKdbx()}>
              {t('settings.importOpen')}
            </button>
          </div>
        </Modal>
      )}
      {preview && (
        <Modal onClose={closeImport}>
          <h2>{t('settings.import')}</h2>
          <p>
            {preview.files
              ? t('settings.importPreviewFiles', { format: preview.format, n: preview.count, m: preview.files })
              : t('settings.importPreview', { format: preview.format, n: preview.count })}
          </p>
          <div className="chips">
            {preview.sample.map((s, i) => (
              <span className="chip" key={i}>
                {s}
              </span>
            ))}
          </div>
          <div className="modal-actions">
            <button className="btn btn-ghost" onClick={closeImport}>
              {t('common.cancel')}
            </button>
            <button
              className="btn btn-primary"
              disabled={!preview.count}
              onClick={async () => {
                const { plain } = preview
                const key = importKey
                closeImport()
                const r = await attempt(api.importRun(key))
                if (r === undefined) return
                toast(r.skipped ? t('settings.importedSkipped', { n: r.added, m: r.skipped }) : t('settings.imported', { n: r.added }))
                if (r.filesSkipped) toast(t('settings.importFilesSkipped', { m: r.filesSkipped }), 'info', 6)
                await refreshEntries()
                if (plain && (await ask(t('settings.importDelete'), t('settings.importDeleteYes')))) {
                  if ((await attempt(api.importDeleteSource())) !== undefined) toast(t('settings.importDeleted'))
                }
              }}
            >
              {t('settings.importDo', { n: preview.count })}
            </button>
          </div>
        </Modal>
      )}
    </>
  )
}
