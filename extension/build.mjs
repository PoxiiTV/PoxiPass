// Compila la extensión en extension/dist (cargable como «descomprimida»).
// Uso: node extension/build.mjs            → dist con `key` (ID fijo de desarrollo)
//      node extension/build.mjs --store    → además, poxipass-extension-X.Y.Z.zip sin `key` para las tiendas
import { build } from 'vite'
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('.', import.meta.url))
const out = join(root, 'dist')
const store = process.argv.includes('--store')
const { version } = JSON.parse(readFileSync(join(root, '..', 'package.json'), 'utf8'))

rmSync(out, { recursive: true, force: true })
// Un IIFE por entrada: los content scripts y el service worker clásico no admiten módulos
for (const name of ['background', 'content', 'popup']) {
  await build({
    configFile: false,
    logLevel: 'warn',
    publicDir: false,
    build: {
      outDir: out,
      emptyOutDir: false,
      target: 'es2022',
      minify: true,
      lib: { entry: join(root, 'src', `${name}.ts`), formats: ['iife'], name: `poxipass_${name}`, fileName: () => `${name}.js` }
    }
  })
}

const manifest = JSON.parse(readFileSync(join(root, 'manifest.json'), 'utf8'))
manifest.version = version
for (const f of ['popup.html', '_locales', 'icons']) cpSync(join(root, f), join(out, f), { recursive: true })
cpSync(join(root, '..', 'node_modules', '@fontsource-variable', 'nunito', 'files', 'nunito-latin-wght-normal.woff2'), join(out, 'nunito.woff2'))
writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 2))
console.log(`Extensión ${version} en ${out}`)

if (store) {
  // Las tiendas no admiten `key`: asignan su propio ID (hay que añadirlo a EXTENSION_IDS en bridge.rs)
  const tmp = join(root, 'store-tmp')
  rmSync(tmp, { recursive: true, force: true })
  mkdirSync(tmp)
  cpSync(out, tmp, { recursive: true })
  delete manifest.key
  writeFileSync(join(tmp, 'manifest.json'), JSON.stringify(manifest, null, 2))
  const zip = join(root, `poxipass-extension-${version}.zip`)
  rmSync(zip, { force: true })
  execFileSync('powershell', ['-NoProfile', '-Command', `Compress-Archive -Path '${tmp}\\*' -DestinationPath '${zip}'`], { stdio: 'inherit' })
  rmSync(tmp, { recursive: true, force: true })
  console.log(`Paquete para las tiendas: ${zip}`)
}
