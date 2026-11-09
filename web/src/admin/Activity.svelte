<script lang="ts">
  import { api } from './auth.svelte';
  import type { OpenJson } from './types';
  import OpensTable from './OpensTable.svelte';

  let { navigate }: { navigate: (to: string) => void } = $props();
  let opens = $state<OpenJson[]>([]);
  $effect(() => {
    api<{ opens: OpenJson[] }>('/api/opens?limit=300').then((r) => (opens = r.opens));
  });
</script>

<h1>Activity</h1>
{#if opens.length}
  <OpensTable {opens} {navigate} />
{:else}
  <p class="empty">No opens yet.</p>
{/if}
