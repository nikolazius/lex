/// <reference lib="webworker" />
// Owns the dictionary: downloads the pack into OPFS, then answers searches through the WASM core.

type Part = { name: string; size: number; raw: number }
type Manifest = {
  version: string
  entries: number
  pairs: number[]
  download: number
  installed: number
  files: { name: string; size: number; parts: Part[] }[]
}
type Current = Omit<Manifest, 'files'>

type Sync = FileSystemSyncAccessHandle
type Source = Sync | Uint8Array

const CURRENT = 'current.json'
const PART_RAW = 4 << 20
const ON_DEMAND = ['postings.bin', 'entries.bin', 'forms.bin'] // order = file ids in the core
const IN_MEMORY = ['index.fst', 'forms.fst', 'blocks.idx']

let base = ''
let sources: Source[] = []
let wasm: {
  memory: WebAssembly.Memory
  alloc(len: number): number
  init(a: number, al: number, b: number, bl: number, c: number, cl: number): number
  search(q: number, len: number, filter: number, limit: number): number
} | null = null
let installing = false
const encoder = new TextEncoder()
const decoder = new TextDecoder()

const post = (m: unknown) => (self as DedicatedWorkerGlobalScope).postMessage(m)
const root = () => navigator.storage.getDirectory()

async function readBytes(dir: FileSystemDirectoryHandle, name: string): Promise<Uint8Array | null> {
  try {
    const file = await (await dir.getFileHandle(name)).getFile()
    return new Uint8Array(await file.arrayBuffer())
  } catch {
    return null
  }
}

async function openSync(dir: FileSystemDirectoryHandle, name: string, create = false): Promise<Sync> {
  return (await dir.getFileHandle(name, { create })).createSyncAccessHandle()
}

async function writeSmall(dir: FileSystemDirectoryHandle, name: string, text: string) {
  const h = await openSync(dir, name, true)
  h.truncate(0)
  h.write(encoder.encode(text), { at: 0 })
  h.flush()
  h.close()
}

function closeSources() {
  for (const s of sources) if (!(s instanceof Uint8Array)) s.close()
  sources = []
  wasm = null
}

// ---- loading ---------------------------------------------------------------

async function load(): Promise<Current | null> {
  const dirRoot = await root()
  const raw = await readBytes(dirRoot, CURRENT)
  if (!raw) return null
  const current: Current = JSON.parse(decoder.decode(raw))
  const dir = await dirRoot.getDirectoryHandle('pack-' + current.version)

  const [mem, ondemand, instance] = await Promise.all([
    Promise.all(IN_MEMORY.map((n) => readBytes(dir, n))),
    Promise.all(
      ON_DEMAND.map(async (n): Promise<Source> => {
        try {
          return await openSync(dir, n)
        } catch {
          // Another tab holds the file: fall back to an in-memory copy.
          const bytes = await readBytes(dir, n)
          if (!bytes) throw new Error('missing ' + n)
          return bytes
        }
      }),
    ),
    instantiate(),
  ])
  if (mem.some((m) => !m)) throw new Error('pack is incomplete')
  sources = ondemand
  wasm = instance.exports as unknown as typeof wasm
  const args: number[] = []
  for (const bytes of mem as Uint8Array[]) {
    const ptr = wasm!.alloc(bytes.length)
    new Uint8Array(wasm!.memory.buffer, ptr, bytes.length).set(bytes)
    args.push(ptr, bytes.length)
  }
  if (wasm!.init(args[0], args[1], args[2], args[3], args[4], args[5]) < 0) throw new Error('pack was rejected')
  return current
}

async function instantiate(): Promise<WebAssembly.Instance> {
  const imports = {
    env: {
      host_read_at(file: number, off: number, ptr: number, len: number): number {
        const src = sources[file]
        const out = new Uint8Array(wasm!.memory.buffer, ptr, len)
        if (src instanceof Uint8Array) {
          const slice = src.subarray(off, off + len)
          out.set(slice)
          return slice.length
        }
        return src.read(out, { at: off })
      },
    },
  }
  const url = new URL('core.wasm', base)
  try {
    return (await WebAssembly.instantiateStreaming(fetch(url), imports)).instance
  } catch {
    return (await WebAssembly.instantiate(await (await fetch(url)).arrayBuffer(), imports)).instance
  }
}

// ---- install ---------------------------------------------------------------

async function fetchManifest(): Promise<{ manifest: Manifest; dirUrl: URL }> {
  const latest = await (await fetch(new URL('packs/latest.json', base), { cache: 'no-store' })).json()
  const dirUrl = new URL(`packs/${latest.version}/`, base)
  const res = await fetch(new URL('manifest.json', dirUrl), { cache: 'no-store' })
  if (!res.ok) throw new Error('manifest ' + res.status)
  return { manifest: await res.json(), dirUrl }
}

async function gunzip(buf: Uint8Array<ArrayBuffer>): Promise<Uint8Array> {
  // Some hosts already decode .gz transparently; only inflate when the gzip magic is present.
  if (buf[0] !== 0x1f || buf[1] !== 0x8b) return buf
  const stream = new Blob([buf]).stream().pipeThrough(new DecompressionStream('gzip'))
  return new Uint8Array(await new Response(stream).arrayBuffer())
}

