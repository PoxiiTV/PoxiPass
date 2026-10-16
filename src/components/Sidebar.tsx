import { useState } from 'react'
import { Archive, CreditCard, Folder, FolderPlus, HeartPulse, IdCard, KeyRound, Layers, MoreHorizontal, Pencil, Settings, Star, StickyNote, Trash2, Wand2 } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { api } from '../lib/api'
import { ask, refreshEntries, setState, useStore, type Section } from '../lib/store'
import { attempt, leaveEditor } from '../lib/actions'
import { useT } from '../i18n'
import { KIND_ICON } from '../lib/templates'

export function Sidebar(): React.JSX.Element {
  const t = useT()
  const section = useStore((s) => s.section)
  const entries = useStore((s) => s.entries)
  const folders = useStore((s) => s.info?.folders ?? [])
  const [adding, setAdding] = useState(false)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [name, setName] = useState('')
  const [menuFor, setMenuFor] = useState<string | null>(null)

  const live = entries.filter((e) => !e.deleted)
  const count = (sec: Section): number => {
    if (sec === 'archive') return live.filter((e) => e.archived).length
    if (sec === 'all') return live.filter((e) => !e.archived).length
    if (sec === 'favorites') return live.filter((e) => e.favorite && !e.archived).length
    if (sec === 'trash') return entries.length - live.length
    if (sec.startsWith('folder:')) return live.filter((e) => e.folder === sec.slice(7) && !e.archived).length
    return live.filter((e) => e.kind === sec && !e.archived).length
  }

  const go = (sec: Section): void => void leaveEditor({ section: sec, selected: null, editing: null })

  const item = (sec: Section, Icon: LucideIcon, label: string, showCount = true): React.JSX.Element => (
    <button
      key={sec}
      className={`nav-item ${section === sec ? 'active' : ''}`}
      onClick={() => go(sec)}
      data-tip={label}
      aria-label={label}
      data-drop={sec.startsWith('folder:') || sec === 'favorites' || sec === 'archive' || sec === 'trash' ? sec : undefined}
    >
      <Icon size={17} />
      <span className="name">{label}</span>
      {showCount && <span className="count">{count(sec) || ''}</span>}
    </button>
  )

  const saveFolder = async (id: string | null): Promise<void> => {
    const n = name.trim()
    setAdding(false)
    setRenaming(null)
    setName('')
    if (!n) return
    const list = await attempt(api.folderSave(id, n))
    if (list) setState((s) => ({ info: s.info && { ...s.info, folders: list } }))
  }

  const deleteFolder = async (id: string, n: string): Promise<void> => {
    if (!(await ask(t('nav.deleteFolderConfirm', { n }), t('nav.deleteFolder')))) return
    const list = await attempt(api.folderDelete(id))
    if (list) {
      setState((s) => ({ info: s.info && { ...s.info, folders: list }, section: s.section === `folder:${id}` ? 'all' : s.section }))
      await refreshEntries()
    }
  }

  const folderInput = (id: string | null): React.JSX.Element => (
    <input
      className="input folder-input"
      autoFocus
      value={name}
      maxLength={60}
      placeholder={t('nav.newFolder')}
      onChange={(e) => setName(e.target.value)}
      onBlur={() => void saveFolder(id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
        if (e.key === 'Escape') {
          setAdding(false)
          setRenaming(null)
          setName('')
        }
      }}
    />
  )

  return (
    <nav className="sidebar">
      {item('all', Layers, t('nav.all'))}
      {item('favorites', Star, t('nav.favorites'))}
      <div className="nav-sep" />
      {item('login', KeyRound, t('nav.login'))}
      {item('card', CreditCard, t('nav.card'))}
      {item('note', StickyNote, t('nav.note'))}
      {item('identity', IdCard, t('nav.identity'))}
      {(['wifi', 'ssh', 'license', 'bank', 'document'] as const).filter((k) => count(k) > 0).map((k) => item(k, KIND_ICON[k], t(`nav.${k}`)))}
      <div className="nav-title">
        {t('nav.folders')}
        <button className="icon-btn" data-tip={t('nav.newFolder')} aria-label={t('nav.newFolder')} onClick={() => setAdding(true)}>
          <FolderPlus size={14} />
        </button>
      </div>
      {folders.map((f) =>
        renaming === f.id ? (
          <div key={f.id}>{folderInput(f.id)}</div>
        ) : (
          <div key={f.id} className={`folder-row ${menuFor === f.id ? 'menu-open' : ''}`}>
            {item(`folder:${f.id}`, Folder, f.name)}
            <button className="icon-btn folder-more" aria-label={t('nav.folderMenu')} aria-haspopup="menu" onClick={() => setMenuFor(menuFor === f.id ? null : f.id)}>
              <MoreHorizontal size={15} />
            </button>
            {menuFor === f.id && (
              <div className="dd-menu folder-menu" role="menu" onMouseLeave={() => setMenuFor(null)}>
                <button
                  role="menuitem"
                  onClick={() => {
                    setMenuFor(null)
                    setRenaming(f.id)
                    setName(f.name)
                  }}
                >
                  <span className="row" style={{ gap: 10 }}>
                    <Pencil size={14} /> {t('nav.renameFolder')}
                  </span>
                </button>
                <button
                  role="menuitem"
                  onClick={() => {
                    setMenuFor(null)
                    void deleteFolder(f.id, f.name)
                  }}
                >
                  <span className="row danger-text" style={{ gap: 10 }}>
                    <Trash2 size={14} /> {t('nav.deleteFolder')}
                  </span>
                </button>
              </div>
            )}
          </div>
        )
      )}
      {adding && folderInput(null)}
      <div className="spacer" />
      {item('archive', Archive, t('nav.archive'))}
      {item('trash', Trash2, t('nav.trash'))}
      <div className="nav-sep" />
      {item('generator', Wand2, t('nav.generator'), false)}
      {item('health', HeartPulse, t('nav.health'), false)}
      <button className="nav-item" onClick={() => setState({ settingsOpen: true })} data-tip={t('nav.settings')} aria-label={t('nav.settings')}>
        <Settings size={17} />
        <span className="name">{t('nav.settings')}</span>
      </button>
    </nav>
  )
}
