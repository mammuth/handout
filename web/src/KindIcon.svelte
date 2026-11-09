<script lang="ts">
  /** Colored file-kind tile used in the sidebar and the admin files table. */
  let { kind, size = 28 }: { kind: string; size?: number } = $props();

  const tiles: Record<string, [string, string]> = {
    html: ['HTML', '#d97706'],
    markdown: ['MD', '#8b8d98'],
    pdf: ['PDF', '#e5484d'],
    image: ['IMG', '#0ea5e9'],
    other: ['FILE', '#64748b'],
  };
  const [label, color] = $derived(tiles[kind] ?? tiles.other);
</script>

<span class="tile" style="--c: {color}; --s: {size}px" aria-hidden="true">{label}</span>

<style>
  .tile {
    width: var(--s);
    height: var(--s);
    flex: 0 0 auto;
    display: grid;
    place-content: center;
    border-radius: 6px;
    font-size: calc(var(--s) * 0.3);
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--c);
    background: color-mix(in oklab, var(--c) 18%, transparent);
  }
</style>
