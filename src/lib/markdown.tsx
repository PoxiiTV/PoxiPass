import { openUrl } from '@tauri-apps/plugin-opener'

// Markdown mínimo y seguro para las notas: se pinta con componentes de React, nunca con HTML, así que el texto
// de una nota no puede ejecutar nada ni cargar nada de fuera. Títulos, listas, **negrita**, *cursiva*,
// `código` y enlaces http(s) (que se abren en el navegador).

function inline(text: string, key: string): React.ReactNode[] {
  const out: React.ReactNode[] = []
  const re = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|\[[^\]]+\]\(https?:\/\/[^)\s]+\)|https?:\/\/[^\s)]+)/g
  let last = 0
  let i = 0
  for (const m of text.matchAll(re)) {
    if (m.index! > last) out.push(text.slice(last, m.index))
    const t = m[0]
    const k = `${key}-${i++}`
    if (t.startsWith('**')) out.push(<b key={k}>{t.slice(2, -2)}</b>)
    else if (t.startsWith('`')) out.push(<code key={k}>{t.slice(1, -1)}</code>)
    else if (t.startsWith('*')) out.push(<i key={k}>{t.slice(1, -1)}</i>)
    else {
      const link = /^\[([^\]]+)\]\((.+)\)$/.exec(t)
      const href = link ? link[2] : t
      out.push(
        <a
          key={k}
          href={href}
          onClick={(e) => {
            e.preventDefault()
            void openUrl(href)
          }}
        >
          {link ? link[1] : t}
        </a>
      )
    }
    last = m.index! + t.length
  }
  if (last < text.length) out.push(text.slice(last))
  return out
}

export function Markdown({ text }: { text: string }): React.JSX.Element {
  const blocks: React.ReactNode[] = []
  let list: { ordered: boolean; items: string[] } | null = null
  const flush = (): void => {
    if (!list) return
    const items = list.items.map((it, i) => <li key={i}>{inline(it, `li${blocks.length}-${i}`)}</li>)
    blocks.push(list.ordered ? <ol key={blocks.length}>{items}</ol> : <ul key={blocks.length}>{items}</ul>)
    list = null
  }
  for (const raw of text.split('\n')) {
    const line = raw.trimEnd()
    const h = /^(#{1,3})\s+(.*)$/.exec(line)
    const ul = /^\s*[-*]\s+(.*)$/.exec(line)
    const ol = /^\s*\d+[.)]\s+(.*)$/.exec(line)
    if (ul || ol) {
      const ordered = !!ol
      if (list && list.ordered !== ordered) flush()
      list ??= { ordered, items: [] }
      list.items.push((ul ?? ol)![1])
      continue
    }
    flush()
    if (h) {
      const Tag = (['h3', 'h4', 'h5'] as const)[h[1].length - 1]
      blocks.push(<Tag key={blocks.length}>{inline(h[2], `h${blocks.length}`)}</Tag>)
    } else if (line) {
      blocks.push(<p key={blocks.length}>{inline(line, `p${blocks.length}`)}</p>)
    }
  }
  flush()
  return <div className="md">{blocks}</div>
}
