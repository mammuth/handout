<script lang="ts">
  /**
   * Owner attribution. `stacked` puts "from" above the name, sized like the viewer's kind and title
   * lines, so the two columns line up in the top bar. Default is one line, for the sidebar header.
   */
  let { ownerName, stacked = false }: { ownerName: string; stacked?: boolean } = $props();
</script>

{#if stacked}
  <div class="stacked">
    <span class="eyebrow">from</span>
    <span class="name"><span class="dot"></span><strong>{ownerName}</strong></span>
  </div>
{:else}
  <div class="brand">
    <span class="dot"></span>
    <span class="from">from</span>
    <strong>{ownerName}</strong>
  </div>
{/if}

<style>
  .brand { display: flex; align-items: baseline; gap: 0.45rem; font-size: 1.05rem; letter-spacing: -0.01em; white-space: nowrap; }
  strong { font-weight: 700; font-variation-settings: 'wdth' 90; }
  .from { color: var(--muted); font-size: 0.85rem; }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--ember); box-shadow: 0 0 0 4px var(--ember-soft); align-self: center; flex: 0 0 auto; }
  .stacked { display: flex; flex-direction: column; white-space: nowrap; padding-right: 0.9rem; border-right: 1px solid var(--line); }
  .eyebrow { font-size: 0.68rem; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); }
  .name { display: flex; align-items: center; gap: 0.55rem; font-size: 1.15rem; letter-spacing: -0.015em; }
</style>
