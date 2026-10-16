import { useCallback, useEffect, useState } from 'react'
import { ArrowLeft, Copy, Eye, EyeOff, History, QrCode, RefreshCw, Type } from 'lucide-react'
import { api, type GenOptions } from '../lib/api'
import { getState, setState, useStore } from '../lib/store'
import { attempt, copyText } from '../lib/actions'
import { useT } from '../i18n'
import { ago } from '../lib/format'
import { Colored, StrengthBar, Switch, useStrength } from '../components/ui'
import { Range, Select } from '../components/controls'

export function Generator({ standalone }: { standalone?: boolean }): React.JSX.Element {
  const t = useT()
  const saved = useStore((s) => s.config?.generator)
  const [o, setO] = useState<GenOptions | null>(saved ?? null)
  const [out, setOut] = useState<{ value: string; bits: number } | null>(null)
  const s = useStrength(out?.value ?? '')
  const [history, setHistory] = useState<{ at: number; value: string }[]>([])
  const [shown, setShown] = useState<number | null>(null)

  useEffect(() => {
    void api.genHistory().then(setHistory)
  }, [])

  useEffect(() => {
    if (!o && saved) setO(saved)
  }, [saved, o])

  const run = useCallback(async (opts: GenOptions) => {
    const g = await attempt(api.generate(opts))
    if (g) setOut(g)
  }, [])

  useEffect(() => {
    if (o) void run(o)
  }, [o, run])

  if (!o) return <div />
  const set = (p: Partial<GenOptions>): void => setO({ ...o, ...p })
  const pass = o.mode === 'passphrase'
  const user = o.mode === 'username'

  return (
    <div className="page-inner">
      {standalone && (
        <button className="link muted" style={{ marginBottom: 10 }} onClick={() => setState({ screen: getState().unlockPath ? 'unlock' : 'welcome' })}>
          <ArrowLeft size={14} style={{ verticalAlign: -2 }} /> {t('common.back')}
        </button>
      )}
      <h1>{t('gen.title')}</h1>
      <div className="gen-out">
        <div className="value">{out && <Colored value={out.value} />}</div>
        <button className="btn btn-icon" data-tip={t('viewer.large')} aria-label={t('viewer.large')} onClick={() => out && setState({ viewer: { type: 'large', title: t('gen.title'), text: out.value } })}>
          <Type size={17} />
        </button>
        <button
          className="btn btn-icon"
          data-tip={t('viewer.qr')}
          aria-label={t('viewer.qr')}
          onClick={async () => {
            const svg = out && (await attempt(api.qrSvg(out.value)))
            if (svg) setState({ viewer: { type: 'qr', title: t('viewer.qr'), svg } })
          }}
        >
          <QrCode size={17} />
        </button>
        <button className="btn btn-icon" data-tip={t('gen.again')} aria-label={t('gen.again')} onClick={() => void run(o)}>
          <RefreshCw size={17} />
        </button>
        <button
          className="btn btn-primary"
          onClick={async () => {
            if (!out) return
            await copyText(out.value, true)
            setHistory(await api.genHistory())
          }}
        >
          <Copy size={16} /> {t('common.copy')}
        </button>
      </div>
      <StrengthBar s={s} bits={out?.bits} />
      <div className="panel" style={{ marginTop: 18 }}>
        <div className="segmented" style={{ marginBottom: 16 }}>
          <button className={!pass ? 'on' : ''} onClick={() => set({ mode: 'password' })}>
            {t('gen.password')}
          </button>
          <button className={pass ? 'on' : ''} onClick={() => set({ mode: 'passphrase' })}>
            {t('gen.passphrase')}
          </button>
          <button className={user ? 'on' : ''} onClick={() => set({ mode: 'username' })}>
            {t('gen.username')}
          </button>
        </div>
        {user ? (
          <div className="hint" style={{ marginTop: 0 }}>
            {t('gen.usernameHint')}
          </div>
        ) : !pass ? (
          <>
            <label className="label">{t('gen.length')}</label>
            <div className="slider-row">
              <Range min={4} max={128} value={o.length} label={t('gen.length')} onChange={(v) => set({ length: v })} />
              <b className="num">{o.length}</b>
            </div>
            <div className="grid2">
              {(['lower', 'upper', 'digits', 'symbols'] as const).map((k) => (
                <label key={k} className="check">
                  <input
                    type="checkbox"
                    checked={o[k]}
                    onChange={(e) => {
                      const next = { ...o, [k]: e.target.checked }
                      if (next.lower || next.upper || next.digits || next.symbols) setO(next)
                    }}
                  />
                  {t(`gen.${k}`)}
                </label>
              ))}
            </div>
            <label className="check">
              <input type="checkbox" checked={o.avoidAmbiguous} onChange={(e) => set({ avoidAmbiguous: e.target.checked })} />
              {t('gen.avoidAmbiguous')}
            </label>
            <div className="set-row">
              <div className="txt">
                <b>{t('gen.exclude')}</b>
                <small>{t('gen.excludeHint')}</small>
              </div>
              <input className="input mono" style={{ width: 160, height: 34 }} value={o.exclude} maxLength={60} spellCheck={false} onChange={(e) => set({ exclude: e.target.value })} />
            </div>
          </>
        ) : (
          <>
            <label className="label">{t('gen.words')}</label>
            <div className="slider-row">
              <Range min={3} max={12} value={o.words} label={t('gen.words')} onChange={(v) => set({ words: v })} />
              <b className="num">{o.words}</b>
            </div>
            <div className="set-row">
              <div className="txt">
                <b>{t('gen.separator')}</b>
              </div>
              <Select
                value={o.separator}
                onChange={(v) => set({ separator: v })}
                options={[
                  { value: '-', label: '-' },
                  { value: '.', label: '.' },
                  { value: '_', label: '_' },
                  { value: ' ', label: t('gen.space') },
                  { value: '', label: t('gen.none') }
                ]}
              />
            </div>
            <div className="set-row">
              <div className="txt">
                <b>{t('gen.capitalize')}</b>
              </div>
              <Switch on={o.capitalize} onChange={(v) => set({ capitalize: v })} label={t('gen.capitalize')} />
            </div>
            <div className="set-row">
              <div className="txt">
                <b>{t('gen.number')}</b>
              </div>
              <Switch on={o.number} onChange={(v) => set({ number: v })} label={t('gen.number')} />
            </div>
            <div className="hint">{t('gen.listHint')}</div>
          </>
        )}
      </div>
      {history.length > 0 && (
        <div className="panel">
          <div className="section-title" style={{ marginTop: 0 }}>
            <History size={14} /> {t('gen.history')}
          </div>
          <div className="hint" style={{ marginTop: -4, marginBottom: 10 }}>
            {t('gen.historyHint')}
          </div>
          {history.map((h, i) => (
            <div className="hist" key={h.at + h.value}>
              <span className="mono" style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {shown === i ? h.value : `${h.value.slice(0, 3)}${'•'.repeat(Math.min(12, Math.max(0, h.value.length - 3)))}`}
              </span>
              <span className="muted small">{ago(h.at)}</span>
              <button className="icon-btn" aria-label={t('common.show')} onClick={() => setShown(shown === i ? null : i)}>
                {shown === i ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
              <button className="icon-btn" aria-label={t('common.copy')} onClick={() => void copyText(h.value)}>
                <Copy size={15} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
