import { currentLang, t, translateList } from '../i18n'

/** Sin tildes ni mayúsculas, para buscar */
export const norm = (s: string): string =>
  s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')

/** Duración legible: 90 → «2 minutos» */
export function duration(secs: number): string {
  const units: [number, string][] = [
    [60, 'seconds'],
    [3600, 'minutes'],
    [86_400, 'hours'],
    [2_592_000, 'days'],
    [31_536_000, 'months'],
    [3_153_600_000, 'years']
  ]
  if (!isFinite(secs) || secs >= 3_153_600_000) return t('time.centuries')
  let prev = 1
  for (const [limit, key] of units) {
    if (secs < limit) return t(`time.${key}`, { n: Math.max(1, Math.round(secs / prev)) })
    prev = limit
  }
  return t('time.centuries')
}

export function crackTime(secs: number): string {
  return secs < 1 ? t('strength.instant') : duration(secs)
}

export function ago(unix: number): string {
  const d = Date.now() / 1000 - unix
  return d < 45 ? t('time.now') : t('time.ago', { t: duration(d) })
}

export function date(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(currentLang() === 'es' ? 'es-ES' : 'en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric'
  })
}

export function bytes(n: number): string {
  const f = new Intl.NumberFormat(currentLang() === 'es' ? 'es-ES' : 'en-GB', { maximumFractionDigits: 1 })
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${f.format(n / 1024)} KB`
  return `${f.format(n / 1024 / 1024)} MB`
}

export function strengthLabel(score: number): string {
  return translateList(currentLang(), 'strength.label')[score] ?? ''
}

/** Color estable a partir de un texto (avatares) */
export function hue(s: string): number {
  let h = 0
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0
  return h % 360
}

export function host(url: string): string {
  const rest = url.includes('://') ? url.split('://')[1] : url
  return (rest.split(/[/?#:]/)[0] ?? '').replace(/^www\./, '')
}

/** ¿Lo escrito es una web? (`netflix.com`, `https://…`) */
export const looksLikeUrl = (v: string): boolean => /^https?:\/\//i.test(v.trim()) || /^[^\s@/]+\.[a-z]{2,}(\/\S*)?$/i.test(v.trim())

export const fullUrl = (v: string): string => (/^[a-z][a-z0-9+.-]*:\/\//i.test(v.trim()) ? v.trim() : `https://${v.trim()}`)

/** Nombre bonito de una web: `https://www.netflix.com/login` → «Netflix», `accounts.google.co.uk` → «Google» */
export function siteTitle(url: string): string {
  const h = host(url).toLowerCase()
  if (/^[\d.]+$/.test(h) || h.includes(':') || !h.includes('.')) return h
  const parts = h.split('.').filter(Boolean)
  const n = parts.length
  const core = n >= 3 && parts[n - 2].length <= 3 && parts[n - 1].length === 2 ? parts[n - 3] : n >= 2 ? parts[n - 2] : (parts[0] ?? '')
  return core.charAt(0).toUpperCase() + core.slice(1)
}
