import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import '@fontsource-variable/nunito'
import './styles/app.css'
import { api } from './lib/api'
import { enterVault, getState, setState } from './lib/store'
import { applyTheme, followSystem } from './lib/theme'
import { startUpdateChecks } from './lib/update'
import { App } from './App'
import { Quick } from './pages/Quick'

// Nada del navegador: ni su menú contextual ni sus atajos (imprimir, buscar, recargar, guardar página…)
window.addEventListener('contextmenu', (e) => {
  if (!import.meta.env.DEV || !e.shiftKey) e.preventDefault()
})
window.addEventListener('keydown', (e) => {
  const k = e.key.toLowerCase()
  if (e.key === 'F5' || e.key === 'F3' || e.key === 'F7' || (e.ctrlKey && ['r', 'p', 's', 'g', 'u', 'j', 'h', 'o', 'd'].includes(k))) e.preventDefault()
})

async function boot(): Promise<void> {
  // Defensa extra: si Rust aún no estuviera listo, se reintenta en vez de quedarse la ventana en negro
  for (let i = 0; ; i++) {
    try {
      return await start()
    } catch (e) {
      if (i >= 10) throw e
      await new Promise((r) => setTimeout(r, 200))
    }
  }
}

async function start(): Promise<void> {
  // La barra de acceso rápido es otra ventana con el mismo código: arranca sola y se enseña con su atajo
  if (location.hash === '#quick') {
    document.documentElement.classList.add('quick-root')
    createRoot(document.getElementById('root')!).render(<Quick />)
    return
  }
  const [app, config, recents, info] = await Promise.all([api.appInfo(), api.configGet(), api.recentList(), api.vaultInfo()])
  applyTheme(config)
  followSystem(() => getState().config)
  // El último llavero que se usó, aunque su archivo falte (así sale «Restaurar desde una copia»); los demás
  // están en la lista de recientes del desbloqueo
  const last = recents[0]
  setState({ app, config, unlockPath: info?.path ?? last?.path ?? null })
  if (info) {
    await enterVault(info)
  } else {
    setState({ screen: last ? 'unlock' : 'welcome' })
  }
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>
  )
  // Se enseña la ventana ya pintada (sin parpadeo blanco), salvo si arrancó con Windows en la bandeja
  if (!app.startHidden) {
    requestAnimationFrame(() => void getCurrentWindow().show())
  }
  startUpdateChecks()
}

void boot()
