// App-shell service worker. The placeholders are filled in by vite.config.ts at build time.
const CACHE = 'lex-shell-__VERSION__'
const ASSETS = __ASSETS__

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches
      .open(CACHE)
      .then((cache) => cache.addAll(ASSETS.map((url) => new Request(url, { cache: 'reload' }))))
      .then(() => self.skipWaiting()),
  )
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k.startsWith('lex-shell-') && k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  )
})

self.addEventListener('fetch', (event) => {
  const req = event.request
  const url = new URL(req.url)
  // Dictionary packs go straight to the network; they are stored in OPFS, not in this cache.
  if (req.method !== 'GET' || url.origin !== location.origin || url.pathname.includes('/packs/')) return
  event.respondWith(
    caches.open(CACHE).then(async (cache) => {
      const hit = await cache.match(req.mode === 'navigate' ? './' : req, { ignoreSearch: true })
      return hit || fetch(req)
    }),
  )
})
