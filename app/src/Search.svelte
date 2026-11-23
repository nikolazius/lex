<script lang="ts">
  import { engine, install, removeData, search, type Group, type Item } from './lib/engine.svelte'
  import { addRecent, clearRecents, isStarred, library, toggleStar, type Saved } from './lib/store.svelte'
  import { alternatives, canSpeak, LANG_CODE, LANG_NAME, megabytes, PAIRS, plain, segments, SOURCES, speak } from './lib/text'
  import Sheet from './Sheet.svelte'

  type Selected = Saved & { side: number }

  const FILTERS = [
    { value: -1, label: 'All' },
    { value: 0, label: 'DE–EN' },
    { value: 1, label: 'DE–RU' },
    { value: 2, label: 'EN–RU' },
  ]

  let q = $state('')
  let filter = $state(Number(localStorage.getItem('lex.filter') ?? -1))
  let groups = $state.raw<Group[]>([])
  let searched = $state('')
  let selected = $state<Selected | null>(null)
  let about = $state(false)
  let copied = $state(-1)
  let input: HTMLInputElement
  let list: HTMLElement
  let touching = false

  const empty = $derived(!q.trim())
  const fuzzyOnly = $derived(groups.length > 0 && groups.every((g) => g.items.every((it) => it[4] === 3)))

  function run() {
    const text = q
    if (!text.trim()) {
      groups = []
      searched = ''
      return
    }
    search(text, filter, filter < 0 ? 30 : 80).then((result) => {
      if (!result) return
      groups = result
      searched = text
      list?.scrollTo(0, 0)
    })
  }

  function setFilter(value: number) {
    filter = value
    localStorage.setItem('lex.filter', String(value))
    run()
  }

  function lookup(text: string) {
    selected = null
    q = text
    run()
  }

  function clear() {
    q = ''
    run()
    input.focus()
  }

  function open(g: Group, it: Item) {
    addRecent(q)
    selected = { pair: g.pair, side: g.side, a: it[1], b: it[2], src: it[3] }
    input.blur()
  }

  function sides(e: Selected) {
    const [la, lb] = PAIRS[e.pair]
    const both = [
      { lang: la, text: e.a },
      { lang: lb, text: e.b },
    ]
    return e.side === 0 ? both : both.reverse()
  }

  async function copy(text: string, i: number) {
    try {
      await navigator.clipboard.writeText(plain(text))
      copied = i
      setTimeout(() => (copied = -1), 1200)
    } catch {}
  }

  function wipe() {
    if (confirm('Delete the downloaded dictionary from this device?')) {
      about = false
      removeData()
    }
  }

  $effect(() => {
    input.focus()
  })
</script>

