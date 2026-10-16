// Textos de la extensión. El idioma es el de la app (lo dice `hello`); sin app, el del navegador. Fallback: español.

const es = {
  noHost: 'Falta PoxiPass para Windows',
  noHostHint: 'Instala PoxiPass en este PC y deja activado Ajustes → Navegador → «Conectar con el navegador».',
  noApp: 'No se ha podido abrir PoxiPass',
  noAppHint: 'Ábrelo a mano y vuelve a intentarlo.',
  unpaired: 'Conecta la extensión con tu llavero',
  unpairedHint: 'PoxiPass te pedirá permiso una sola vez.',
  connect: 'Conectar con PoxiPass',
  pairing: 'Confirma este código en PoxiPass',
  locked: 'El llavero está bloqueado',
  unlock: 'Desbloquear PoxiPass',
  open: 'Abrir PoxiPass',
  retry: 'Reintentar',
  noMatches: 'No hay entradas para esta web',
  insecure: 'Web sin cifrar (http): cuidado con lo que rellenas',
  generate: 'Usar contraseña segura',
  generateHint: 'Con tu configuración del generador',
  generator: 'Generador',
  another: 'Otra',
  copy: 'Copiar',
  copied: 'Copiada · se borra en {n} s',
  copiedForever: 'Copiada',
  totp: 'Código 2FA',
  noUser: 'Sin usuario',
  forSite: 'Para {h}',
  saveTitle: '¿Guardar en PoxiPass?',
  updateTitle: '¿Actualizar la contraseña?',
  save: 'Guardar',
  update: 'Actualizar',
  notNow: 'Ahora no',
  never: 'Nunca en esta web',
  saved: 'Guardada en PoxiPass',
  updated: 'Contraseña actualizada',
  saveAfterUnlock: 'Desbloquea PoxiPass y se guarda sola',
  neverHere: 'No se ofrece guardar en esta web',
  reenable: 'Volver a ofrecer',
  shortcut: '{k} rellena la web abierta',
  close: 'Cerrar',
  'err.unknown': 'Algo ha fallado',
  'err.noHost': 'Falta PoxiPass para Windows',
  'err.noApp': 'No se ha podido abrir PoxiPass',
  'err.timeout': 'PoxiPass no responde',
  'err.vault.locked': 'El llavero está bloqueado',
  'err.entry.protected': 'Entrada sensible: confírmala en PoxiPass y vuelve a intentarlo',
  'err.entry.notFound': 'Esa entrada ya no existe',
  'err.hello.cancelled': 'Cancelado',
  'err.hello.failed': 'Windows Hello no ha funcionado',
  'err.bridge.disabled': 'La conexión está desactivada en PoxiPass (Ajustes → Navegador)',
  'err.bridge.notPaired': 'Este navegador no está conectado',
  'err.bridge.busy': 'Ya hay otro navegador esperando permiso',
  'err.bridge.denied': 'Conexión rechazada en PoxiPass',
  'err.bridge.timeout': 'Nadie ha respondido en PoxiPass',
  'err.bridge.wrongSite': 'Esa entrada no es de esta web',
  'err.bridge.insecure': 'Esta web no es segura (http)'
}

export type Key = keyof typeof es

const en: Record<Key, string> = {
  noHost: 'PoxiPass for Windows is missing',
  noHostHint: 'Install PoxiPass on this PC and leave Settings → Browser → "Connect with the browser" on.',
  noApp: "Couldn't open PoxiPass",
  noAppHint: 'Open it yourself and try again.',
  unpaired: 'Connect the extension with your keychain',
  unpairedHint: 'PoxiPass will ask for permission just once.',
  connect: 'Connect with PoxiPass',
  pairing: 'Confirm this code in PoxiPass',
  locked: 'The keychain is locked',
  unlock: 'Unlock PoxiPass',
  open: 'Open PoxiPass',
  retry: 'Try again',
  noMatches: 'No items for this website',
  insecure: 'Unencrypted website (http): be careful what you fill in',
  generate: 'Use a strong password',
  generateHint: 'With your generator settings',
  generator: 'Generator',
  another: 'Another',
  copy: 'Copy',
  copied: 'Copied · cleared in {n} s',
  copiedForever: 'Copied',
  totp: '2FA code',
  noUser: 'No username',
  forSite: 'For {h}',
  saveTitle: 'Save in PoxiPass?',
  updateTitle: 'Update the password?',
  save: 'Save',
  update: 'Update',
  notNow: 'Not now',
  never: 'Never on this website',
  saved: 'Saved in PoxiPass',
  updated: 'Password updated',
  saveAfterUnlock: "Unlock PoxiPass and it'll be saved",
  neverHere: "Saving isn't offered on this website",
  reenable: 'Offer again',
  shortcut: '{k} fills the open website',
  close: 'Close',
  'err.unknown': 'Something went wrong',
  'err.noHost': 'PoxiPass for Windows is missing',
  'err.noApp': "Couldn't open PoxiPass",
  'err.timeout': "PoxiPass isn't responding",
  'err.vault.locked': 'The keychain is locked',
  'err.entry.protected': 'Sensitive item: confirm it in PoxiPass and try again',
  'err.entry.notFound': "That item doesn't exist anymore",
  'err.hello.cancelled': 'Cancelled',
  'err.hello.failed': "Windows Hello didn't work",
  'err.bridge.disabled': 'The connection is turned off in PoxiPass (Settings → Browser)',
  'err.bridge.notPaired': "This browser isn't connected",
  'err.bridge.busy': 'Another browser is already waiting for permission',
  'err.bridge.denied': 'Connection declined in PoxiPass',
  'err.bridge.timeout': 'Nobody answered in PoxiPass',
  'err.bridge.wrongSite': "That item isn't for this website",
  'err.bridge.insecure': "This website isn't secure (http)"
}

export const dicts = { es, en }

let lang: 'es' | 'en' = navigator.language.toLowerCase().startsWith('es') ? 'es' : 'en'

export function setLang(l: 'es' | 'en' | undefined): void {
  if (l) lang = l
}

export function t(key: Key, vars: Record<string, string | number> = {}): string {
  const s = dicts[lang][key] ?? es[key]
  return s.replace(/\{(\w+)\}/g, (_, k: string) => String(vars[k] ?? ''))
}

export function errText(e: unknown): string {
  const code = (e as { code?: string } | null)?.code ?? 'unknown'
  const k = `err.${code}` as Key
  return k in es ? t(k) : t('err.unknown')
}
