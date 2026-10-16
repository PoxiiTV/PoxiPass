import type { EntrySummary } from './api'
import { norm } from './format'

/**
 * Búsqueda difusa: «net» → Netflix, «nfx» también (letras en orden), y sin tildes ni mayúsculas.
 * Puntúa: empieza por > empieza una palabra > contiene > en orden; y suben las más usadas.
 */
function score(e: EntrySummary, q: string): number {
  const title = norm(e.title)
  const other = norm(`${e.subtitle} ${e.url} ${e.tags.join(' ')}`)
  let s = 0
  if (title.startsWith(q)) s = 100
  else if (title.split(/[\s\-_.@]+/).some((w) => w.startsWith(q))) s = 85
  else if (title.includes(q)) s = 70
  else if (other.includes(q)) s = 50
  else {
    // Letras en orden (con huecos): cuanto más juntas, mejor
    let pos = 0
    let gaps = 0
    for (const [i, c] of [...q].entries()) {
      const at = title.indexOf(c, pos)
      if (at < 0) return 0
      if (i > 0) gaps += at - pos
      pos = at + 1
    }
    s = Math.max(10, 40 - gaps * 2)
  }
  // Las que usas a menudo, un poco más arriba
  if (e.used) s += Math.max(0, 12 - (Date.now() / 1000 - e.used) / 86_400 / 3)
  if (e.favorite) s += 4
  return s
}

export function search(entries: EntrySummary[], query: string): EntrySummary[] {
  const q = norm(query.trim())
  if (!q) return entries
  return entries
    .map((e) => [e, score(e, q)] as const)
    .filter(([, s]) => s > 0)
    .sort((a, b) => b[1] - a[1] || a[0].title.localeCompare(b[0].title))
    .map(([e]) => e)
}

/** Sin búsqueda: las más usadas primero (acceso rápido) */
export function byUse(entries: EntrySummary[]): EntrySummary[] {
  return [...entries].sort((a, b) => (b.used ?? 0) - (a.used ?? 0) || Number(b.favorite) - Number(a.favorite) || a.title.localeCompare(b.title))
}
