// Tipos compartidos entre el background, los content scripts y el popup

export type State = 'connecting' | 'noHost' | 'noApp' | 'unpaired' | 'pairing' | 'locked' | 'ready'

export interface Info {
  version: string
  lang: 'es' | 'en'
  theme: 'dark' | 'light' | 'system'
  accent: string
}

export interface Status {
  state: State
  info: Info | null
  /** Código del emparejado mientras se espera la respuesta en la app */
  code: string | null
}

export interface Login {
  id: string
  title: string
  username: string
  totp: boolean
  protected: boolean
}

export interface Card {
  id: string
  title: string
  holder: string
  last4: string
}

export interface Identity {
  id: string
  title: string
  fullname: string
  email: string
}

export interface Creds {
  username: string
  password: string
  totp: string | null
}

export interface CardData {
  holder: string
  number: string
  expiry: string
  cvv: string
}

export interface IdentityData {
  fullname: string
  email: string
  phone: string
  address: string
}

export interface Err {
  code: string
  detail?: string
}

export type FieldKind = 'login' | 'newpw' | 'otp' | 'card' | 'identity'

/** Lo que se puede ofrecer en un campo: depende del tipo de campo y del estado de la app */
export interface Offer {
  status: Status
  logins?: Login[]
  cards?: Card[]
  identities?: Identity[]
  sameSite?: boolean
  insecure?: boolean
}

/** Contraseña enviada en un formulario que se puede guardar (la contraseña nunca sale del background) */
export interface Pending {
  host: string
  username: string
  status: 'new' | 'changed' | 'unknown'
}

export type Reply<T> = { ok: T } | { err: Err }

/** Mensaje a la extensión (background). Los errores llegan como `Err`. */
export async function send<T>(msg: Record<string, unknown>): Promise<T> {
  const r = (await chrome.runtime.sendMessage(msg)) as Reply<T> | undefined
  if (!r) throw { code: 'unknown' } satisfies Err
  if ('err' in r) throw r.err
  return r.ok
}
