<script lang="ts">
  import { formatDuration } from '../lib/api';
  import type { OpenJson } from './types';

  let {
    opens,
    navigate,
    who = true,
    client = true,
  }: { opens: OpenJson[]; navigate: (to: string) => void; who?: boolean; client?: boolean } = $props();

  function when(iso: string): string {
    const d = new Date(iso);
    const diff = (Date.now() - d.getTime()) / 1000;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} h ago`;
    return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
  }
  function full(iso: string): string {
    return new Date(iso).toLocaleString(undefined, { dateStyle: 'full', timeStyle: 'short' });
  }
  function ua(s: string): string {
    const m = s.match(/(Firefox|Edg|Chrome|Safari|curl|reqwest)\/[\d.]+/);
    return m ? m[0].split('/')[0] : s.slice(0, 24);
  }
</script>

<table class="list compact">
  <thead><tr><th>When</th><th>File</th>{#if who}<th>Who</th>{/if}<th>Active</th><th>Read</th>{#if client}<th>Client</th>{/if}</tr></thead>
  <tbody>
    {#each opens as o}
      <tr>
        <td class="muted"><time datetime={o.at} title={full(o.at)}>{when(o.at)}</time></td>
        <td class="title"><a href="/admin/files/{o.file_id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/files/${o.file_id}`); }}>{o.title}</a> <span class="muted">v{o.version}</span></td>
        {#if who}<td>{#if o.recipient}<a href="/admin/recipients/{o.recipient.id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/recipients/${o.recipient!.id}`); }}>{o.recipient.name}</a>{:else}<span class="muted">anonymous</span>{/if}</td>{/if}
        <td>{o.active_seconds ? formatDuration(o.active_seconds) : '—'}{#if o.visible_seconds > o.active_seconds}<span class="muted"> / {formatDuration(o.visible_seconds)}</span>{/if}</td>
        <td>{o.max_scroll_pct ? `${o.max_scroll_pct}%` : '—'}</td>
        {#if client}<td class="muted">{ua(o.user_agent)}</td>{/if}
      </tr>
    {/each}
  </tbody>
</table>
