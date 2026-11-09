<script lang="ts">
  let { ownerName }: { ownerName: string } = $props();
</script>

<div class="brand">
  <span class="dot"></span>
  <span class="from">from</span>
  <strong>{ownerName}</strong>
</div>

<style>
  .brand { display: flex; align-items: baseline; gap: 0.45rem; font-size: 1.05rem; letter-spacing: -0.01em; white-space: nowrap; }
  strong { font-weight: 700; font-variation-settings: 'wdth' 90; }
  .from { color: var(--muted); font-size: 0.85rem; }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--ember); box-shadow: 0 0 0 4px var(--ember-soft); align-self: center; }
</style>
