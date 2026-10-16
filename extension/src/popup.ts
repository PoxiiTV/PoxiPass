// Panel del icono de la extensión: estado de la conexión, entradas de la web abierta y generador.

import { send, type Login, type Status } from './protocol'
import { applyTheme, CSS, h, ICON, initial, loadFont, LOGO } from './ui'
import { errText, setLang, t } from './i18n'

interface PopupState {
  status: Status
  host: string
  logins: Login[]
  never: boolean
  shortcut: string
}

const POPUP_CSS = `
html, body { margin: 0; }
body { width: 340px; background: #0a0e17; }
body:has(.pp[data-theme='light']) { background: #f3f6fb; }
.popup { padding: 12px; min-height: 140px; }
.popup .head { color: var(--text); font-size: 14px; padding: 0 0 8px; }
.popup .head .logo svg { width: 22px; height: 22px; border-radius: 6px; }
.hero { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; padding: 14px 10px 8px; }
.hero .big { width: 46px; height: 46px; border-radius: 14px; display: grid; place-items: center; background: rgba(var(--accent-rgb), 0.14); color: var(--accent); }
.hero .big svg { width: 22px; height: 22px; }
.hero b { font-size: 15px; font-weight: 800; }
.hero p { margin: 0 0 4px; color: var(--muted); font-size: 12.5px; line-height: 1.45; }
.label { font-size: 11.5px; font-weight: 800; color: var(--muted); padding: 10px 6px 4px; letter-spacing: 0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.list { display: flex; flex-direction: column; gap: 2px; max-height: 260px; overflow-y: auto; }
.gen { display: flex; align-items: center; gap: 2px; padding: 4px 4px 4px 10px; border-radius: 12px; background: var(--surface-2); border: 1px solid var(--border); }
.gen .val { flex: 1; min-width: 0; font-family: var(--mono); font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.foot { margin-top: 12px; padding-top: 8px; border-top: 1px solid var(--border); color: var(--muted); font-size: 11.5px; text-align: center; }
.row { display: flex; align-items: center; gap: 8px; }
.row .grow { flex: 1; }
.center { display: grid; place-items: center; padding: 30px 0; }
`

document.head.append(h('style', {}, CSS + POPUP_CSS))
const app = h('div', { class: 'pp popup' })
document.body.append(app)
loadFont()

let st: PopupState | null = null
let generated = ''
let message: HTMLElement | null = null

function flash(kind: 'err' | '', icon: string, text: string): void {
  message = h('div', { class: `note ${kind}`, role: 'status' }, h('span', { html: icon }), h('span', {}, text))
  render()
}

async function load(): Promise<void> {
  st = await send<PopupState>({ type: 'popup' }).catch(() => null)
  if (st?.status.state === 'ready' && !generated) generated = (await send<{ password: string }>({ type: 'generate' }).catch(() => ({ password: '' }))).password
  render()
}

function hero(icon: string, title: string, text: string, button?: HTMLElement): HTMLElement {
  return h('div', { class: 'hero' }, h('div', { class: 'big', html: icon }), h('b', {}, title), text ? h('p', {}, text) : null, button ?? null)
}

function button(cls: string, label: string, fn: () => Promise<unknown>): HTMLButtonElement {
  const b = h('button', { class: `btn ${cls}`, type: 'button' }, label)
  b.addEventListener('click', async () => {
    b.setAttribute('disabled', '')
    try {
      await fn()
    } catch (e) {
      flash('err', ICON.alert, errText(e))
    } finally {
      b.removeAttribute('disabled')
    }
  })
  return b
}

function body(s: PopupState): (HTMLElement | null)[] {
  const state = s.status.state
  if (state === 'connecting') return [h('div', { class: 'center' }, h('div', { class: 'spin' }))]
  if (state === 'noHost' || state === 'noApp') return [hero(ICON.alert, t(state), t(state === 'noHost' ? 'noHostHint' : 'noAppHint'), button('', t('retry'), () => send({ type: 'retry' }).then(load)))]
  if (state === 'unpaired') return [hero(ICON.key, t('unpaired'), t('unpairedHint'), button('btn-primary', t('connect'), () => send({ type: 'pair' })))]
  if (state === 'pairing') return [hero(ICON.key, t('pairing'), ''), h('div', { class: 'code' }, ...(s.status.code ?? '').split('').map((d) => h('span', {}, d)))]
  if (state === 'locked') return [hero(ICON.lock, t('locked'), '', button('btn-primary', t('unlock'), () => send({ type: 'unlock' })))]

  const out: (HTMLElement | null)[] = []
  if (s.host) {
    out.push(h('div', { class: 'label' }, t('forSite', { h: s.host })))
    const items = s.logins.map((l) => {
      const b = h('button', { class: 'item', type: 'button' }, h('span', { class: 'av' }, initial(l.title)), h('span', { class: 'txt' }, h('span', { class: 't' }, l.title), h('span', { class: 's' }, l.username || t('noUser'))))
      b.addEventListener('click', () => send({ type: 'popupFill', entryId: l.id }).then(() => window.close(), (e: unknown) => flash('err', ICON.alert, errText(e))))
      return b
    })
    out.push(items.length ? h('div', { class: 'list' }, ...items) : h('div', { class: 'note' }, t('noMatches')))
    if (s.never)
      out.push(
        h(
          'div',
          { class: 'note row' },
          h('span', { class: 'grow' }, t('neverHere')),
          button('btn-link', t('reenable'), () => send({ type: 'unnever', host: s.host }).then(load))
        )
      )
  }
  out.push(h('div', { class: 'label' }, t('generator')))
  const regen = h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('another'), html: ICON.refresh })
  regen.addEventListener('click', async () => {
    generated = (await send<{ password: string }>({ type: 'generate' }).catch(() => ({ password: generated }))).password
    render()
  })
  const copy = h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('copy'), html: ICON.copy })
  copy.addEventListener('click', async () => {
    try {
      const secs = await send<number>({ type: 'copy', text: generated })
      flash('', ICON.copy, secs ? t('copied', { n: secs }) : t('copiedForever'))
    } catch (e) {
      flash('err', ICON.alert, errText(e))
    }
  })
  out.push(h('div', { class: 'gen' }, h('span', { class: 'val' }, generated), regen, copy))
  return out
}

function render(): void {
  if (!st) {
    app.replaceChildren(h('div', { class: 'center' }, h('div', { class: 'spin' })))
    return
  }
  setLang(st.status.info?.lang)
  applyTheme(app, st.status.info)
  const s = st.status.state
  const open = s === 'ready' || s === 'locked' ? h('button', { class: 'icon-btn', type: 'button', 'aria-label': t('open'), html: ICON.external, onclick: () => void send({ type: 'unlock' }) }) : null
  app.replaceChildren(
    h('div', { class: 'head' }, h('span', { class: 'logo', html: LOGO }), h('span', { class: 'grow' }, 'PoxiPass'), open),
    ...body(st).filter((x): x is HTMLElement => !!x),
    message ?? '',
    st.shortcut && s === 'ready' ? h('div', { class: 'foot' }, t('shortcut', { k: st.shortcut })) : ''
  )
}

chrome.runtime.onMessage.addListener((msg: { type: string }) => {
  if (msg.type === 'status') void load()
})

render()
void load()
