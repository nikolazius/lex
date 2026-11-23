<script lang="ts">
  import { engine } from './lib/engine.svelte'
  import Search from './Search.svelte'
  import Setup from './Setup.svelte'
</script>

{#if engine.phase === 'ready'}
  <Search />
{:else if engine.phase !== 'boot'}
  <Setup />
{/if}
