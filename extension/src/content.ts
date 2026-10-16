// En cada página (y frame): detecta los campos, pinta el menú propio bajo el campo enfocado, rellena solo tras un
// gesto de la persona y propone guardar lo que se envía. Todo lo visible va en un shadow DOM cerrado.

import { send, type CardData, type Creds, type Err, type FieldKind, type IdentityData, type Offer, type Pending, type Status } from './protocol'
import { applyTheme, CSS, h, ICON, initial, loadFont, LOGO } from './ui'
import { errText, setLang, t } from './i18n'

const isTop = window === window.top

// ————————————————————————————————————————————————————————————— detección

const USER_RE = /user|login|e-?mail|correo|usuario|account|cuenta|identifier|nick|dni|nif|phone|tel[eé]fono|m[oó]vil|mobile/
const OTP_RE = /one.?time|otp|totp|2fa|mfa|two.?factor|verification|verificaci|c[oó]digo|\bcode\b|token|auth/
const CURRENT_RE = /current|old|actual|anterior/
const NEW_RE = /new|nuev|confirm|repeat|repit|retype|again|regist|sign.?up|crear|create|join/
const NEW_FORM_RE = /regist|sign.?up|signup|crear|create|join|alta|nueva.?cuenta|new.?account|change|cambiar|reset|restablec/
const SUBMIT_RE = /log|sign|entrar|acceder|iniciar|continu|next|siguiente|regist|crear|create|join|guardar|save|submit|enviar|cambiar|change|update|actualizar|confirm|aceptar|ok\b|go\b/i
const CARD_NUM_RE = /card.?num|cc.?num|cardnumber|n[uú]mero.?(de.?)?(la.?)?tarjeta|credit.?card|card.?no\b/
const CVC_RE = /\bcvv|\bcvc|\bcsc|\bcvn|security.?code|c[oó]digo.?de.?seguridad|card.?code|verification.?(value|number)/
const CARD_NAME_RE = /card.?holder|holder|name.?on.?card|cc.?name|titular|nombre.?(en|de|del).?(la.?)?tarjeta/
const EXP_RE = /exp|caduc|venc|valid/
const MONTH_RE = /month|\bmes\b|\bmm\b/
const YEAR_RE = /year|a[nñ]o|\byy\b|\baa\b|\byyyy\b|\baaaa\b/
const TEXTUAL = new Set(['text', 'email', 'tel'])
const AC_SKIP = /^(section-.*|shipping|billing|home|work|mobile|fax|pager|webauthn|on|off)$/

type Field = HTMLInputElement | HTMLSelectElement

/** El `autocomplete` útil (`username webauthn` → `username`; `shipping street-address` → `street-address`) */
function acOf(el: Element): string {
  const tokens = (el.getAttribute('autocomplete') ?? '').toLowerCase().split(/\s+/).filter((x) => x && !AC_SKIP.test(x))
  return tokens.at(-1) ?? ''
}

function hints(el: Field): string {
  const labels = [...(el.labels ?? [])].map((l) => l.textContent ?? '')
  const ph = el instanceof HTMLInputElement ? el.placeholder : ''
  return [el.name, el.id, ph, el.getAttribute('aria-label') ?? '', ...labels].join(' ').toLowerCase()
}

function visible(el: Element): boolean {
  const r = el.getBoundingClientRect()
  return r.width > 2 && r.height > 2 && el.checkVisibility({ opacityProperty: true, visibilityProperty: true })
}

const editable = (el: Field): boolean => !el.disabled && !(el instanceof HTMLInputElement && el.readOnly) && visible(el)

/** Todos los campos bajo `root`, también dentro de shadow DOM abiertos */
function fieldsIn(root: ParentNode): Field[] {
  const out: Field[] = []
  const walk = (r: ParentNode): void => {
    for (const e of r.querySelectorAll('*')) {
      if (e instanceof HTMLInputElement || e instanceof HTMLSelectElement) out.push(e)
      if (e.shadowRoot) walk(e.shadowRoot)
    }
  }
  walk(root)
  return out
}

