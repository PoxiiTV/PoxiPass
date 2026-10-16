// Piezas visuales compartidas por el popup y lo que se pinta dentro de las páginas (menú y barra de guardar):
// los mismos tokens que la app (azul marino + acento, cristal) y un mini creador de elementos.

import type { Info } from './protocol'

/** El logo de la app, el mismo archivo */
export { default as LOGO } from '../../src/assets/logo.svg?raw'

// Iconos de lucide (los mismos de la app)
const svg = (d: string): string =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${d}</svg>`
export const ICON = {
  key: svg('<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/>'),
  lock: svg('<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>'),
  card: svg('<rect width="20" height="14" x="2" y="5" rx="2"/><line x1="2" x2="22" y1="10" y2="10"/>'),
  user: svg('<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>'),
  wand: svg(
    '<path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72"/><path d="m14 7 3 3"/><path d="M5 6v4"/><path d="M19 14v4"/><path d="M10 2v2"/><path d="M7 8H3"/><path d="M21 16h-4"/><path d="M11 3H9"/>'
  ),
  shield: svg(
    '<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/><path d="m9 12 2 2 4-4"/>'
  ),
  alert: svg('<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>'),
  copy: svg('<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>'),
  refresh: svg('<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/>'),
  x: svg('<path d="M18 6 6 18"/><path d="m6 6 12 12"/>'),
  external: svg('<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>')
}

export const CSS = `
.pp {
  --font: 'PoxiPass Nunito', 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
  --mono: 'Cascadia Mono', Consolas, ui-monospace, monospace;
  --ease-out: cubic-bezier(0.23, 1, 0.32, 1);
  --accent: #3b82f6; --accent-2: #38bdf8; --accent-rgb: 59, 130, 246;
  --warning: #f59e0b; --success: #22c55e; --danger: #ef4444;
  font: 500 13.5px/1.4 var(--font);
  color: var(--text);
  letter-spacing: 0;
  text-align: left;
  -webkit-font-smoothing: antialiased;
}
.pp, .pp * { box-sizing: border-box; }
.pp[data-theme='dark'] {
  color-scheme: dark;
  --glass: rgba(14, 19, 31, 0.86); --glass-strong: rgba(16, 22, 36, 0.97);
  --surface: rgba(255, 255, 255, 0.045); --surface-2: rgba(255, 255, 255, 0.075); --surface-3: rgba(255, 255, 255, 0.11);
  --border: rgba(255, 255, 255, 0.08); --border-strong: rgba(255, 255, 255, 0.14);
  --text: #f1f5fb; --text-2: #b4c0d4; --muted: #7f8ca3;
  --shadow: 0 22px 48px -14px rgba(0, 0, 0, 0.75), 0 2px 8px rgba(0, 0, 0, 0.35);
}
.pp[data-theme='light'] {
  color-scheme: light;
  --glass: rgba(255, 255, 255, 0.86); --glass-strong: rgba(255, 255, 255, 0.98);
  --surface: rgba(255, 255, 255, 0.6); --surface-2: rgba(20, 40, 80, 0.06); --surface-3: #dde5f1;
  --border: rgba(20, 40, 80, 0.09); --border-strong: rgba(20, 40, 80, 0.16);
  --text: #121a2b; --text-2: #3f4b62; --muted: #6c7890;
  --shadow: 0 22px 48px -18px rgba(30, 50, 100, 0.35), 0 2px 8px rgba(30, 50, 100, 0.1);
}
.pp[data-accent='violet'] { --accent: #8b5cf6; --accent-2: #c084fc; --accent-rgb: 139, 92, 246; }
.pp[data-accent='emerald'] { --accent: #10b981; --accent-2: #5eead4; --accent-rgb: 16, 185, 129; }
.pp[data-accent='pink'] { --accent: #ec4899; --accent-2: #f9a8d4; --accent-rgb: 236, 72, 153; }
.pp[data-accent='amber'] { --accent: #f59e0b; --accent-2: #fcd34d; --accent-rgb: 245, 158, 11; }
.pp[data-accent='red'] { --accent: #ef4444; --accent-2: #fb7185; --accent-rgb: 239, 68, 68; }
.pp svg { width: 16px; height: 16px; flex: none; }
/* Sin especificidad: así .btn-link, .btn-primary… mandan */
:where(.pp) button { font: inherit; color: inherit; margin: 0; }

.glass {
  background: var(--glass);
  backdrop-filter: blur(20px) saturate(1.5);
  -webkit-backdrop-filter: blur(20px) saturate(1.5);
  border: 1px solid var(--border-strong);
  border-radius: 14px;
  box-shadow: var(--shadow);
}
.head { display: flex; align-items: center; gap: 8px; padding: 4px 6px 8px; color: var(--muted); font-size: 11.5px; font-weight: 800; letter-spacing: 0.02em; }
.head .logo svg { width: 18px; height: 18px; border-radius: 5px; display: block; }
.head .grow { flex: 1; }
.item {
  display: flex; align-items: center; gap: 10px; width: 100%;
  padding: 7px 8px; border: 0; border-radius: 10px; background: transparent;
  cursor: pointer; text-align: left;
  transition: background-color 0.12s ease, transform 0.12s var(--ease-out);
}
.item:hover, .item.active { background: var(--surface-3); }
.item:active { transform: scale(0.985); }
.item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.av {
  width: 30px; height: 30px; border-radius: 9px; flex: none;
  display: grid; place-items: center;
  background: rgba(var(--accent-rgb), 0.16); color: var(--accent);
  font-weight: 800; font-size: 14px;
}
.av svg { width: 15px; height: 15px; }
.txt { flex: 1; min-width: 0; }
.t, .s { display: block; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.t { font-weight: 700; color: var(--text); }
.s { color: var(--muted); font-size: 12px; }
.mono { font-family: var(--mono); }
.note { display: flex; gap: 8px; align-items: flex-start; padding: 6px 8px; color: var(--text-2); font-size: 12.5px; }
.note.warn { color: var(--warning); }
.note.err { color: var(--danger); }
.note svg { width: 15px; height: 15px; margin-top: 1px; }
.code { display: flex; gap: 6px; justify-content: center; padding: 6px 0 4px; }
.code span {
  width: 34px; height: 42px; display: grid; place-items: center;
  border-radius: 10px; background: var(--surface-2); border: 1px solid var(--border-strong);
  font: 800 20px var(--mono); color: var(--text);
}
.btn {
  display: inline-flex; align-items: center; justify-content: center; gap: 6px;
  height: 32px; padding: 0 12px; border-radius: 10px; cursor: pointer;
  border: 1px solid var(--border-strong); background: var(--surface-2); color: var(--text);
  font-weight: 700; font-size: 13px; white-space: nowrap;
  transition: background-color 0.15s ease, transform 0.12s var(--ease-out), filter 0.15s ease;
}
.btn:hover { background: var(--surface-3); }
.btn:active { transform: scale(0.97); }
.btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.btn-primary { border-color: transparent; color: #fff; background: linear-gradient(135deg, var(--accent), var(--accent-2)); box-shadow: 0 6px 16px -6px rgba(var(--accent-rgb), 0.6); }
.btn-primary:hover { background: linear-gradient(135deg, var(--accent), var(--accent-2)); filter: brightness(1.08); }
.btn-ghost { background: transparent; border-color: transparent; color: var(--text-2); }
.btn-ghost:hover { background: var(--surface-2); }
.btn-link { background: none; border: 0; padding: 0; height: auto; color: var(--accent); font-weight: 700; cursor: pointer; }
.icon-btn {
  width: 28px; height: 28px; display: grid; place-items: center; flex: none;
  border: 0; border-radius: 8px; background: transparent; color: var(--muted); cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease, transform 0.12s var(--ease-out);
}
.icon-btn:hover { background: var(--surface-2); color: var(--text); }
.icon-btn:active { transform: scale(0.94); }
.icon-btn:focus-visible { outline: 2px solid var(--accent); }
@keyframes pp-in { from { opacity: 0; transform: translateY(-4px) scale(0.98); } }
@keyframes pp-spin { to { transform: rotate(360deg); } }
.spin { width: 16px; height: 16px; border-radius: 50%; border: 2px solid var(--surface-3); border-top-color: var(--accent); animation: pp-spin 0.7s linear infinite; }
@media (prefers-reduced-motion: reduce) {
  .pp *, .pp { animation: none !important; transition: none !important; }
}
`

/** Crea un elemento: `h('div', { class: 'x', onclick }, hijo, 'texto')`. `html` mete un SVG nuestro (nunca datos). */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, unknown> = {},
  ...kids: (Node | string | null | false | undefined)[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag)
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue
    if (k === 'html') el.innerHTML = String(v)
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v as EventListener)
    else el.setAttribute(k, String(v))
  }
  for (const c of kids) if (c) el.append(c)
  return el
}

/** Tema y acento de la app (con «Como Windows», el del sistema) */
export function applyTheme(root: HTMLElement, info: Info | null): void {
  const theme = info?.theme ?? 'system'
  const dark = theme === 'dark' || (theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
  root.dataset.theme = dark ? 'dark' : 'light'
  root.dataset.accent = info?.accent ?? 'blue'
}

export function initial(s: string): string {
  return (s.trim()[0] ?? '?').toUpperCase()
}

let fontLoaded = false
/** Nunito, la letra de la app. En las páginas se añade a `document.fonts` (el shadow DOM no carga @font-face). */
export function loadFont(): void {
  if (fontLoaded) return
  fontLoaded = true
  try {
    const f = new FontFace('PoxiPass Nunito', `url(${chrome.runtime.getURL('nunito.woff2')})`, { weight: '200 1000' })
    document.fonts.add(f)
    void f.load().catch(() => {})
  } catch {
    // Sin la letra se usa Segoe UI
  }
}
