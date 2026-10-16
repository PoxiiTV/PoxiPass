import { useEffect, useRef, useState } from 'react'
import { Archive, ArchiveRestore, FolderInput, Layers, Star, Trash2, X } from 'lucide-react'
import { api } from '../lib/api'
import { refreshEntries, setState, toast, useStore } from '../lib/store'
import { attempt } from '../lib/actions'
import { t, useT } from '../i18n'
import { Select } from './controls'

type Action = 'trash' | 'restore' | 'archive' | 'unarchive' | 'favorite' | 'unfavorite' | 'move'

/** Aplica una acción a varias entradas y avisa (con «Deshacer» cuando tiene sentido) */
export async function bulk(ids: string[], action: Action, folder: string | null = null): Promise<void> {
  const undo: Partial<Record<Action, Action>> = { trash: 'restore', archive: 'unarchive', favorite: 'unfavorite' }
  const n = await attempt(api.entriesBulk(ids, action, folder))
  if (n === undefined) return
  await refreshEntries()
  setState({ multi: [] })
  const back = undo[action]
  toast(t(`bulk.done.${action}`, { n }), 'ok', undefined, back ? { label: t('common.undo'), run: () => void attempt(api.entriesBulk(ids, back, null)).then(refreshEntries) } : undefined)
}

/** Panel del detalle cuando hay varias entradas elegidas (Ctrl/Mayús + clic) */
export function BulkPanel(): React.JSX.Element {
  useT()
  const multi = useStore((s) => s.multi)
  const section = useStore((s) => s.section)
  const folders = useStore((s) => s.info?.folders ?? [])
  const [folder, setFolder] = useState('')
  const inTrash = section === 'trash'
  const inArchive = section === 'archive'
  return (
    <section className="detail">
      <div className="detail-inner">
        <div className="detail-head">
          <span className="avatar lg" style={{ background: 'var(--gradient)' }}>
            <Layers size={24} />
          </span>
          <div style={{ flex: 1 }}>
            <h1>{t('bulk.title', { n: multi.length })}</h1>
            <div className="kind">{t('bulk.hint')}</div>
          </div>
          <button className="icon-btn" aria-label={t('common.close')} onClick={() => setState({ multi: [] })}>
            <X size={18} />
          </button>
        </div>
        <div className="col" style={{ gap: 10, maxWidth: 420 }}>
          {inTrash ? (
            <button className="btn" onClick={() => void bulk(multi, 'restore')}>
              <ArchiveRestore size={16} /> {t('detail.restoreFromTrash')}
            </button>
          ) : (
            <>
              <div className="row">
                <div style={{ flex: 1 }}>
                  <Select block value={folder} onChange={setFolder} options={[{ value: '', label: t('editor.noFolder') }, ...folders.map((f) => ({ value: f.id, label: f.name }))]} />
                </div>
                <button className="btn" onClick={() => void bulk(multi, 'move', folder || null)}>
                  <FolderInput size={16} /> {t('bulk.move')}
                </button>
              </div>
              <button className="btn" onClick={() => void bulk(multi, 'favorite')}>
                <Star size={16} /> {t('bulk.favorite')}
              </button>
              <button className="btn" onClick={() => void bulk(multi, inArchive ? 'unarchive' : 'archive')}>
                {inArchive ? <ArchiveRestore size={16} /> : <Archive size={16} />} {inArchive ? t('detail.unarchive') : t('detail.archive')}
              </button>
              <button className="btn btn-danger" onClick={() => void bulk(multi, 'trash')}>
                <Trash2 size={16} /> {t('detail.moveToTrash')}
              </button>
            </>
          )}
        </div>
      </div>
    </section>
  )
}

/**
 * Arrastrar entradas a una carpeta, a Favoritos, al Archivo o a la Papelera de la barra lateral. Es un arrastre
 * propio con el ratón (el del navegador no convive con soltar archivos de Windows para adjuntarlos).
 */
export function useEntryDrag(): {
  start: (e: React.PointerEvent, id: string) => void
  /** Tras soltar, el clic que viene detrás no debe seleccionar */
  dragged: () => boolean
} {
  const st = useRef<{ x: number; y: number; ids: string[]; active: boolean; over: Element | null } | null>(null)
  const justDragged = useRef(false)
  const [ghost, setGhost] = useState<{ x: number; y: number; n: number } | null>(null)

  useEffect(() => {
    const move = (e: PointerEvent): void => {
      const d = st.current
      if (!d) return
      if (!d.active && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 6) return
      d.active = true
      document.body.classList.add('dragging')
      setGhost({ x: e.clientX, y: e.clientY, n: d.ids.length })
      const over = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-drop]') ?? null
      if (over !== d.over) {
        d.over?.classList.remove('drop-hover')
        over?.classList.add('drop-hover')
        d.over = over
      }
    }
    const up = (): void => {
      const d = st.current
      st.current = null
      setGhost(null)
      document.body.classList.remove('dragging')
      if (!d?.active) return
      justDragged.current = true
      setTimeout(() => (justDragged.current = false), 50)
      d.over?.classList.remove('drop-hover')
      const target = d.over?.getAttribute('data-drop')
      if (!target) return
      if (target.startsWith('folder:')) void bulk(d.ids, 'move', target.slice(7))
      else if (target === 'favorites') void bulk(d.ids, 'favorite')
      else if (target === 'archive') void bulk(d.ids, 'archive')
      else if (target === 'trash') void bulk(d.ids, 'trash')
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    return () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
    }
  }, [])

  useEffect(() => {
    let el = document.getElementById('drag-ghost')
    if (!ghost) return el?.remove()
    if (!el) {
      el = document.createElement('div')
      el.id = 'drag-ghost'
      document.body.appendChild(el)
    }
    el.textContent = `${ghost.n}`
    el.style.transform = `translate(${ghost.x + 14}px, ${ghost.y + 10}px)`
  }, [ghost])

  return {
    start: (e, id) => {
      if (e.button !== 0) return
      const multi = useStore.getState().multi
      st.current = { x: e.clientX, y: e.clientY, ids: multi.length > 1 && multi.includes(id) ? multi : [id], active: false, over: null }
    },
    dragged: () => justDragged.current
  }
}