const scopeOf = (el: Field): ParentNode => el.form ?? document
const inputsIn = (scope: ParentNode): HTMLInputElement[] => fieldsIn(scope).filter((f): f is HTMLInputElement => f instanceof HTMLInputElement)
const passwords = (scope: ParentNode): HTMLInputElement[] => inputsIn(scope).filter((i) => i.type === 'password' && visible(i))
const textual = (scope: ParentNode): HTMLInputElement[] => inputsIn(scope).filter((i) => TEXTUAL.has(i.type) && visible(i))
const before = (a: Node, b: Node): boolean => !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING)

function formHints(el: HTMLInputElement): string {
  const f = el.form
  if (!f) return ''
  const buttons = [...f.querySelectorAll('button, input[type=submit]')].map((b) => (b instanceof HTMLInputElement ? b.value : b.textContent) ?? '')
  return [f.id, f.name, f.className, f.getAttribute('action') ?? '', ...buttons].join(' ').toLowerCase()
}

function isNewPassword(el: HTMLInputElement, scope: ParentNode): boolean {
  const ac = acOf(el)
  if (ac === 'new-password') return true
  if (ac === 'current-password') return false
  const h = hints(el)
  if (CURRENT_RE.test(h)) return false
  if (NEW_RE.test(h) || passwords(scope).length >= 2) return true
  return NEW_FORM_RE.test(formHints(el))
}

function usernameFor(pw: HTMLInputElement, scope: ParentNode): HTMLInputElement | undefined {
  const prev = textual(scope).filter((i) => before(i, pw))
  const likely = prev.filter((i) => acOf(i) === 'username' || i.type === 'email' || USER_RE.test(hints(i)))
  return likely.at(-1) ?? prev.at(-1)
}

type CardRole = 'number' | 'name' | 'exp' | 'month' | 'year' | 'cvc'
const CARD_AC: Record<string, CardRole> = {
  'cc-number': 'number',
  'cc-name': 'name',
  'cc-given-name': 'name',
  'cc-exp': 'exp',
  'cc-exp-month': 'month',
  'cc-exp-year': 'year',
  'cc-csc': 'cvc'
}

function cardRole(el: Field): CardRole | null {
  const ac = CARD_AC[acOf(el)]
  if (ac) return ac
  const h = hints(el)
  if (CVC_RE.test(h)) return 'cvc'
  if (CARD_NUM_RE.test(h)) return 'number'
  if (CARD_NAME_RE.test(h)) return 'name'
  if (EXP_RE.test(h) || (MONTH_RE.test(h) && YEAR_RE.test(h))) {
    const m = MONTH_RE.test(h)
    const y = YEAR_RE.test(h)
    return m && !y ? 'month' : y && !m ? 'year' : 'exp'
  }
  return null
}

type IdRole = 'name' | 'given' | 'family' | 'email' | 'tel' | 'address'
const ID_AC: Record<string, IdRole> = {
  name: 'name',
  'given-name': 'given',
  'family-name': 'family',
  email: 'email',
  tel: 'tel',
  'tel-national': 'tel',
  'street-address': 'address',
  'address-line1': 'address'
}

function identityRole(el: Field): IdRole | null {
  const ac = ID_AC[acOf(el)]
  if (ac) return ac
  if (el instanceof HTMLInputElement && !TEXTUAL.has(el.type)) return null
  const h = hints(el)
  if (/e-?mail|correo/.test(h)) return 'email'
  if (/phone|tel[eé]?f|m[oó]vil|mobile|\btel\b/.test(h)) return 'tel'
  if (/last.?name|surname|family.?name|apellido/.test(h)) return 'family'
  if (/first.?name|given.?name|\bnombre\b(?!.*complet)/.test(h)) return 'given'
  if (/full.?name|nombre.?completo|your.?name|^\s*name\b/.test(h)) return 'name'
  if (/address|direcci[oó]n|street|calle|domicilio/.test(h)) return 'address'
  return null
}

