export type Lang = 'de' | 'en' | 'ru'

export const PAIRS: [Lang, Lang][] = [
  ['de', 'en'],
  ['de', 'ru'],
  ['en', 'ru'],
]
export const LANG_NAME: Record<Lang, string> = { de: 'Deutsch', en: 'English', ru: 'Русский' }
export const LANG_CODE: Record<Lang, string> = { de: 'DE', en: 'EN', ru: 'RU' }
const VOICE: Record<Lang, string> = { de: 'de-DE', en: 'en-US', ru: 'ru-RU' }

export const SOURCES: [bit: number, name: string][] = [
  [1, 'Ding'],
  [2, 'English Wiktionary'],
  [4, 'Russian Wiktionary'],
  [8, 'OpenRussian'],
  [16, 'WikDict'],
]

const OPEN = '{[(<'
const CLOSE = '}])>'

/** Splits display text into plain runs and dimmed annotation runs ({m}, [tech.], (hint)). */
export function segments(s: string): { t: string; dim: boolean }[] {
  const out: { t: string; dim: boolean }[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < s.length; i++) {
    const c = s[i]
    if (OPEN.includes(c)) {
      if (depth === 0 && i > start) out.push({ t: s.slice(start, i), dim: false })
      if (depth === 0) start = i
      depth++
    } else if (CLOSE.includes(c) && depth > 0) {
      depth--
      if (depth === 0) {
        out.push({ t: s.slice(start, i + 1), dim: true })
        start = i + 1
      }
    }
  }
  if (start < s.length) out.push({ t: s.slice(start), dim: depth > 0 })
  return out
}

/** Text without annotations and stress marks. */
export function plain(s: string): string {
  return segments(s)
    .filter((x) => !x.dim)
    .map((x) => x.t)
    .join(' ')
    .replace(/\u0301/g, '')
    .replace(/\s+/g, ' ')
    .replace(/\s+([;,])/g, '$1')
    .trim()
}

/** Individual words or phrases of one side, for "look up" chips. */
export function alternatives(s: string): string[] {
  const seen = new Set<string>()
  for (const part of plain(s).split(/[;,]/)) {
    const t = part
      .trim()
      .replace(/^to /, '')
      .replace(/\b(sth|sb|etw|jdn|jdm|jds)\.?/g, '')
      .replace(/\s+/g, ' ')
      .trim()
    if (t.length > 1) seen.add(t)
  }
  return [...seen].slice(0, 8)
}

export function speak(text: string, lang: Lang) {
  if (!('speechSynthesis' in window)) return
  const u = new SpeechSynthesisUtterance(plain(text).replace(/\b(sth|sb|etw|jdn|jdm)\./g, ''))
  u.lang = VOICE[lang]
  const voice = speechSynthesis.getVoices().find((v) => v.lang.replace('_', '-').startsWith(VOICE[lang].slice(0, 2)))
  if (voice) u.voice = voice
  speechSynthesis.cancel()
  speechSynthesis.speak(u)
}

export const canSpeak = typeof window !== 'undefined' && 'speechSynthesis' in window

export function megabytes(bytes: number): string {
  return `${Math.round(bytes / 1048576)} MB`
}