{#snippet rich(text: string)}
  {#each segments(text) as s}{#if s.dim}<i>{s.t}</i>{:else}{s.t}{/if}{/each}
{/snippet}

<div
  class="list"
  role="presentation"
  bind:this={list}
  ontouchstart={() => (touching = true)}
  ontouchend={() => (touching = false)}
  onscroll={() => touching && input.blur()}
>
  {#if empty}
    <header>
      <h1>Lex</h1>
      <button class="icon-btn" aria-label="About" onclick={() => (about = true)}>
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7.5v.5" /></svg>
      </button>
    </header>

    {#if library.stars.length}
      <h2>Starred</h2>
      {#each library.stars as s (s.pair + s.a + s.b)}
        <button class="row" onclick={() => (selected = { ...s, side: 0 })}>
          <span class="l">{@render rich(s.a)}</span>
          <span class="r">{@render rich(s.b)}</span>
        </button>
      {/each}
    {/if}

    {#if library.recents.length}
      <h2>
        Recent
        <button class="h-action" onclick={clearRecents}>Clear</button>
      </h2>
      {#each library.recents as r (r)}
        <button class="recent" onclick={() => lookup(r)}>
          <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>
          <span>{r}</span>
        </button>
      {/each}
    {/if}

    {#if !library.stars.length && !library.recents.length}
      <button class="hint" onclick={() => input.focus()}>
        <span class="big">Type a word</span>
        <span>German, English or Russian — any direction, any form.</span>
        {#if engine.info}
          <span class="stat">{engine.info.entries.toLocaleString('en')} entries · offline</span>
        {/if}
      </button>
    {/if}
  {:else}
    {#if fuzzyOnly}
      <p class="notice">No exact match. Showing similar words.</p>
    {/if}
    {#each groups as g (g.pair * 2 + g.side)}
      {@const langs = g.side === 0 ? PAIRS[g.pair] : [PAIRS[g.pair][1], PAIRS[g.pair][0]]}
      <section>
        <h2>{LANG_NAME[langs[0]]} <span class="arrow">→</span> {LANG_NAME[langs[1]]}</h2>
        {#each g.items as it (it[0])}
          <button class="row" onclick={() => open(g, it)}>
            <span class="l">{@render rich(g.side === 0 ? it[1] : it[2])}</span>
            <span class="r">{@render rich(g.side === 0 ? it[2] : it[1])}</span>
          </button>
        {/each}
      </section>
    {/each}
    {#if searched && !groups.length}
      <p class="none">
        Nothing found for “{searched}”{#if filter >= 0}
          in {FILTERS[filter + 1].label}.
          <button class="h-action" onclick={() => setFilter(-1)}>Search all</button>
        {/if}
      </p>
    {/if}
  {/if}
</div>

<div class="dock">
  <div class="filters" role="tablist">
    {#each FILTERS as f}
      <button
        role="tab"
        aria-selected={filter === f.value}
        class:on={filter === f.value}
        onmousedown={(e) => e.preventDefault()}
        onclick={() => setFilter(f.value)}>{f.label}</button
      >
    {/each}
  </div>
  <form
    class="field"
    role="search"
    onsubmit={(e) => {
      e.preventDefault()
      addRecent(q)
      input.blur()
    }}
  >
    <svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
    <input
      bind:this={input}
      bind:value={q}
      oninput={run}
      type="text"
      inputmode="search"
      enterkeyhint="search"
      placeholder="Search"
      aria-label="Search the dictionary"
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck="false"
    />
    {#if q}
      <button type="button" class="clear" aria-label="Clear" onmousedown={(e) => e.preventDefault()} onclick={clear}>
        <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" class="disc" /><path d="M9 9l6 6M15 9l-6 6" /></svg>
      </button>
    {/if}
  </form>
</div>

{#if selected}
  {@const e = selected}
  <Sheet onclose={() => (selected = null)}>
    {#each sides(e) as s, i}
      <div class="side">
        <div class="lang">{LANG_NAME[s.lang]}</div>
        <div class="term-row">
          <p class="term selectable" lang={s.lang}>{@render rich(s.text)}</p>
          {#if canSpeak}
            <button class="icon-btn" aria-label="Listen in {LANG_NAME[s.lang]}" onclick={() => speak(s.text, s.lang)}>
              <svg viewBox="0 0 24 24"><path d="M4 9.5v5h3.5L12 18.5v-13L7.5 9.5zM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" /></svg>
            </button>
          {/if}
          <button class="icon-btn" aria-label="Copy" onclick={() => copy(s.text, i)}>
            {#if copied === i}
              <svg viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
            {:else}
              <svg viewBox="0 0 24 24"><rect x="8.5" y="8.5" width="11" height="11" rx="2.5" /><path d="M15.5 5.5v-.5a1.5 1.5 0 0 0-1.5-1.5H6A2.5 2.5 0 0 0 3.500 6v8A1.5 1.5 0 0 0 5 15.5h.5" /></svg>
            {/if}
          </button>
        </div>
        {#if alternatives(s.text).length > 1}
          <div class="chips">
            {#each alternatives(s.text) as alt}
              <button onclick={() => lookup(alt)}>{alt}</button>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
    <div class="sheet-foot">
      <button class="star" class:on={isStarred(e)} onclick={() => toggleStar(e)}>
        <svg viewBox="0 0 24 24"><path d="M12 3.8l2.5 5.2 5.7.8-4.100 4 1 5.700L12 16.800 6.900 19.500l1-5.700-4.100-4 5.700-.8z" /></svg>
        {isStarred(e) ? 'Starred' : 'Star'}
      </button>
      <span class="src">{LANG_CODE[PAIRS[e.pair][0]]}–{LANG_CODE[PAIRS[e.pair][1]]} · {SOURCES.filter(([bit]) => e.src & bit).map(([, name]) => name).join(', ')}</span>
    </div>
  </Sheet>
{/if}

{#if about}
  <Sheet onclose={() => (about = false)}>
    <div class="about selectable">
      <h3>Lex</h3>
      {#if engine.info}
        {@const info = engine.info}
        <p>
          {info.entries.toLocaleString('en')} entries, stored on this device ({megabytes(info.installed)}).
        </p>
        <dl>
          {#each PAIRS as p, i}
            <dt>{LANG_NAME[p[0]]} – {LANG_NAME[p[1]]}</dt>
            <dd>{info.pairs[i].toLocaleString('en')}</dd>
          {/each}
        </dl>
        {#if engine.updateSize}
          <button class="wide" onclick={() => { about = false; install() }}>Update dictionary ({megabytes(engine.updateSize)})</button>
        {/if}
      {/if}
      <h4>Sources</h4>
      <p class="small">
        Ding by Frank Richter, TU Chemnitz (GPL). Wiktionary, English and Russian editions, via kaikki.org and WikDict
        (CC BY-SA). OpenRussian.org (CC BY-SA). Word frequencies from OpenSubtitles via FrequencyWords (CC BY-SA).
      </p>
      <button class="wide danger" onclick={wipe}>Delete dictionary data</button>
      {#if engine.info}<p class="small ver">Pack {engine.info.version}</p>{/if}
    </div>
  </Sheet>
{/if}

<style>
  svg {
    width: 22px;
    height: 22px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    padding-top: env(safe-area-inset-top);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 8px 2px 16px;
  }
  h1 {
    margin: 0;
    font-size: 30px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .icon-btn {
    flex: none;
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    border-radius: 22px;
    color: var(--accent);
  }
  .icon-btn:active {
    background: var(--fill);
  }

  h2 {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 14px 16px 6px;
    background: color-mix(in srgb, var(--bg) 88%, transparent);
    -webkit-backdrop-filter: blur(14px);
    backdrop-filter: blur(14px);
    color: var(--text-2);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .arrow {
    color: var(--text-3);
  }
  .h-action {
    margin-left: auto;
    padding: 4px 0 4px 12px;
    color: var(--accent);
    font-size: 15px;
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    column-gap: 14px;
    width: 100%;
    min-height: 44px;
    box-sizing: border-box;
    padding: 11px 16px;
    align-items: start;
    border-bottom: 1px solid var(--line);
    font-size: 16px;
    overflow-wrap: anywhere;
  }
  .row:active {
    background: var(--fill);
  }
  .l {
    font-weight: 550;
  }
  .r {
    color: var(--text);
  }
  i {
    font-style: normal;
    font-weight: 400;
    font-size: 0.82em;
    color: var(--text-3);
  }

  .recent {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 46px;
    padding: 0 16px;
    border-bottom: 1px solid var(--line);
  }
  .recent:active {
    background: var(--fill);
  }
  .recent svg {
    width: 18px;
    height: 18px;
    color: var(--text-3);
  }

  .hint {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 22vh 40px 0;
    text-align: center;
    color: var(--text-2);
    font-size: 15px;
  }
  .hint .big {
    color: var(--text);
    font-size: 22px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }
  .hint .stat {
    margin-top: 10px;
    color: var(--text-3);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .notice,
  .none {
    margin: 0;
    padding: 14px 16px 2px;
    color: var(--text-2);
    font-size: 15px;
  }
  .none {
    padding-top: 22vh;
    text-align: center;
  }
  .none .h-action {
    display: block;
    margin: 6px auto 0;
    padding: 10px;
  }

  .dock {
    flex: none;
    padding: 8px 12px calc(8px + env(safe-area-inset-bottom));
    background: var(--bg);
    border-top: 1px solid var(--line);
  }
  :global(.keyboard) .dock {
    padding-bottom: 8px;
  }
  .filters {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 2px;
    padding: 2px;
    margin-bottom: 8px;
    border-radius: 10px;
    background: var(--fill);
  }
  .filters button {
    height: 30px;
    border-radius: 8px;
    text-align: center;
    font-size: 13px;
    font-weight: 550;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .filters button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.12);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 46px;
    padding: 0 6px 0 12px;
    border-radius: 13px;
    background: var(--surface);
    box-shadow: 0 0 0 1px var(--line);
  }
  .field:focus-within {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .field > svg {
    width: 20px;
    height: 20px;
    color: var(--text-3);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: 0;
    outline: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: 17px;
    -webkit-user-select: text;
    user-select: text;
  }
  input::placeholder {
    color: var(--text-3);
  }
  .clear {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    color: var(--surface);
  }
  .clear svg {
    width: 20px;
    height: 20px;
    stroke-width: 2;
  }
  .clear .disc {
    fill: var(--text-3);
    stroke: none;
  }

  /* entry sheet */
  .side {
    padding: 10px 0 14px;
  }
  .side + .side {
    border-top: 1px solid var(--line);
  }
  .lang {
    color: var(--text-2);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .term-row {
    display: flex;
    align-items: flex-start;
    gap: 2px;
    margin-right: -10px;
  }
  .term {
    flex: 1;
    margin: 6px 8px 0 0;
    font-size: 22px;
    font-weight: 600;
    line-height: 1.25;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .chips button {
    min-height: 32px;
    padding: 0 12px;
    border-radius: 16px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 14px;
    font-weight: 500;
  }
  .chips button:active {
    opacity: 0.6;
  }
  .sheet-foot {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .star {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 40px;
    padding: 0 14px 0 10px;
    border-radius: 20px;
    background: var(--fill);
    font-size: 15px;
    font-weight: 550;
  }
  .star.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .star.on svg {
    fill: currentColor;
  }
  .src {
    flex: 1;
    text-align: right;
    color: var(--text-3);
    font-size: 12px;
  }

  /* about sheet */
  .about h3 {
    margin: 4px 0 6px;
    font-size: 24px;
    letter-spacing: -0.01em;
  }
  .about h4 {
    margin: 18px 0 4px;
    color: var(--text-2);
    font-size: 13px;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }
  .about p {
    margin: 0 0 10px;
    color: var(--text-2);
    font-size: 15px;
  }
  .about .small {
    font-size: 13px;
  }
  .about .ver {
    margin-top: 12px;
    text-align: center;
    color: var(--text-3);
  }
  dl {
    display: grid;
    grid-template-columns: 1fr auto;
    margin: 0;
    font-size: 15px;
  }
  dt,
  dd {
    margin: 0;
    padding: 9px 0;
    border-top: 1px solid var(--line);
  }
  dd {
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .wide {
    width: 100%;
    min-height: 46px;
    margin-top: 12px;
    border-radius: 12px;
    background: var(--fill);
    text-align: center;
    font-weight: 550;
    color: var(--accent);
  }
  .wide.danger {
    color: var(--danger);
  }
</style>
