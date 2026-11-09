<script lang="ts">
  import { api } from './auth.svelte';
  import { formatDuration } from '../lib/api';
  import type { OpenJson } from './types';

  let { navigate }: { navigate: (to: string) => void } = $props();
  let opens = $state<OpenJson[]>([]);
  $effect(() => {
    api<{ opens: OpenJson[] }>('/api/opens?limit=300').then((r) => (opens = r.opens));
  });

  function when(iso: string): string {
    const d = new Date(iso);
    const diff = (Date.now() - d.getTime()) / 1000;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} h ago`;
    return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }
  function ua(s: string): string {
    const m = s.match(/(Firefox|Edg|Chrome|Safari|curl|reqwest)\/[\d.]+/);
    return m ? m[0].split('/')[0] : s.slice(0, 24);
  }
</script>

<h1>Activity</h1>
{#if opens.length}
  <table class="list">
    <thead><tr><th>When</th><th>File</th><th>Who</th><th>Active</th><th>Read</th><th>Client</th></tr></thead>
    <tbody>
      {#each opens as o}
        <tr>
          <td class="muted small" style="white-space:nowrap">{when(o.at)}</td>
          <td><a href="/admin/files/{o.file_id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/files/${o.file_id}`); }}>{o.title}</a> <span class="muted small">v{o.version}</span></td>
          <td>{#if o.recipient}<a href="/admin/recipients/{o.recipient.id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/recipients/${o.recipient!.id}`); }}>{o.recipient.name}</a>{:else}<span class="muted">anonymous</span>{/if}</td>
          <td class="small">{o.active_seconds ? formatDuration(o.active_seconds) : '—'}{#if o.visible_seconds > o.active_seconds}<span class="muted"> / {formatDuration(o.visible_seconds)}</span>{/if}</td>
          <td class="small">{o.max_scroll_pct ? `${o.max_scroll_pct}%` : '—'}</td>
          <td class="muted small">{ua(o.user_agent)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="empty">No opens yet.</p>
{/if}
