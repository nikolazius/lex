<script lang="ts">
  import { engine, install } from './lib/engine.svelte'
  import { megabytes } from './lib/text'

  const ios = /iPad|iPhone|iPod/.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)
  const standalone = (navigator as any).standalone === true || matchMedia('(display-mode: standalone)').matches

  // iOS gives a Home Screen app its own storage, so the download only makes sense once installed.
  let guide = $state(ios && !standalone)
  const size = $derived(engine.size ? megabytes(engine.size) : 'about 40 MB')
  const percent = $derived(engine.total ? Math.min(100, Math.floor((engine.got / engine.total) * 100)) : 0)

  $effect(() => {
    if (standalone && engine.phase === 'missing' && !engine.error) install()
  })
</script>

<main>
  <img class="icon" src="./icons/icon-192.png" alt="" width="84" height="84" />
  <h1>Lex</h1>
  <p class="tag">Offline dictionary<br />Deutsch · English · Русский</p>

  {#if guide}
    <ol>
      <li>
        <span class="n">1</span>
        <span>Tap <svg viewBox="0 0 24 24" aria-label="Share"><path d="M12 3v12M8 7l4-4 4 4M6 11H5a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-8a1 1 0 0 0-1-1h-1" /></svg> in Safari</span>
      </li>
      <li><span class="n">2</span><span>Choose <b>Add to Home Screen</b></span></li>
      <li><span class="n">3</span><span>Open <b>Lex</b> from the Home Screen</span></li>
    </ol>
    <p class="note">The dictionary ({size}) downloads once on first launch. After that everything works offline.</p>
    <button class="link" onclick={() => (guide = false)}>Use in this browser instead</button>
  {:else if engine.phase === 'downloading' && !engine.error}
    <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent}>
      <div class="bar" style:transform="scaleX({percent / 100})"></div>
    </div>
    <p class="status">
      <b>{percent}%</b>
      {#if engine.total}· {megabytes(engine.got)} of {megabytes(engine.total)}{/if}
    </p>
    <p class="note">Downloading the dictionary. Keep the app open — if it is interrupted, it resumes where it stopped.</p>
  {:else}
    {#if engine.error}
      <p class="error">Download failed. Check the connection and try again.</p>
    {:else}
      <p class="note">One download ({size}), then everything works offline.</p>
    {/if}
    <button class="primary" onclick={install}>{engine.error ? 'Try again' : 'Download dictionary'}</button>
  {/if}
</main>

<style>
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 32px 32px calc(32px + env(safe-area-inset-bottom));
    text-align: center;
  }
  .icon {
    border-radius: 19px;
    box-shadow: 0 6px 24px rgba(61, 79, 224, 0.28);
  }
  h1 {
    margin: 20px 0 4px;
    font-size: 32px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .tag {
    margin: 0 0 32px;
    color: var(--text-2);
  }
  ol {
    list-style: none;
    margin: 0 0 20px;
    padding: 4px 16px;
    width: 100%;
    max-width: 340px;
    box-sizing: border-box;
    background: var(--surface);
    border-radius: 16px;
    text-align: left;
  }
  li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 0;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  .n {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 13px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 14px;
    font-weight: 600;
    display: grid;
    place-items: center;
  }
  li svg {
    width: 20px;
    height: 20px;
    vertical-align: -4px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .note {
    max-width: 320px;
    margin: 0 0 20px;
    color: var(--text-2);
    font-size: 15px;
  }
  .error {
    max-width: 320px;
    margin: 0 0 20px;
    color: var(--danger);
    font-size: 15px;
  }
  .status {
    margin: 14px 0 20px;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .status b {
    color: var(--text);
  }
  .progress {
    width: 100%;
    max-width: 300px;
    height: 6px;
    border-radius: 3px;
    background: var(--fill);
    overflow: hidden;
  }
  .bar {
    height: 100%;
    background: var(--accent);
    transform-origin: left;
    transition: transform 0.2s linear;
  }
  .primary {
    min-height: 50px;
    padding: 0 28px;
    border-radius: 14px;
    background: var(--accent);
    color: var(--on-accent);
    font-weight: 600;
  }
  .primary:active {
    opacity: 0.8;
  }
  .link {
    min-height: 44px;
    color: var(--accent);
    font-size: 15px;
  }
</style>