/** Qué es un campo (o nada): de eso depende lo que ofrece el menú */
function classify(el: Element | null): FieldKind | null {
  if (!(el instanceof HTMLInputElement) || !editable(el)) return null
  const ac = acOf(el)
  const scope = scopeOf(el)
  if (ac === 'one-time-code') return 'otp'
  if (ac.startsWith('cc-') || CARD_NUM_RE.test(hints(el))) return 'card'
  if (el.type === 'password') return isNewPassword(el, scope) ? 'newpw' : 'login'
  if (!TEXTUAL.has(el.type) && el.type !== 'number') return null
  const h = hints(el)
  if (cardRole(el) && fieldsIn(scope).some((f) => cardRole(f) === 'number')) return 'card'
  const pws = passwords(scope)
  const short = (el.maxLength > 0 && el.maxLength <= 10) || el.inputMode === 'numeric' || el.type === 'tel' || el.type === 'number'
  if (!pws.length && OTP_RE.test(h) && short) return 'otp'
  if (pws.length) {
    const next = pws.find((p) => before(el, p))
    // El usuario de un registro no ofrece nada (no hay cuenta todavía)
    return next && !isNewPassword(next, scope) ? 'login' : null
  }
  if (TEXTUAL.has(el.type)) {
    const roles = textual(scope).filter((i) => identityRole(i))
    // Formulario de datos (envío, perfil…): identidad. Un solo campo de usuario: primer paso de un login.
    if (roles.length >= 2 && identityRole(el)) return 'identity'
    if (ac === 'username' || ((ac === 'email' || el.type === 'email' || USER_RE.test(h)) && textual(scope).length <= 2)) return 'login'
  }
  return null
}

function deepActive(): Element | null {
  let a = document.activeElement
  while (a?.shadowRoot?.activeElement) a = a.shadowRoot.activeElement
  return a
}

// ————————————————————————————————————————————————————————————— rellenar

/** Mientras se rellena, cambiar el foco de campo no abre el menú */
let quietUntil = 0

function setValue(el: Field, v: string): void {
  if (!editable(el)) return
  quietUntil = Date.now() + 600
  el.focus({ preventScroll: true })
  const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype
  Object.getOwnPropertyDescriptor(proto, 'value')?.set?.call(el, v)
  el.dispatchEvent(new Event('input', { bubbles: true, composed: true }))
  el.dispatchEvent(new Event('change', { bubbles: true }))
}

function setSelect(el: HTMLSelectElement, wanted: string[]): void {
  const norm = (s: string): string => s.trim().toLowerCase().replace(/^0+(?=\d)/, '')
  const want = wanted.map(norm)
  const opt = [...el.options].find((o) => want.includes(norm(o.value)) || want.includes(norm(o.text)))
  if (opt) setValue(el, opt.value)
}

function fillLogin(anchor: HTMLInputElement | null, c: Creds): void {
  const scope = anchor ? scopeOf(anchor) : document
  const pws = passwords(scope)
  const pw = (anchor?.type === 'password' ? anchor : undefined) ?? pws.find((p) => !isNewPassword(p, scope)) ?? pws[0]
  const user = pw ? usernameFor(pw, scope) : anchor?.type !== 'password' ? (anchor ?? textual(scope)[0]) : undefined
  if (user && c.username) setValue(user, c.username)
  if (pw) setValue(pw, c.password)
}

function fillNewPassword(anchor: HTMLInputElement, pw: string): void {
  const scope = scopeOf(anchor)
  // Contraseña nueva y su repetición; la actual (en un cambio de contraseña) no se toca
  for (const p of passwords(scope)) if (isNewPassword(p, scope)) setValue(p, pw)
}

function fillOtp(anchor: HTMLInputElement, code: string): void {
  if (anchor.maxLength !== 1) return setValue(anchor, code)
  // Una casilla por cifra
  const boxes = inputsIn(scopeOf(anchor)).filter((i) => i.maxLength === 1 && visible(i))
  const start = boxes.indexOf(anchor)
  ;[...code].forEach((d, i) => boxes[start + i] && setValue(boxes[start + i], d))
}