async function install() {
  if (installing) return
  installing = true
  const open = new Map<string, Sync>()
  try {
    const { manifest, dirUrl } = await fetchManifest()
    const dirRoot = await root()
    const dir = await dirRoot.getDirectoryHandle('pack-' + manifest.version, { create: true })

    // Parts finished in an earlier, interrupted run are skipped.
    const doneRaw = await readBytes(dir, 'done.json')
    const done = new Set<string>(doneRaw ? JSON.parse(decoder.decode(doneRaw) || '[]') : [])
    const doneHandle = await openSync(dir, 'done.json', true)
    open.set('done.json', doneHandle)
    const saveDone = () => {
      const bytes = encoder.encode(JSON.stringify([...done]))
      doneHandle.truncate(0)
      doneHandle.write(bytes, { at: 0 })
      doneHandle.flush()
    }

    const queue: { file: string; part: Part; at: number }[] = []
    let got = 0
    for (const f of manifest.files) {
      open.set(f.name, await openSync(dir, f.name, true))
      f.parts.forEach((part, i) => {
        if (done.has(part.name)) got += part.size
        else queue.push({ file: f.name, part, at: i * PART_RAW })
      })
    }
    let lastPost = 0
    const progress = (force = false) => {
      const now = performance.now()
      if (force || now - lastPost > 80) {
        lastPost = now
        post({ type: 'progress', got, total: manifest.download })
      }
    }
    progress(true)

    const fetchPart = async (part: Part): Promise<Uint8Array> => {
      for (let attempt = 0; ; attempt++) {
        let counted = 0
        try {
          const res = await fetch(new URL(part.name, dirUrl))
          if (!res.ok || !res.body) throw new Error(`${part.name}: HTTP ${res.status}`)
          const reader = res.body.getReader()
          const chunks: Uint8Array[] = []
          let len = 0
          for (;;) {
            const { done: end, value } = await reader.read()
            if (end) break
            chunks.push(value)
            len += value.length
            const step = Math.min(value.length, part.size - counted)
            counted += step
            got += step
            progress()
          }
          const buf = new Uint8Array(len)
          let o = 0
          for (const c of chunks) {
            buf.set(c, o)
            o += c.length
          }
          const raw = await gunzip(buf)
          if (raw.length !== part.raw) throw new Error(`${part.name}: unexpected size`)
          got += part.size - counted
          return raw
        } catch (e) {
          got -= counted
          if (attempt >= 3) throw e
          await new Promise((r) => setTimeout(r, 600 * (attempt + 1)))
        }
      }
    }
    const runner = async () => {
      for (let job = queue.shift(); job; job = queue.shift()) {
        const raw = await fetchPart(job.part)
        open.get(job.file)!.write(raw, { at: job.at })
        done.add(job.part.name)
        saveDone()
      }
    }
    await Promise.all([runner(), runner(), runner(), runner()])

    for (const h of open.values()) {
      h.flush()
      h.close()
    }
    open.clear()
    const { files: _files, ...current } = manifest
    closeSources()
    await writeSmall(dirRoot, CURRENT, JSON.stringify(current))
    // Drop packs from older versions.
    for await (const name of (dirRoot as unknown as { keys(): AsyncIterable<string> }).keys()) {
      if (name.startsWith('pack-') && name !== 'pack-' + manifest.version) {
        await dirRoot.removeEntry(name, { recursive: true }).catch(() => {})
      }
    }
    progress(true)
    post({ type: 'ready', info: await load() })
  } catch (e) {
    for (const h of open.values()) h.close()
    post({ type: 'error', message: e instanceof Error ? e.message : String(e) })
  } finally {
    installing = false
  }
}

async function remove() {
  closeSources()
  const dirRoot = await root()
  for await (const name of (dirRoot as unknown as { keys(): AsyncIterable<string> }).keys()) {
    await dirRoot.removeEntry(name, { recursive: true }).catch(() => {})
  }
  await boot()
}

async function boot() {
  let info: Current | null = null
  try {
    info = await load()
  } catch {
    closeSources()
  }
  if (info) {
    post({ type: 'ready', info })
    // Tell the app when a newer pack is published (best effort, needs network).
    fetchManifest()
      .then(({ manifest }) => {
        if (manifest.version !== info!.version) post({ type: 'update', size: manifest.download })
      })
      .catch(() => {})
    return
  }
  let size = 0
  try {
    size = (await fetchManifest()).manifest.download
  } catch {}
  post({ type: 'missing', size })
}

self.onmessage = (e: MessageEvent) => {
  const m = e.data
  if (m.type === 'search') {
    if (!wasm) return post({ type: 'result', seq: m.seq, json: '{"groups":[]}' })
    const q = encoder.encode(m.q)
    const ptr = wasm.alloc(q.length)
    new Uint8Array(wasm.memory.buffer, ptr, q.length).set(q)
    const out = wasm.search(ptr, q.length, m.filter, m.limit)
    const len = new DataView(wasm.memory.buffer).getUint32(out, true)
    post({ type: 'result', seq: m.seq, json: decoder.decode(new Uint8Array(wasm.memory.buffer, out + 4, len)) })
  } else if (m.type === 'boot') {
    base = m.base
    boot()
  } else if (m.type === 'install') {
    install()
  } else if (m.type === 'remove') {
    remove()
  }
}
