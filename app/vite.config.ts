import { defineConfig, type Plugin } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { createHash } from 'node:crypto'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

/** Emits sw.js with the list of app-shell files to precache. Dictionary packs are never precached. */
function serviceWorker(): Plugin {
  return {
    name: 'lex-service-worker',
    apply: 'build',
    generateBundle(_, bundle) {
      const hash = createHash('sha256')
      const assets = ['./']
      for (const [name, chunk] of Object.entries(bundle)) {
        if (name.endsWith('.map')) continue
        assets.push('./' + name)
        hash.update(name).update(chunk.type === 'chunk' ? chunk.code : chunk.source)
      }
      const walk = (dir: string, rel: string) => {
        for (const f of readdirSync(dir)) {
          const p = join(dir, f)
          const r = rel + f
          if (r === 'packs' || f.startsWith('.')) continue
          if (statSync(p).isDirectory()) walk(p, r + '/')
          else {
            assets.push('./' + r)
            hash.update(r).update(readFileSync(p))
          }
        }
      }
      walk('public', '')
      const source = readFileSync('src/sw.js', 'utf8')
        .replace('__VERSION__', hash.digest('hex').slice(0, 12))
        .replace('__ASSETS__', JSON.stringify(assets))
      this.emitFile({ type: 'asset', fileName: 'sw.js', source })
    },
  }
}

export default defineConfig({
  base: './',
  plugins: [svelte(), serviceWorker()],
  worker: { format: 'es' },
  build: { target: 'es2022', assetsInlineLimit: 0 },
})
