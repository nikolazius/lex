// Recent searches and starred entries, kept in localStorage (small, synchronous, survives pack updates).

export type Saved = { pair: number; a: string; b: string; src: number }

function load<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) ?? '') as T
  } catch {
    return fallback
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {}
}

export const library = $state({
  recents: load<string[]>('lex.recents', []),
  stars: load<Saved[]>('lex.stars', []),
})

const same = (x: Saved, y: Saved) => x.pair === y.pair && x.a === y.a && x.b === y.b

export function addRecent(q: string) {
  const text = q.trim()
  if (!text) return
  library.recents = [text, ...library.recents.filter((r) => r.toLowerCase() !== text.toLowerCase())].slice(0, 30)
  save('lex.recents', library.recents)
}

export function clearRecents() {
  library.recents = []
  save('lex.recents', [])
}

export function isStarred(e: Saved): boolean {
  return library.stars.some((s) => same(s, e))
}

export function toggleStar(e: Saved) {
  library.stars = isStarred(e)
    ? library.stars.filter((s) => !same(s, e))
    : [{ pair: e.pair, a: e.a, b: e.b, src: e.src }, ...library.stars]
  save('lex.stars', library.stars)
}
