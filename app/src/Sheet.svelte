<script lang="ts">
  import type { Snippet } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'

  let { onclose, children }: { onclose: () => void; children: Snippet } = $props()

  let drag = $state(0)
  let startY = 0
  let dragging = $state(false)

  function down(e: PointerEvent) {
    dragging = true
    startY = e.clientY
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }
  function move(e: PointerEvent) {
    if (dragging) drag = Math.max(0, e.clientY - startY)
  }
  function up() {
    if (!dragging) return
    dragging = false
    if (drag > 90) onclose()
    else drag = 0
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<button class="backdrop" aria-label="Close" onclick={onclose} transition:fade={{ duration: 180 }}></button>
<div
  class="sheet"
  class:dragging
  role="dialog"
  aria-modal="true"
  style:transform="translateY({drag}px)"
  transition:fly={{ y: 480, duration: 280, easing: cubicOut, opacity: 1 }}
>
  <div class="grab" role="presentation" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
    <span></span>
  </div>
  <div class="body">
    {@render children()}
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 10;
    background: rgba(0, 0, 0, 0.32);
  }
  .sheet {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 11;
    max-height: 88%;
    display: flex;
    flex-direction: column;
    background: var(--raised);
    border-radius: 20px 20px 0 0;
    box-shadow: var(--shadow);
    transition: transform 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
  }
  .sheet.dragging {
    transition: none;
  }
  .grab {
    flex: none;
    height: 26px;
    display: grid;
    place-items: center;
    touch-action: none;
    cursor: grab;
  }
  .grab span {
    width: 38px;
    height: 5px;
    border-radius: 3px;
    background: var(--text-3);
    opacity: 0.6;
  }
  .body {
    overflow-y: auto;
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
    padding: 2px 20px calc(20px + env(safe-area-inset-bottom));
  }
</style>
