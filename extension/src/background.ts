// Service worker: la única conexión con PoxiPass (Native Messaging → relé → app). Las URLs que viajan a la app
// salen siempre de `sender` (las pone el navegador), nunca de lo que diga la página o el content script.

import type { Card, CardData, Creds, Err, FieldKind, Identity, IdentityData, Info, Login, Offer, Pending, State, Status } from './protocol'

const HOST = 'com.poxi.poxipass'
/** Lo enviado en un formulario se puede guardar durante 3 min (la web suele cambiar de página entre medias) */
const PENDING_TTL = 3 * 60_000
/** Con Windows Hello de por medio, la respuesta tarda lo que tarde la persona */
const SLOW = 90_000

type Waiter = { resolve: (v: unknown) => void; reject: (e: Err) => void; timer: ReturnType<typeof setTimeout> }

let port: chrome.runtime.Port | null = null
let greeted = false
let greeting: Promise<void> | null = null
let seq = 0
const waiting = new Map<number, Waiter>()
let status: Status = { state: 'connecting', info: null, code: null }

function setStatus(p: Partial<Status>): void {
  const before = JSON.stringify(status)
  status = { ...status, ...p }
  if (JSON.stringify(status) === before) return
  // Al popup (si está abierto) y a las pestañas (para refrescar un menú abierto)
  const msg = { type: 'status', status }
  chrome.runtime.sendMessage(msg).catch(() => {})
  void chrome.tabs.query({}).then((tabs) => {
    for (const t of tabs) if (t.id !== undefined) chrome.tabs.sendMessage(t.id, msg).catch(() => {})
  })
}

// ————————————————————————————————————————————————————————————— conexión

function connect(): chrome.runtime.Port {
  if (port) return port
  const p = chrome.runtime.connectNative(HOST)
  port = p
  greeted = false
  p.onMessage.addListener((m: { id?: number; ok?: unknown; err?: Err; event?: string }) => {
    if (m.event === 'locked') return setStatus({ state: 'locked' })
    if (m.event === 'unlocked') {
      setStatus({ state: 'ready' })
      void flushSaves()
      return
    }
    const w = m.id === undefined ? undefined : waiting.get(m.id)
    if (!w || m.id === undefined) return
    waiting.delete(m.id)
    clearTimeout(w.timer)
    if (m.err) w.reject(m.err)
    else w.resolve(m.ok)
  })
  p.onDisconnect.addListener(() => {
    // «not found» / «forbidden»: no hay app instalada o tiene la conexión apagada; si no, se ha cerrado
    const why = chrome.runtime.lastError?.message ?? ''
    const code = /not found|forbidden/i.test(why) ? 'noHost' : 'noApp'
    if (port === p) port = null
    for (const [id, w] of waiting) {
      clearTimeout(w.timer)
      w.reject({ code })
      waiting.delete(id)
    }
    setStatus({ state: code, code: null })
  })
  return p
}

function raw<T>(type: string, data: Record<string, unknown> = {}, ms = 20_000): Promise<T> {
  const p = connect()
  const id = ++seq
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      waiting.delete(id)
      reject({ code: 'timeout' })
    }, ms)
    waiting.set(id, { resolve: resolve as (v: unknown) => void, reject, timer })
    try {
      p.postMessage({ id, type, ...data })
    } catch {
      clearTimeout(timer)
      waiting.delete(id)
      reject({ code: 'noApp' })
    }
  })
}

/** Saludo con la clave del emparejado. La primera vez puede tardar: si la app no estaba abierta, el relé la arranca. */
function greet(): Promise<void> {
  if (port && greeted) return Promise.resolve()
  greeting ??= (async () => {
    const { pair } = (await chrome.storage.local.get('pair')) as { pair?: { pairId: string; key: string } }
    const { state, ...info } = await raw<Info & { state: State }>('hello', pair ?? {}, 30_000)
    greeted = true
    setStatus({ state: status.state === 'pairing' && state === 'unpaired' ? 'pairing' : state, info })
  })().finally(() => {
    greeting = null
  })
  return greeting
}

