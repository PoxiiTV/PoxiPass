// Genera src-tauri/src/recovery/wordlist.txt (la Lista Poxi) a partir de las candidatas castizas,
// rellenando hasta 2048 con la lista BIP39 en español. Reglas: de 3 a 10 letras, prefijo de 4 letras único
// (sin tildes) y nada de «maricón» ni «z» (son los comodines). Uso: node scripts/wordlist.mjs <bip39-es.txt>
import { readFileSync, writeFileSync } from 'node:fs'

const norm = (w) => w.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '')
const key = (w) => norm(w).slice(0, 4)
const ok = (w) => /^[a-zñáéíóúü]+$/.test(w) && norm(w).length >= 3 && norm(w).length <= 10 && !['maricon', 'z'].includes(norm(w))

const candidates = readFileSync(new URL('./wordlist-candidatas.txt', import.meta.url), 'utf8')
  .split('\n')
  .filter((l) => !l.startsWith('#'))
  .flatMap((l) => l.trim().split(/\s+/))
  .filter(Boolean)
  .map((w) => w.toLowerCase())
const filler = readFileSync(process.argv[2], 'utf8').split('\n').map((w) => w.trim()).filter(Boolean)

const taken = new Map()
let castizas = 0
for (const [list, castiza] of [[candidates, true], [filler, false]]) {
  for (const w of list) {
    if (taken.size === 2048) break
    if (!ok(w) || taken.has(key(w))) continue
    taken.set(key(w), w)
    if (castiza) castizas++
  }
}
if (taken.size !== 2048) throw new Error(`Solo hay ${taken.size} palabras`)

const words = [...taken.values()].sort((a, b) => norm(a).localeCompare(norm(b)))
writeFileSync(new URL('../src-tauri/src/recovery/wordlist.txt', import.meta.url), words.join('\n') + '\n')
console.log(`Lista Poxi: ${words.length} palabras (${castizas} castizas, ${words.length - castizas} de relleno)`)