function expiryParts(s: string): { mm: string; yyyy: string } | null {
  const iso = s.match(/^\s*(\d{4})\D+(\d{1,2})/)
  const usual = s.match(/(\d{1,2})\D+(\d{2,4})/)
  const [mm, y] = iso ? [iso[2], iso[1]] : usual ? [usual[1], usual[2]] : []
  if (!mm || !y) return null
  return { mm: mm.padStart(2, '0'), yyyy: y.length === 2 ? `20${y}` : y }
}

function fillCard(anchor: HTMLInputElement, d: CardData): void {
  const exp = expiryParts(d.expiry)
  for (const f of fieldsIn(scopeOf(anchor)).filter(editable)) {
    const role = cardRole(f)
    if (!role) continue
    const yy = exp?.yyyy.slice(2) ?? ''
    if (f instanceof HTMLSelectElement) {
      if (role === 'month' && exp) setSelect(f, [exp.mm, String(Number(exp.mm))])
      if (role === 'year' && exp) setSelect(f, [exp.yyyy, yy])
      continue
    }
    const long = /yyyy|aaaa/.test(hints(f)) || f.maxLength >= 7
    const value = { number: d.number, name: d.holder, cvc: d.cvv, month: exp?.mm, year: exp && (f.maxLength === 2 ? yy : exp.yyyy), exp: exp && `${exp.mm}/${long ? exp.yyyy : yy}` }[role]
    if (value) setValue(f, value)
  }
}

function fillIdentity(anchor: HTMLInputElement, d: IdentityData): void {
  const [given, ...rest] = d.fullname.trim().split(/\s+/)
  const values: Record<IdRole, string> = { name: d.fullname, given: given ?? '', family: rest.join(' '), email: d.email, tel: d.phone, address: d.address.split(/\r?\n/)[0] ?? '' }
  for (const f of fieldsIn(scopeOf(anchor)).filter(editable)) {
    const role = identityRole(f)
    if (role && values[role] && !f.value) setValue(f, values[role])
  }
}

// ————————————————————————————————————————————————————————————— capa visible (shadow DOM cerrado)

const PAGE_CSS = `
.layer { position: fixed; inset: 0; pointer-events: none; }
/* Sobre webs claras el cristal translúcido se ve gris: casi opaco, como los menús de la app */
.menu, .bar { background: var(--glass-strong); }
.menu { position: fixed; pointer-events: auto; width: 300px; max-height: 340px; overflow-y: auto; padding: 6px; animation: pp-in 0.16s var(--ease-out); transform-origin: top center; }
.menu.up { transform-origin: bottom center; }
.bar { position: fixed; top: 16px; right: 16px; width: 340px; max-width: calc(100vw - 32px); padding: 12px 14px 14px; pointer-events: auto; animation: pp-in 0.2s var(--ease-out); }
.bar h4 { margin: 2px 0 2px; font-size: 15px; font-weight: 800; color: var(--text); }
.bar .sub { color: var(--muted); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.bar .acts { display: flex; gap: 8px; align-items: center; margin-top: 12px; }
.bar .acts .grow { flex: 1; }
.bar .ok { display: flex; gap: 8px; align-items: center; color: var(--success); font-weight: 700; padding: 4px 0; }
`

let layer: { host: HTMLElement; pp: HTMLElement } | null = null

function getLayer(status: Status | null): HTMLElement {
  if (!layer) {
    loadFont()
    const host = document.createElement('poxipass-ui')
    // En la capa superior (popover): por encima de cualquier z-index de la página
    host.setAttribute('popover', 'manual')
    host.style.cssText = 'all: initial; position: fixed; inset: 0; width: 100vw; height: 100vh; margin: 0; padding: 0; border: 0; background: transparent; overflow: visible; pointer-events: none;'
    const root = host.attachShadow({ mode: 'closed' })
    root.append(h('style', {}, CSS + PAGE_CSS))
    const pp = h('div', { class: 'pp layer' })
    root.append(pp)
    layer = { host, pp }
  }
  if (!layer.host.isConnected) {
    document.documentElement.append(layer.host)
    layer.host.showPopover()
  }
  applyTheme(layer.pp, status?.info ?? null)
  return layer.pp
}