async function call<T>(type: string, data: Record<string, unknown> = {}, ms?: number): Promise<T> {
  await greet()
  try {
    return await raw<T>(type, data, ms)
  } catch (e) {
    const err = e as Err
    if (err.code === 'vault.locked') setStatus({ state: 'locked' })
    if (err.code === 'bridge.notPaired') {
      await chrome.storage.local.remove('pair')
      setStatus({ state: 'unpaired' })
    }
    throw err
  }
}

// ————————————————————————————————————————————————————————————— emparejado

function browserName(): string {
  const brands = ((navigator as { userAgentData?: { brands: { brand: string }[] } }).userAgentData?.brands ?? []).map((b) => b.brand).join(' ')
  const ua = navigator.userAgent
  if (/Edge/.test(brands) || /Edg\//.test(ua)) return 'Edge'
  if (/Opera/.test(brands) || /OPR\//.test(ua)) return 'Opera'
  if (/Brave/.test(brands)) return 'Brave'
  if (/Vivaldi/.test(ua)) return 'Vivaldi'
  if (/Google Chrome/.test(brands)) return 'Chrome'
  return 'Chromium'
}

async function pair(): Promise<void> {
  if (status.state === 'pairing') return
  await greet()
  const code = String(crypto.getRandomValues(new Uint16Array(1))[0] % 10_000).padStart(4, '0')
  setStatus({ state: 'pairing', code })
  try {
    const r = await raw<{ pairId: string; key: string }>('pair', { browser: browserName(), code }, 130_000)
    await chrome.storage.local.set({ pair: r })
    greeted = false
    setStatus({ state: 'connecting', code: null })
    await greet()
  } catch (e) {
    setStatus({ state: 'unpaired', code: null })
    throw e
  }
}

// ————————————————————————————————————————————————————————————— guardar lo enviado

interface Stored {
  url: string
  username: string
  password: string
  status: Pending['status']
  entryId?: string
  at: number
  /** Se pulsó «Guardar» con el llavero bloqueado: se guarda en cuanto se desbloquee */
  afterUnlock?: boolean
}

const pendingKey = (tab: number): string => `pending:${tab}`
/** Usuario del primer paso de un login en dos pasos (Google, Microsoft…) por pestaña */
const lastUser = new Map<number, { user: string; at: number }>()
const lastSubmit = new Map<number, { sig: string; at: number }>()

function hostOf(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return ''
  }
}

async function neverList(): Promise<string[]> {
  return ((await chrome.storage.local.get('never')) as { never?: string[] }).never ?? []
}

async function getPending(tab: number): Promise<Stored | null> {
  const k = pendingKey(tab)
  const p = (await chrome.storage.session.get(k))[k] as Stored | undefined
  return p && Date.now() - p.at < PENDING_TTL ? p : null
}

const clearPending = (tab: number): Promise<void> => chrome.storage.session.remove(pendingKey(tab))

async function submitted(tab: number, url: string, username: string, password: string): Promise<void> {
  const now = Date.now()
  if (!password) {
    if (username) lastUser.set(tab, { user: username, at: now })
    return
  }
  const host = hostOf(url)
  if (!host || (await neverList()).includes(host)) return
  const prev = lastUser.get(tab)
  const user = username || (prev && now - prev.at < PENDING_TTL ? prev.user : '')
  // El mismo envío llega a la vez por el clic y por el submit
  const sig = `${url}\n${user}\n${password.length}:${password.slice(-2)}`
  const last = lastSubmit.get(tab)
  if (last && last.sig === sig && now - last.at < 5000) return
  lastSubmit.set(tab, { sig, at: now })

  let p: Stored = { url, username: user, password, status: 'unknown', at: now }
  try {
    const r = await call<{ status: 'new' | 'same' | 'changed'; entryId?: string }>('check', { url, username: user, password })
    if (r.status === 'same') return clearPending(tab)
    p = { ...p, status: r.status, entryId: r.entryId }
  } catch (e) {
    // Bloqueado: se ofrece igual y se mira al guardar. Sin app o sin emparejar, nada.
    if ((e as Err).code !== 'vault.locked') return
  }
  await chrome.storage.session.set({ [pendingKey(tab)]: p })
  chrome.tabs.sendMessage(tab, { type: 'pendingSave' }, { frameId: 0 }).catch(() => {})
}

type Done = { done: 'saved' | 'updated' | 'waiting' | null }

async function commit(tab: number, p: Stored): Promise<Done> {
  try {
    let { status: st, entryId } = p
    if (st === 'unknown') {
      const r = await call<{ status: 'new' | 'same' | 'changed'; entryId?: string }>('check', { url: p.url, username: p.username, password: p.password })
      if (r.status === 'same') {
        await clearPending(tab)
        return { done: null }
      }
      st = r.status
      entryId = r.entryId
    }
    if (st === 'changed' && entryId) await call('update', { entryId, url: p.url, password: p.password }, SLOW)
    else await call('save', { url: p.url, username: p.username, password: p.password })
    await clearPending(tab)
    return { done: st === 'changed' ? 'updated' : 'saved' }
  } catch (e) {
    if ((e as Err).code !== 'vault.locked') throw e
    await chrome.storage.session.set({ [pendingKey(tab)]: { ...p, afterUnlock: true } })
    call('unlock').catch(() => {})
    return { done: 'waiting' }
  }
}

async function flushSaves(): Promise<void> {
  const all = (await chrome.storage.session.get(null)) as Record<string, Stored>
  for (const [k, p] of Object.entries(all)) {
    if (!k.startsWith('pending:') || !p.afterUnlock || Date.now() - p.at > PENDING_TTL) continue
    const tab = Number(k.slice('pending:'.length))
    try {
      const r = await commit(tab, p)
      chrome.tabs.sendMessage(tab, { type: 'saved', done: r.done }, { frameId: 0 }).catch(() => {})
    } catch {
      // Se queda ofrecido en la barra
    }
  }
}

async function answer(tab: number, action: string): Promise<Done> {
  const p = await getPending(tab)
  if (!p) return { done: null }
  if (action === 'never') {
    const host = hostOf(p.url)
    await chrome.storage.local.set({ never: [...new Set([...(await neverList()), host])] })
  }
  if (action !== 'save') {
    await clearPending(tab)
    return { done: null }
  }
  return commit(tab, p)
}

// ————————————————————————————————————————————————————————————— lo que se ofrece en cada campo

async function offer(kind: FieldKind, url: string, top: string): Promise<Offer> {
  try {
    await greet()
  } catch {
    return { status }
  }
  if (status.state !== 'ready') return { status }
  try {
    if (kind === 'card') return { status, cards: await call<Card[]>('cards', { url }) }
    if (kind === 'identity') return { status, identities: await call<Identity[]>('identities') }
    if (kind === 'newpw') return { status }
    return { status, ...(await call<{ logins: Login[]; sameSite: boolean; insecure: boolean }>('match', { url, top })) }
  } catch {
    return { status }
  }
}

async function activeTab(): Promise<chrome.tabs.Tab | undefined> {
  return (await chrome.tabs.query({ active: true, currentWindow: true }))[0]
}

async function popupState(): Promise<{ status: Status; host: string; logins: Login[]; never: boolean; shortcut: string }> {
  const tab = await activeTab()
  const url = tab?.url ?? ''
  const web = /^https?:\/\//.test(url)
  try {
    await greet()
  } catch {
    // El estado ya dice qué pasa
  }
  let logins: Login[] = []
  if (status.state === 'ready' && web) logins = (await call<{ logins: Login[] }>('match', { url, top: url }).catch(() => ({ logins: [] }))).logins
  const host = web ? hostOf(url) : ''
  const shortcut = (await chrome.commands.getAll()).find((c) => c.name === 'fill-login')?.shortcut ?? ''
  return { status, host, logins, never: !!host && (await neverList()).includes(host), shortcut }
}

async function popupFill(entryId: string): Promise<void> {
  const tab = await activeTab()
  if (tab?.id === undefined || !tab.url) throw { code: 'unknown' } satisfies Err
  const creds = await call<Creds>('fill', { entryId, url: tab.url }, SLOW)
  await chrome.tabs.sendMessage(tab.id, { type: 'fillLogin', creds }, { frameId: 0 })
}

// ————————————————————————————————————————————————————————————— mensajes

type Msg = { type: string; [k: string]: unknown }

async function handle(msg: Msg, sender: chrome.runtime.MessageSender): Promise<unknown> {
  const tab = sender.tab?.id
  const url = sender.url ?? ''
  const top = sender.tab?.url ?? ''
  const entryId = String(msg.entryId ?? '')
  // Páginas de la propia extensión (el popup); los content scripts traen la URL de la web
  const fromPopup = (sender.url ?? '').startsWith(chrome.runtime.getURL(''))
  switch (msg.type) {
    // Desde cualquier sitio de la extensión
    case 'peek':
      return status
    case 'status':
      await greet().catch(() => {})
      return status
    case 'pair':
      return pair()
    case 'unlock':
      return call('unlock')
    case 'generate':
      return call<{ password: string }>('generate')
    // Desde las páginas (content scripts)
    case 'offer':
      return offer(msg.kind as FieldKind, url, top)
    case 'fill':
      return call<Creds>('fill', { entryId, url }, SLOW)
    case 'totp':
      return call<{ code: string }>('totp', { entryId, url }, SLOW)
    case 'fillCard':
      return call<CardData>('fillCard', { entryId, url }, SLOW)
    case 'fillIdentity':
      return call<IdentityData>('fillIdentity', { entryId }, SLOW)
    case 'submitted':
      if (tab !== undefined) await submitted(tab, url, String(msg.username ?? ''), String(msg.password ?? ''))
      return null
    case 'pending': {
      if (tab === undefined) return null
      const p = await getPending(tab)
      return p && ({ host: hostOf(p.url), username: p.username, status: p.status } satisfies Pending)
    }
    case 'answer':
      return tab === undefined ? { done: null } : answer(tab, String(msg.action))
  }
  // Solo desde el popup
  if (!fromPopup) throw { code: 'unknown' } satisfies Err
  switch (msg.type) {
    case 'popup':
      return popupState()
    case 'popupFill':
      return popupFill(entryId)
    case 'copy':
      return call<number>('copy', { text: String(msg.text ?? '') })
    case 'retry':
      setStatus({ state: 'connecting' })
      await greet().catch(() => {})
      return status
    case 'unnever': {
      const host = String(msg.host ?? '')
      await chrome.storage.local.set({ never: (await neverList()).filter((h) => h !== host) })
      return null
    }
  }
  throw { code: 'unknown' } satisfies Err
}

chrome.runtime.onMessage.addListener((msg: Msg, sender, reply) => {
  handle(msg, sender).then(
    (ok) => reply({ ok: ok ?? null }),
    (e: unknown) => reply({ err: e && typeof e === 'object' && 'code' in e ? (e as Err) : { code: 'unknown' } })
  )
  return true
})

// Atajo: lo atiende el frame que tenga el foco
chrome.commands.onCommand.addListener((cmd, tab) => {
  if (cmd === 'fill-login' && tab?.id !== undefined) chrome.tabs.sendMessage(tab.id, { type: 'shortcut' }).catch(() => {})
})

chrome.tabs.onRemoved.addListener((tab) => {
  void clearPending(tab)
  lastUser.delete(tab)
  lastSubmit.delete(tab)
})
