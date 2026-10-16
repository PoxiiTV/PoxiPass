import { api, isAppError } from './api'
import { ask, getState, pickFile, promptPassword, refreshEntries, refreshInfo, setState, toast } from './store'
import { errorText, t } from '../i18n'

/** Errores de Rust: el conflicto con el disco abre su diálogo; el resto, un aviso (y se refresca el estado
 * por si el guardado falló y hay que enseñar el banner). */
export function fail(e: unknown): void {
  if (isAppError(e) && e.code === 'vault.changedOnDisk') {
    setState({ conflict: true })
    return
  }
  if (isAppError(e) && e.code === 'vault.locked') return
  toast(errorText(e), 'err')
  if (getState().info) void refreshInfo().catch(() => undefined)
}

/** Ejecuta y, si falla, avisa. Devuelve `undefined` si falló. */
export async function attempt<T>(p: Promise<T>): Promise<T | undefined> {
  try {
    return await p
  } catch (e) {
    fail(e)
    return undefined
  }
}

/**
 * Para entradas sensibles: si Rust responde `entry.protected`, se confirma que eres tú (Windows Hello o, si este
 * PC no lo tiene, la contraseña maestra) y se repite. Durante un minuto no se vuelve a pedir.
 */
export async function secured<T>(entry: string, fn: () => Promise<T>): Promise<T> {
  try {
    return await fn()
  } catch (e) {
    if (!isAppError(e) || e.code !== 'entry.protected') throw e
    try {
      await api.verifyEntry(entry, null)
    } catch (h) {
      if (!isAppError(h) || h.code !== 'hello.unavailable') throw h
      const pw = await promptPassword(t('protect.reason'))
      if (!pw) throw { code: 'hello.cancelled' }
      await api.verifyEntry(entry, pw)
    }
    return await fn()
  }
}

export async function copyField(entry: string, field: string): Promise<void> {
  const secs = await attempt(secured(entry, () => api.copyField(entry, field)))
  if (secs === undefined) return
  toast(secs > 0 ? t('toast.copied', { n: secs }) : t('toast.copiedForever'), 'ok', secs || undefined)
}

export async function copyText(text: string, remember = false): Promise<void> {
  const secs = await attempt(api.copyText(text, remember))
  if (secs === undefined) return
  toast(secs > 0 ? t('toast.copied', { n: secs }) : t('toast.copiedForever'), 'ok', secs || undefined)
}

export async function lockNow(): Promise<void> {
  await api.vaultLock()
}

/** Guarda el kit de emergencia en PDF (hecho por la app, con su diseño) donde elija la persona. */
export async function saveKitPdf(words: string[], vault: string): Promise<void> {
  const path = await pickFile({ mode: 'save', title: t('kit.pdfTitle'), exts: ['pdf'], ext: 'pdf', name: `${t('kit.fileName')} - ${vault}` })
  if (!path) return
  const texts = {
    title: t('kit.printTitle'),
    vaultLabel: t('kit.printVault'),
    vault,
    dateLabel: t('kit.printDate'),
    date: new Date().toLocaleDateString(),
    how: t('kit.printHow'),
    steps: t('kit.printSteps'),
    warn: t('kit.printWarn'),
    whereLabel: t('kit.printWhere')
  }
  if ((await attempt(api.kitPdf(path, words, texts))) !== undefined) toast(t('kit.pdfSaved'))
}

/** Antes de salir del editor con cambios sin guardar, se pregunta. Devuelve si se puede salir. */
export async function confirmLeave(): Promise<boolean> {
  const s = getState()
  if (!s.editing || !s.dirty) return true
  const ok = await ask(t('editor.discard'), t('editor.discardYes'))
  if (ok) setState({ dirty: false })
  return ok
}

/** Sale del editor (preguntando si hay cambios) y aplica lo que se pida */
export async function leaveEditor(next: Partial<ReturnType<typeof getState>> = { editing: null }): Promise<void> {
  if (await confirmLeave()) setState(next)
}

/** A la papelera, con «Deshacer» */
export async function trashEntry(id: string): Promise<void> {
  if ((await attempt(api.entryDelete(id))) === undefined) return
  await refreshEntries()
  toast(t('toast.trashed'), 'ok', undefined, {
    label: t('common.undo'),
    run: () => void attempt(api.entryRestore(id)).then(refreshEntries)
  })
}