function dropLayerIfEmpty(): void {
  if (layer && !layer.pp.childElementCount) layer.host.remove()
}

// ————————————————————————————————————————————————————————————— menú bajo el campo

interface Item {
  node: HTMLButtonElement
  run: () => void
}

let menu: { el: HTMLElement; anchor: HTMLInputElement; kind: FieldKind; items: Item[]; active: number } | null = null
const dismissed = new WeakSet<Element>()

function closeMenu(): void {
  menu?.el.remove()
  menu = null
  dropLayerIfEmpty()
}

function item(icon: string, title: string, sub: string, run: () => void, avatar?: string): Item {
  const node = h(
    'button',
    { class: 'item', type: 'button', role: 'option', tabindex: '-1' },
    h('span', avatar ? { class: 'av' } : { class: 'av', html: icon }, avatar ?? null),
    h('span', { class: 'txt' }, h('span', { class: 't' }, title), sub ? h('span', { class: 's' }, sub) : null)
  )
  // Sin robar el foco al campo (así el teclado y el autocompletado de la web siguen donde estaban)
  node.addEventListener('mousedown', (e) => e.preventDefault())
  node.addEventListener('click', run)
  return { node, run }
}

function note(kind: 'warn' | 'err' | '', icon: string, text: string): HTMLElement {
  return h('div', { class: `note ${kind}` }, h('span', { html: icon }), h('span', {}, text))
}

const NATIVE_MENU_H = 220

function place(): void {
  if (!menu) return
  const a = menu.anchor
  if (!a.isConnected || !visible(a)) return closeMenu()
  const r = a.getBoundingClientRect()
  const w = Math.max(260, Math.min(360, r.width))
  const mh = menu.el.offsetHeight
  // El navegador pinta su propio menú de autorrelleno debajo del campo (encima si no cabe) y ninguna
  // extensión puede quitarlo ni ponerse por delante: el nuestro va al lado contrario para que no lo tape.
  // La altura del menú del navegador es una estimación (unas 3 sugerencias); si la página no deja hueco arriba, va debajo
  const roomUp = r.top - 6 - mh > 8
  const roomDown = r.bottom + 6 + mh <= innerHeight - 8
  const nativeBelow = r.bottom + NATIVE_MENU_H <= innerHeight
  const up = roomUp && (nativeBelow || !roomDown)
  menu.el.classList.toggle('up', up)
  menu.el.style.width = `${w}px`
  menu.el.style.left = `${Math.min(Math.max(8, r.left), innerWidth - w - 8)}px`
  menu.el.style.top = `${up ? r.top - 6 - mh : r.bottom + 6}px`
}

function setActive(i: number): void {
  if (!menu) return
  menu.active = i
  menu.items.forEach((it, j) => it.node.classList.toggle('active', j === i))
  menu.items[i]?.node.scrollIntoView({ block: 'nearest' })
}

/** Error dentro del propio menú (sin cerrarlo) */
function menuError(e: unknown): void {
  if (!menu) return
  menu.el.querySelector('.note.err')?.remove()
  menu.el.append(note('err', ICON.alert, errText(e)))
  place()
}

async function act(fn: () => Promise<void>): Promise<void> {
  try {
    await fn()
    closeMenu()
  } catch (e) {
    menuError(e)
  }
}

