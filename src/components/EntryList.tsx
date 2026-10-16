import { useMemo } from 'react'
import { Plus, SearchX, Star, Trash2, Vault } from 'lucide-react'
import { api, type EntrySummary } from '../lib/api'
import { ask, refreshEntries, setState, useStore, type Section, type Sort } from '../lib/store'
import { attempt, leaveEditor } from '../lib/actions'
import { search } from '../lib/search'
import { useT } from '../i18n'
import { Avatar } from './ui'
import { useEntryDrag } from './Bulk'
import { Select } from './controls'

export function inSection(e: EntrySummary, s: Section): boolean {
  if (s === 'trash') return !!e.deleted
  if (e.deleted) return false
  if (s === 'archive') return e.archived
  if (e.archived) return false
  if (s === 'all' || s === 'generator' || s === 'health') return true
  if (s === 'favorites') return e.favorite
  if (s.startsWith('folder:')) return e.folder === s.slice(7)
  return e.kind === s
}

/** Lo que se ve en la lista (sección + búsqueda + orden). Lo usan también los atajos de teclado. */
export function useVisible(): EntrySummary[] {
  const entries = useStore((s) => s.entries)
  const section = useStore((s) => s.section)
  const query = useStore((s) => s.query)
  const sort = useStore((s) => s.sort)
  return useMemo(() => {
    const q = query.trim()
    // Buscando: en todo el llavero (menos la papelera) y por relevancia
    if (q) return search(entries.filter((e) => !e.deleted && !e.archived), q)
    const hit = entries.filter((e) => inSection(e, section))
    const cmp: Record<Sort, (a: EntrySummary, b: EntrySummary) => number> = {
      title: (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: 'base' }),
      modified: (a, b) => b.modified - a.modified,
      used: (a, b) => (b.used ?? 0) - (a.used ?? 0) || a.title.localeCompare(b.title),
      created: (a, b) => b.created - a.created
    }
    return [...hit].sort(cmp[sort])
  }, [entries, section, query, sort])
}

export function EntryList(): React.JSX.Element {
  const t = useT()
  const section = useStore((s) => s.section)
  const selected = useStore((s) => s.selected)
  const query = useStore((s) => s.query)
  const sort = useStore((s) => s.sort)
  const folders = useStore((s) => s.info?.folders ?? [])
  const list = useVisible()
  const multi = useStore((s) => s.multi)
  const drag = useEntryDrag()

  // Clic: elegir. Ctrl+clic: añadir o quitar de la selección. Mayús+clic: el tramo desde la elegida.
  const pick = (ev: React.MouseEvent, id: string): void => {
    if (drag.dragged()) return
    const s = useStore.getState()
    if (ev.ctrlKey || ev.metaKey) {
      const base = s.multi.length ? s.multi : s.selected ? [s.selected] : []
      const next = base.includes(id) ? base.filter((x) => x !== id) : [...base, id]
      return setState({ multi: next, selected: id, editing: null })
    }
    if (ev.shiftKey && s.selected) {
      const a = list.findIndex((x) => x.id === s.selected)
      const b = list.findIndex((x) => x.id === id)
      if (a >= 0 && b >= 0) return setState({ multi: list.slice(Math.min(a, b), Math.max(a, b) + 1).map((x) => x.id), editing: null })
    }
    void leaveEditor({ selected: id, editing: null, multi: [] })
  }

  const title = query
    ? t('common.search').replace('…', '')
    : section.startsWith('folder:')
      ? (folders.find((f) => f.id === section.slice(7))?.name ?? '')
      : t(`nav.${section}`)

  const newKind = section === 'card' || section === 'note' || section === 'identity' ? section : 'login'

  return (
    <section className="list">
      <div className="list-head">
        <h2>{title}</h2>
        <Select
          size="sm"
          value={sort}
          onChange={(v: Sort) => setState({ sort: v })}
          options={[
            { value: 'title', label: t('list.sortTitle') },
            { value: 'modified', label: t('list.sortModified') },
            { value: 'used', label: t('list.sortUsed') },
            { value: 'created', label: t('list.sortCreated') }
          ]}
        />
        {section === 'trash' ? (
          <button
            className="btn btn-sm btn-danger"
            aria-label={t('list.emptyTrash')}
            data-tip={t('list.emptyTrash')}
            disabled={!list.length}
            onClick={async () => {
              if (await ask(t('list.emptyTrashConfirm'), t('list.emptyTrash'))) {
                await attempt(api.trashEmpty())
                await refreshEntries()
              }
            }}
          >
            <Trash2 size={14} />
          </button>
        ) : (
          <button className="btn btn-sm btn-primary" onClick={() => void leaveEditor({ editing: { id: null, kind: newKind }, selected: null })} data-tip="Ctrl+N">
            <Plus size={15} /> {t('list.new')}
          </button>
        )}
      </div>
      {section === 'trash' && list.length > 0 && <div className="hint" style={{ padding: '0 16px 8px', marginTop: 0 }}>{t('list.trashHint')}</div>}
      <div className="list-body" role="listbox">
        {list.length === 0 ? (
          <div className="empty">
            {query ? <SearchX size={34} /> : <Vault size={34} />}
            <b>{query ? t('list.noResults', { q: query }) : t('list.empty')}</b>
            {!query && section !== 'trash' && <span className="small">{t('list.emptyHint')}</span>}
          </div>
        ) : (
          list.map((e) => (
            <button
              key={e.id}
              role="option"
              aria-selected={selected === e.id}
              data-id={e.id}
              className={`row-entry ${selected === e.id || multi.includes(e.id) ? 'on' : ''}`}
              onPointerDown={(ev) => drag.start(ev, e.id)}
              onClick={(ev) => pick(ev, e.id)}
            >
              <Avatar name={e.title} icon={e.icon} />
              <span className="txt">
                <b>{e.title}</b>
                <small>{e.subtitle || (e.url ? e.url.replace(/^https?:\/\/(www\.)?/, '') : t(`kind.${e.kind}`))}</small>
              </span>
              {e.favorite && <Star size={14} className="star" fill="currentColor" />}
            </button>
          ))
        )}
      </div>
    </section>
  )
}
