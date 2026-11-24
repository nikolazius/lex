// App-shell service worker. The placeholders are filled in by vite.config.ts at build time.
const CACHE = 'lex-shell-4be630dea5b8'
const ASSETS = ["./","./assets/index-Cl5rbX9k.js","./assets/index-DIK8I_tG.css","./assets/worker-CuWfomNo.js","./core.wasm","./icons/icon-180.png","./icons/icon-192.png","./icons/icon-512.png","./manifest.webmanifest"]

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