function buildItems(anchor: HTMLInputElement, kind: FieldKind, o: Offer, explicit: boolean): { items: Item[]; notes: HTMLElement[] } {
  const items: Item[] = []
  const notes: HTMLElement[] = []
  const s = o.status
  const anyKind = kind === 'login' || kind === 'newpw' || explicit
  if (s.state === 'noHost' || s.state === 'noApp') {
    // Sin app no se molesta al enfocar; con el atajo sí se explica
    if (explicit) notes.push(note('err', ICON.alert, t(s.state)))
    return { items, notes }
  }
  if (s.state === 'unpaired' && anyKind) items.push(item(ICON.key, t('connect'), t('unpairedHint'), () => void send({ type: 'pair' }).catch(menuError)))
  if (s.state === 'pairing') {
    notes.push(note('', ICON.key, t('pairing')))
    notes.push(h('div', { class: 'code' }, ...(s.code ?? '').split('').map((d) => h('span', {}, d))))
  }
  if (s.state === 'locked') items.push(item(ICON.lock, t('unlock'), t('locked'), () => void send({ type: 'unlock' }).catch(menuError)))
  if (s.state !== 'ready') return { items, notes }

  if (kind === 'newpw') {
    items.push(
      item(ICON.wand, t('generate'), t('generateHint'), () =>
        void act(async () => {
          const { password } = await send<{ password: string }>({ type: 'generate' })
          fillNewPassword(anchor, password)
        })
      )
    )
    return { items, notes }
  }
  if (kind === 'card') {
    for (const c of o.cards ?? [])
      items.push(item(ICON.card, c.title, c.last4 ? `•••• ${c.last4}` : c.holder, () => void act(async () => fillCard(anchor, await send<CardData>({ type: 'fillCard', entryId: c.id })))))
    return { items, notes }
  }
  if (kind === 'identity') {
    for (const p of o.identities ?? [])
      items.push(item(ICON.user, p.title, p.fullname || p.email, () => void act(async () => fillIdentity(anchor, await send<IdentityData>({ type: 'fillIdentity', entryId: p.id })))))
    return { items, notes }
  }
  // Login y 2FA: solo en la página principal o en frames del mismo sitio (un frame ajeno podría ser una trampa)
  if (!isTop && !o.sameSite) return { items: [], notes: [] }
  if (o.insecure && o.logins?.length) notes.push(note('warn', ICON.alert, t('insecure')))
  for (const l of o.logins ?? []) {
    if (kind === 'otp') {
      if (l.totp)
        items.push(
          item(ICON.shield, l.title, `${t('totp')} · ${l.username || t('noUser')}`, () =>
            void act(async () => fillOtp(anchor, (await send<{ code: string }>({ type: 'totp', entryId: l.id })).code))
          )
        )
      continue
    }
    items.push(item('', l.title, l.username || t('noUser'), () => void act(async () => fillLogin(anchor, await send<Creds>({ type: 'fill', entryId: l.id }))), initial(l.title)))
  }
  return { items, notes }
}

async function openMenu(anchor: HTMLInputElement, kind: FieldKind, explicit: boolean): Promise<void> {
  const o = await send<Offer>({ type: 'offer', kind }).catch(() => null)
  if (!o) return
  setLang(o.status.info?.lang)
  if (!explicit && deepActive() !== anchor) return
  const { items, notes } = buildItems(anchor, kind, o, explicit)
  if (!items.length && !notes.length) return closeMenu()
  const pp = getLayer(o.status)
  menu?.el.remove()
  const head = h('div', { class: 'head' }, h('span', { class: 'logo', html: LOGO }), h('span', { class: 'grow' }, 'PoxiPass'))
  const list = h('div', { role: 'listbox', 'aria-label': 'PoxiPass' }, ...items.map((i) => i.node))
  const el = h('div', { class: 'menu glass' }, head, ...notes, list)
  el.addEventListener('mousedown', (e) => e.preventDefault())
  pp.append(el)
  menu = { el, anchor, kind, items, active: -1 }
  place()
}

document.addEventListener(
  'focusin',
  (e) => {
    const el = e.composedPath()[0]
    if (Date.now() < quietUntil) return
    if (!(el instanceof HTMLInputElement)) return closeMenu()
    if (menu?.anchor === el || dismissed.has(el)) return
    const kind = classify(el)
    if (!kind) return closeMenu()
    void openMenu(el, kind, false)
  },
  true
)

