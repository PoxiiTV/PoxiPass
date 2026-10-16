import { CreditCard, FileBadge, IdCard, KeyRound, Landmark, StickyNote, Terminal, Wifi, ScrollText } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { FieldKind, Kind } from './api'

export interface TemplateField {
  key: string
  kind: FieldKind
  secret: boolean
}

const f = (key: string, kind: FieldKind = 'text', secret = false): TemplateField => ({ key, kind, secret })

/** Campos con los que nace cada tipo de entrada (luego se pueden añadir más) */
export const TEMPLATES: Record<Kind, TemplateField[]> = {
  login: [f('username'), f('password', 'password', true), f('url', 'url'), f('totp', 'totp', true), f('notes', 'multiline', true)],
  card: [f('holder'), f('number', 'password', true), f('expiry'), f('cvv', 'password', true), f('pin', 'password', true), f('notes', 'multiline', true)],
  note: [f('content', 'multiline', true)],
  wifi: [f('ssid'), f('password', 'password', true), f('security'), f('notes', 'multiline', true)],
  ssh: [
    f('username'),
    f('host'),
    f('privatekey', 'multiline', true),
    f('publickey', 'multiline'),
    f('passphrase', 'password', true),
    f('notes', 'multiline', true)
  ],
  license: [f('product'), f('licensekey', 'password', true), f('email', 'email'), f('purchased'), f('notes', 'multiline', true)],
  bank: [f('bank'), f('holder'), f('iban', 'password', true), f('swift'), f('pin', 'password', true), f('notes', 'multiline', true)],
  document: [f('doctype'), f('number', 'password', true), f('holder'), f('issued'), f('expiry'), f('notes', 'multiline', true)],
  identity: [
    f('fullname'),
    f('email', 'email'),
    f('phone'),
    f('address', 'multiline'),
    f('document', 'password', true),
    f('notes', 'multiline', true)
  ]
}

export const KINDS: Kind[] = ['login', 'card', 'note', 'identity', 'wifi', 'ssh', 'license', 'bank', 'document']

/** Campos que se ven nada más abrir el editor; el resto va en «Más detalles» */
export const PRIMARY: Record<Kind, string[]> = {
  login: ['username', 'password'],
  card: ['holder', 'number', 'expiry', 'cvv'],
  note: ['content'],
  identity: ['fullname', 'email', 'phone'],
  wifi: ['ssid', 'password'],
  ssh: ['username', 'host', 'privatekey'],
  license: ['product', 'licensekey'],
  bank: ['bank', 'iban'],
  document: ['doctype', 'number', 'expiry']
}

export const KIND_ICON: Record<Kind, LucideIcon> = {
  login: KeyRound,
  card: CreditCard,
  note: StickyNote,
  identity: IdCard,
  wifi: Wifi,
  ssh: Terminal,
  license: ScrollText,
  bank: Landmark,
  document: FileBadge
}
