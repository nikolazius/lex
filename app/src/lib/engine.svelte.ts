// Main-thread side of the dictionary worker.

export type Item = [id: number, a: string, b: string, src: number, tier: number]
export type Group = { pair: number; side: number; items: Item[] }
export type PackInfo = { version: string; entries: number; pairs: number[]; download: number; installed: number }

export const engine = $state({
  phase: 'boot' as 'boot' | 'missing' | 'downloading' | 'ready',
  info: null as PackInfo | null,
  size: 0,
  got: 0,
  total: 0,
  error: '',
  updateSize: 0,
})

const worker = new Worker(new URL('../worker.ts', import.meta.url), { type: 'module' })
let seq = 0
let waiting: { seq: number; resolve: (g: Group[] | null) => void } | null = null
let wakeLock: { release(): Promise<void> } | null = null

worker.onmessage = (e: MessageEvent) => {
  const m = e.data
  switch (m.type) {
    case 'result':
      if (waiting && waiting.seq === m.seq) {
        waiting.resolve(JSON.parse(m.json).groups)
        waiting = null
      }
      break
    case 'ready':
      engine.info = m.info
      engine.phase = 'ready'
      engine.updateSize = 0
      wakeLock?.release().catch(() => {})
      wakeLock = null
      break
    case 'missing':
      engine.size = m.size
      engine.phase = 'missing'
      break
    case 'progress':
      engine.got = m.got
      engine.total = m.total
      break
    case 'update':
      engine.updateSize = m.size
      break
    case 'error':
      engine.error = m.message
      wakeLock?.release().catch(() => {})
      wakeLock = null
      break
  }
}

worker.postMessage({ type: 'boot', base: document.baseURI })

/** Resolves with the result groups, or null when a newer search superseded this one. */
export function search(q: string, filter: number, limit: number): Promise<Group[] | null> {
  waiting?.resolve(null)
  return new Promise((resolve) => {
    waiting = { seq: ++seq, resolve }
    worker.postMessage({ type: 'search', seq, q, filter, limit })
  })
}

export async function install() {
  engine.error = ''
  engine.phase = 'downloading'
  navigator.storage?.persist?.().catch(() => {})
  try {
    wakeLock = await (navigator as any).wakeLock?.request('screen')
  } catch {}
  worker.postMessage({ type: 'install' })
}

export function removeData() {
  engine.phase = 'boot'
  engine.info = null
  worker.postMessage({ type: 'remove' })
}