document.addEventListener(
  'focusout',
  () => {
    setTimeout(() => {
      if (menu && deepActive() !== menu.anchor) closeMenu()
    }, 120)
  },
  true
)

document.addEventListener(
  'keydown',
  (e) => {
    if (e.key === 'Enter' && !(menu && menu.active >= 0)) {
      const a = deepActive()
      if (a instanceof HTMLInputElement) capture(a)
    }
    if (!menu || deepActive() !== menu.anchor) return
    const n = menu.items.length
    if (e.key === 'ArrowDown' && n) setActive((menu.active + 1) % n)
    else if (e.key === 'ArrowUp' && n) setActive((menu.active - 1 + n) % n)
    else if (e.key === 'Enter' && menu.active >= 0) menu.items[menu.active].run()
    else if (e.key === 'Escape') {
      dismissed.add(menu.anchor)
      closeMenu()
    } else return
    e.preventDefault()
    e.stopPropagation()
  },
  true
)

addEventListener('scroll', () => requestAnimationFrame(place), { capture: true, passive: true })
addEventListener('resize', () => requestAnimationFrame(place), { passive: true })

// ————————————————————————————————————————————————————————————— guardar lo que se envía

function capture(from: Element): void {
  const scope = from instanceof HTMLInputElement || from instanceof HTMLButtonElement ? (from.form ?? document) : document
  const pws = passwords(scope).filter((p) => p.value)
  if (!pws.length) {
    // Primer paso de un login en dos pasos: se recuerda el usuario para cuando llegue la contraseña
    const u = textual(scope).find((i) => i.value && (acOf(i) === 'username' || i.type === 'email' || USER_RE.test(hints(i))))
    if (u) send({ type: 'submitted', username: u.value, password: '' }).catch(() => {})
    return
  }
  const fresh = pws.filter((p) => isNewPassword(p, scope))
  // Contraseña y repetición distintas: es una errata, no se ofrece guardar
  if (fresh.length >= 2 && fresh.at(-1)?.value !== fresh.at(-2)?.value) return
  const pw = (fresh.at(-1) ?? pws[0]).value
  const user = usernameFor(pws[0], scope)?.value ?? ''
  send({ type: 'submitted', username: user, password: pw }).catch(() => {})
}

document.addEventListener('submit', (e) => e.target instanceof HTMLFormElement && capture(e.target.querySelector('input') ?? e.target), true)
document.addEventListener(
  'click',
  (e) => {
    const t0 = e.composedPath()[0]
    const b = t0 instanceof Element ? t0.closest('button, input[type=submit], input[type=button], [role=button]') : null
    if (!b) return
    const isSubmit = (b instanceof HTMLButtonElement && b.type === 'submit' && !!b.form) || (b instanceof HTMLInputElement && b.type === 'submit')
    const text = (b instanceof HTMLInputElement ? b.value : b.textContent) ?? ''
    if (isSubmit || SUBMIT_RE.test(text) || SUBMIT_RE.test(b.getAttribute('aria-label') ?? '')) capture(b)
  },
  true
)

// ————————————————————————————————————————————————————————————— barra «¿Guardar?» (solo en la página principal)

let bar: HTMLElement | null = null

function closeBar(): void {
  bar?.remove()
  bar = null
  dropLayerIfEmpty()
}

function barDone(text: string): void {
  if (!bar) return
  bar.querySelector('.body')?.replaceChildren(h('div', { class: 'ok' }, h('span', { html: ICON.shield }), h('span', {}, text)))
  setTimeout(closeBar, 2600)
}

