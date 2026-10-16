import { useEffect, useState } from 'react'
import { ArrowUp, ChevronRight, Download, File, FileKey2, FileText, Folder, FolderPlus, HardDrive, Home, Monitor } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { api } from '../lib/api'
import { ask, setState, useStore } from '../lib/store'
import { fail } from '../lib/actions'
import { bytes, date } from '../lib/format'
import { useT } from '../i18n'
import { Modal } from './ui'

interface Place {
  kind: string
  name: string
  path: string
}
interface Item {
  name: string
  path: string
  dir: boolean
  size: number
  modified: number
}

const PLACE_ICON: Record<string, LucideIcon> = { documents: FileText, desktop: Monitor, downloads: Download, home: Home, drive: HardDrive }
const LAST_DIR = 'poxipass.lastDir'

function lastDir(): string | null {
  try {
    return localStorage.getItem(LAST_DIR)
  } catch {
    return null
  }
}

/** Explorador de archivos propio (abrir y guardar), en vez de los cuadros de Windows. */
export function FilePicker(): React.JSX.Element | null {
  const t = useT()
  const req = useStore((s) => s.picker)
  const [places, setPlaces] = useState<Place[]>([])
  const [dir, setDir] = useState<string | null>(null)
  const [items, setItems] = useState<Item[]>([])
  const [parent, setParent] = useState<string | null>(null)
  const [sel, setSel] = useState<string | null>(null)
  const [name, setName] = useState('')
  /** Ruta escrita a mano (al pulsar en la barra de ruta) */
  const [typing, setTyping] = useState<string | null>(null)
  /** Nombre de la carpeta nueva que se está creando */
  const [newDir, setNewDir] = useState<string | null>(null)
  /** Fila con el foco del teclado */
  const [cursor, setCursor] = useState(0)

  useEffect(() => {
    if (!req) return
    setName(req.name ?? '')
    setSel(null)
    void api.fsPlaces().then((p) => {
      setPlaces(p)
      setDir(req.dir ?? lastDir() ?? p.find((x) => x.kind === 'documents')?.path ?? p[0]?.path ?? null)
    })
  }, [req])

  useEffect(() => {
    if (!req || !dir) return
    let live = true
    api
      .fsList(dir, req.exts)
      .then((l) => {
        if (!live) return
        setItems(req.mode === 'folder' ? l.items.filter((i) => i.dir) : l.items)
        setParent(l.parent)
        setSel(null)
        setCursor(0)
        try {
          localStorage.setItem(LAST_DIR, l.path)
        } catch {
          /* sin almacenamiento: no pasa nada */
        }
      })
      .catch((e) => {
        if (live) fail(e)
      })
    return () => {
      live = false
    }
  }, [dir, req])

  if (!req) return null

  const done = (p: string | null): void => {
    setState({ picker: null })
    req.resolve(p)
  }
  const confirm = async (): Promise<void> => {
    if (req.mode === 'open') {
      if (sel) done(sel)
      return
    }
    if (req.mode === 'folder') {
      if (dir) done(dir)
      return
    }
    if (!dir || !name.trim()) return
    try {
      const r = await api.fsSavePath(dir, name, req.ext ?? '')
      if (r.exists && !(await ask(t('picker.overwrite', { n: r.path.split('\\').pop() ?? '' }), t('picker.replace')))) return
      done(r.path)
    } catch (e) {
      fail(e)
    }
  }

  const createDir = async (): Promise<void> => {
    if (!dir || !newDir?.trim()) return setNewDir(null)
    try {
      const p = await api.fsMkdir(dir, newDir)
      setNewDir(null)
      setDir(p)
    } catch (e) {
      fail(e)
    }
  }

  // Teclado: flechas para moverse, Enter para entrar/abrir, Retroceso para subir
  const onListKey = (e: React.KeyboardEvent): void => {
    const it = items[cursor]
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const next = Math.max(0, Math.min(items.length - 1, cursor + (e.key === 'ArrowDown' ? 1 : -1)))
      setCursor(next)
      const nx = items[next]
      if (nx && !nx.dir) {
        setSel(nx.path)
        if (req.mode === 'save') setName(nx.name)
      }
      document.querySelectorAll('.files .file')[next]?.scrollIntoView({ block: 'nearest' })
    } else if (e.key === 'Enter' && it) {
      e.preventDefault()
      if (it.dir) setDir(it.path)
      else if (req.mode === 'open') done(it.path)
      else void confirm()
    } else if (e.key === 'Backspace' && parent) {
      e.preventDefault()
      setDir(parent)
    }
  }

  const parts = (dir ?? '').split('\\').filter(Boolean)
  const crumbPath = (i: number): string => {
    const p = parts.slice(0, i + 1).join('\\')
    return i === 0 ? `${p}\\` : p
  }

  return (
    <Modal onClose={() => done(null)} className="picker">
      <div className="picker-head">
        <h2>{req.title}</h2>
        <button className="icon-btn" disabled={!parent} data-tip={t('picker.up')} aria-label={t('picker.up')} onClick={() => parent && setDir(parent)}>
          <ArrowUp size={16} />
        </button>
        {typing !== null ? (
          <input
            className="input small crumbs-input"
            autoFocus
            spellCheck={false}
            value={typing}
            onChange={(e) => setTyping(e.target.value)}
            onBlur={() => setTyping(null)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && typing.trim()) {
                setDir(typing.trim())
                setTyping(null)
              } else if (e.key === 'Escape') {
                e.stopPropagation()
                setTyping(null)
              }
            }}
          />
        ) : (
          // Clic en el hueco de la barra: escribir o pegar una ruta
          <div className="crumbs" onClick={(e) => e.target === e.currentTarget && setTyping(dir ?? '')} data-tip={t('picker.typePath')}>
            {parts.map((p, i) => (
              <span key={i} className="row" style={{ gap: 2 }}>
                {i > 0 && <ChevronRight size={13} />}
                <button onClick={() => setDir(crumbPath(i))}>{p}</button>
              </span>
            ))}
          </div>
        )}
        {req.mode !== 'open' && (
          <button className="icon-btn" data-tip={t('picker.newFolder')} aria-label={t('picker.newFolder')} onClick={() => setNewDir('')}>
            <FolderPlus size={16} />
          </button>
        )}
      </div>
      <div className="places">
        {places.map((p) => {
          const Icon = PLACE_ICON[p.kind] ?? Folder
          return (
            <button key={p.path} className={`nav-item ${dir === p.path ? 'active' : ''}`} onClick={() => setDir(p.path)}>
              <Icon size={16} />
              <span className="name">{p.kind === 'drive' ? `${t('picker.drive')} ${p.name}` : t(`picker.${p.kind}`)}</span>
            </button>
          )
        })}
      </div>
      <div className="files" tabIndex={0} onKeyDown={onListKey} role="listbox" aria-label={req.title}>
        {newDir !== null && (
          <div className="file dir on">
            <Folder size={17} />
            <input
              className="input small"
              style={{ height: 30 }}
              autoFocus
              value={newDir}
              placeholder={t('picker.newFolder')}
              onChange={(e) => setNewDir(e.target.value)}
              onBlur={() => void createDir()}
              onKeyDown={(e) => {
                e.stopPropagation()
                if (e.key === 'Enter') void createDir()
                if (e.key === 'Escape') setNewDir(null)
              }}
            />
          </div>
        )}
        {items.length === 0 && newDir === null && <div className="empty small">{t('picker.empty')}</div>}
        {items.map((it) => {
          const Icon = it.dir ? Folder : it.name.toLowerCase().endsWith('.poxipass') ? FileKey2 : File
          return (
            <button
              key={it.path}
              className={`file ${it.dir ? 'dir' : ''} ${sel === it.path || (cursor === items.indexOf(it) && it.dir) ? 'on' : ''}`}
              tabIndex={-1}
              onClick={() => {
                if (it.dir) return setDir(it.path)
                setSel(it.path)
                if (req.mode === 'save') setName(it.name)
              }}
              onDoubleClick={() => !it.dir && req.mode === 'open' && done(it.path)}
            >
              <Icon size={17} />
              <span className="n">{it.name}</span>
              {!it.dir && <span className="muted small num">{bytes(it.size)}</span>}
              {!it.dir && it.modified > 0 && <span className="muted small" style={{ width: 120, textAlign: 'right' }}>{date(it.modified)}</span>}
            </button>
          )
        })}
      </div>
      <div className="picker-foot">
        {req.mode === 'save' ? (
          <input
            className="input"
            value={name}
            autoFocus
            spellCheck={false}
            placeholder={t('picker.fileName')}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && void confirm()}
          />
        ) : (
          <span className="spacer muted small">{req.mode === 'folder' ? dir : sel ? sel.split('\\').pop() : t('picker.choose')}</span>
        )}
        <button className="btn btn-ghost" onClick={() => done(null)}>
          {t('common.cancel')}
        </button>
        <button className="btn btn-primary" disabled={req.mode === 'open' ? !sel : req.mode === 'save' ? !name.trim() : !dir} onClick={() => void confirm()}>
          {req.mode === 'open' ? t('picker.open') : req.mode === 'folder' ? t('picker.useFolder') : t('common.save')}
        </button>
      </div>
    </Modal>
  )
}
