import { useEffect, useState } from 'react'
import { ShieldAlert, ShieldCheck } from 'lucide-react'
import { api, type Report } from '../lib/api'
import { refreshEntries, setState, toast, useStore } from '../lib/store'
import { attempt } from '../lib/actions'
import { ago } from '../lib/format'
import { useT } from '../i18n'
import { Avatar } from '../components/ui'

type Cat = 'weak' | 'reused' | 'old' | 'no2fa' | 'breached' | 'duplicates' | 'expiring'

export function Health(): React.JSX.Element {
  const t = useT()
  const entries = useStore((s) => s.entries)
  const [r, setR] = useState<Report | null>(null)
  const [cat, setCat] = useState<Cat>('breached')
  const [checking, setChecking] = useState(false)

  useEffect(() => {
    void attempt(api.auditReport()).then((x) => x && setR(x))
  }, [entries])

  if (!r) return <div />

  const byId = new Map(entries.map((e) => [e.id, e]))
  const color = r.score >= 80 ? 'var(--s4)' : r.score >= 60 ? 'var(--s3)' : r.score >= 40 ? 'var(--s2)' : 'var(--s0)'
  const R = 48
  const len = 2 * Math.PI * R

  const groups: Record<Cat, string[][]> = {
    breached: r.breached.map(([id]) => [id]),
    weak: r.weak.map((id) => [id]),
    reused: r.reused,
    old: r.old.map((id) => [id]),
    no2fa: r.no2fa.map((id) => [id]),
    duplicates: r.duplicates,
    expiring: r.expiring.map(([id]) => [id])
  }
  const counts: Record<Cat, number> = {
    breached: r.breached.length,
    weak: r.weak.length,
    reused: r.reused.reduce((n, g) => n + g.length, 0),
    old: r.old.length,
    no2fa: r.no2fa.length,
    duplicates: r.duplicates.length,
    expiring: r.expiring.length
  }
  const tone: Record<Cat, string> = { breached: 'bad', weak: 'bad', reused: 'warn', old: 'warn', no2fa: 'warn', duplicates: 'warn', expiring: 'bad' }
  const breachCount = new Map(r.breached)
  const expDays = new Map(r.expiring)

  const open = (id: string): void => setState({ section: 'all', selected: id, query: '', editing: null })

  return (
    <div className="page-inner">
      <h1>{t('health.title')}</h1>
      {r.total === 0 ? (
        <div className="empty">
          <ShieldCheck size={40} />
          <span>{t('health.empty')}</span>
        </div>
      ) : (
        <>
          <div className="panel score">
            <div className="score-ring">
              <svg width="110" height="110" viewBox="0 0 110 110">
                <circle className="bg" cx="55" cy="55" r={R} />
                <circle className="fg" cx="55" cy="55" r={R} stroke={color} strokeDasharray={len} strokeDashoffset={len * (1 - r.score / 100)} />
              </svg>
              <b className="num" style={{ color }}>
                {r.score}
              </b>
            </div>
            <div style={{ flex: 1 }}>
              <div className="muted small">{t('health.of', { n: r.total })}</div>
              <p className="small muted" style={{ lineHeight: 1.5 }}>
                {t('health.checkHint')}
              </p>
              <div className="row">
                <button
                  className="btn btn-primary"
                  disabled={checking}
                  onClick={async () => {
                    setChecking(true)
                    const x = await attempt(api.hibpCheck())
                    if (x) setR(x)
                    setChecking(false)
                  }}
                >
                  <ShieldAlert size={16} /> {checking ? t('health.checking') : t('health.check')}
                </button>
                <span className="muted small">{r.breachChecked ? t('health.lastCheck', { d: ago(r.breachChecked) }) : t('health.neverChecked')}</span>
              </div>
            </div>
          </div>
          <div className="issues">
            {(['breached', 'weak', 'reused', 'duplicates', 'expiring', 'old', 'no2fa'] as Cat[]).map((c) => (
              <button key={c} className={`issue ${counts[c] ? tone[c] : 'ok'} ${cat === c ? 'on' : ''}`} onClick={() => setCat(c)}>
                <b className="num">{counts[c]}</b>
                <span>{t(`health.${c}`)}</span>
              </button>
            ))}
          </div>
          <div className="panel" style={{ marginTop: 16 }}>
            {cat === 'no2fa' && <div className="hint" style={{ marginTop: 0, marginBottom: 10 }}>{t('health.no2faHint')}</div>}
            {groups[cat].length === 0 ? (
              <div className="empty" style={{ padding: 20 }}>
                <ShieldCheck size={30} />
                <b>{t('health.allGood')}</b>
              </div>
            ) : (
              groups[cat].map((g, i) => (
                <div key={i} style={{ marginBottom: cat === 'reused' || cat === 'duplicates' ? 12 : 0 }}>
                  {cat === 'reused' && <div className="label">{t('health.group', { n: g.length })}</div>}
                  {cat === 'duplicates' && (
                    <div className="row" style={{ marginBottom: 4 }}>
                      <span className="label" style={{ margin: 0, flex: 1 }}>
                        {t('health.dupGroup', { n: g.length })}
                      </span>
                      <button
                        className="btn btn-sm"
                        onClick={async () => {
                          // Se queda la modificada más recientemente; el resto, a la papelera
                          const sorted = g.map((id) => byId.get(id)).filter((x) => !!x).sort((a, b) => b!.modified - a!.modified)
                          const keep = sorted[0]!.id
                          const n = await attempt(api.duplicatesMerge(keep, g.filter((id) => id !== keep)))
                          if (n !== undefined) {
                            toast(t('health.merged', { n }))
                            await refreshEntries()
                          }
                        }}
                      >
                        {t('health.merge')}
                      </button>
                    </div>
                  )}
                  {g.map((id) => {
                    const e = byId.get(id)
                    if (!e) return null
                    return (
                      <button key={id} className="row-entry" onClick={() => open(id)}>
                        <Avatar name={e.title} icon={e.icon} />
                        <span className="txt">
                          <b>{e.title}</b>
                          <small>{e.subtitle}</small>
                        </span>
                        {cat === 'breached' && <span className="chip bad">×{(breachCount.get(id) ?? 0).toLocaleString()}</span>}
                        {cat === 'expiring' && (
                          <span className="chip bad">
                            {(expDays.get(id) ?? 0) < 0 ? t('health.expired') : t('health.expiresIn', { n: expDays.get(id) ?? 0 })}
                          </span>
                        )}
                      </button>
                    )
                  })}
                </div>
              ))
            )}
          </div>
        </>
      )}
    </div>
  )
}