async function showPending(): Promise<void> {
  const p = await send<Pending | null>({ type: 'pending' }).catch(() => null)
  if (!p) return
  const status = await send<Status>({ type: 'peek' }).catch(() => null)
  setLang(status?.info?.lang)
  const pp = getLayer(status)
  bar?.remove()
  const update = p.status === 'changed'
  const answer = (action: string): Promise<{ done: string | null }> => send<{ done: string | null }>({ type: 'answer', action })
  const save = h('button', { class: 'btn btn-primary', type: 'button' }, t(update ? 'update' : 'save'))
  save.addEventListener('click', async () => {
    save.setAttribute('disabled', '')
    try {
      const r = await answer('save')
      if (r.done === 'waiting') {
        bar?.querySelector('.body')?.replaceChildren(note('', ICON.lock, t('saveAfterUnlock')))
        return
      }
      if (r.done) barDone(t(r.done === 'updated' ? 'updated' : 'saved'))
      else closeBar()
    } catch (e) {
      save.removeAttribute('disabled')
      bar?.querySelector('.note.err')?.remove()
      bar?.querySelector('.body')?.append(note('err', ICON.alert, errText(e as Err)))
    }
  })
  const later = h('button', { class: 'btn btn-ghost', type: 'button', onclick: () => void answer('dismiss').finally(closeBar) }, t('notNow'))
  const never = h('button', { class: 'btn-link', type: 'button', onclick: () => void answer('never').finally(closeBar) }, t('never'))
  const close = h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('close'), html: ICON.x, onclick: () => void answer('dismiss').finally(closeBar) })
  bar = h(
    'div',
    { class: 'bar glass', role: 'dialog', 'aria-label': 'PoxiPass' },
    h('div', { class: 'head' }, h('span', { class: 'logo', html: LOGO }), h('span', { class: 'grow' }, 'PoxiPass'), close),
    h(
      'div',
      { class: 'body' },
      h('h4', {}, t(update ? 'updateTitle' : 'saveTitle')),
      h('div', { class: 'sub' }, p.username ? `${p.host} · ${p.username}` : p.host),
      h('div', { class: 'acts' }, never, h('span', { class: 'grow' }), later, save)
    )
  )
  pp.append(bar)
}

// ————————————————————————————————————————————————————————————— mensajes del background

chrome.runtime.onMessage.addListener((msg: { type: string; status?: Status; creds?: Creds; done?: string | null }) => {
  if (msg.type === 'status' && msg.status) {
    setLang(msg.status.info?.lang)
    if (menu) void openMenu(menu.anchor, menu.kind, true)
  }
  if (!isTop && msg.type !== 'shortcut') return
  if (msg.type === 'pendingSave') void showPending()
  if (msg.type === 'saved') {
    if (msg.done) barDone(t(msg.done === 'updated' ? 'updated' : 'saved'))
    else closeBar()
  }
  if (msg.type === 'fillLogin' && msg.creds) {
    const a = deepActive()
    fillLogin(a instanceof HTMLInputElement && classify(a) === 'login' ? a : null, msg.creds)
  }
  if (msg.type === 'shortcut') void onShortcut()
})

/** Atajo (Ctrl+Shift+L): con una sola entrada para la web, rellena; si no, abre el menú */
async function onShortcut(): Promise<void> {
  if (!document.hasFocus()) return
  let el = deepActive()
  let kind = classify(el)
  if (!kind || !(el instanceof HTMLInputElement)) {
    const pw = passwords(document).find((p) => editable(p))
    el = pw ? (usernameFor(pw, scopeOf(pw)) ?? pw) : null
    kind = classify(el)
    if (!kind || !(el instanceof HTMLInputElement)) return
    quietUntil = Date.now() + 300
    el.focus()
  }
  const anchor = el
  if (kind === 'login') {
    const o = await send<Offer>({ type: 'offer', kind }).catch(() => null)
    const only = o?.logins?.length === 1 && (isTop || o.sameSite) ? o.logins[0] : null
    if (only) {
      try {
        return fillLogin(anchor, await send<Creds>({ type: 'fill', entryId: only.id }))
      } catch {
        // Se abre el menú y ahí se ve el error
      }
    }
  }
  await openMenu(anchor, kind, true)
}

// Una web que cambia de página tras enviar: la barra sale en la siguiente
if (isTop) {
  void showPending()
  setTimeout(() => !bar && void showPending(), 1500)
}
