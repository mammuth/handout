<script lang="ts">
  import { api, json, session } from './auth.svelte';
  import { formatDate } from '../lib/api';
  import type { ApiError } from '../lib/types';
  import type { ApiKeyJson } from './types';
  import Copy from './Copy.svelte';

  let keys = $state<ApiKeyJson[]>([]);
  let name = $state('');
  let fresh = $state<ApiKeyJson | null>(null);
  let error = $state('');
  let busy = $state(false);

  async function load() {
    keys = (await api<{ keys: ApiKeyJson[] }>('/api/keys')).keys;
  }
  $effect(() => {
    load();
  });

  async function create(ev: SubmitEvent) {
    ev.preventDefault();
    busy = true;
    error = '';
    try {
      fresh = await api<ApiKeyJson>('/api/keys', json({ name }));
      name = '';
      await load();
    } catch (e) {
      error = (e as ApiError).message;
    } finally {
      busy = false;
    }
  }
  async function revoke(id: number) {
    if (!confirm('Revoke this key? Agents using it lose access immediately.')) return;
    await api(`/api/keys/${id}`, { method: 'DELETE' });
    await load();
  }
  const mcpCmd = $derived(`claude mcp add --transport http handout ${session.owner?.mcp_url} --header "Authorization: Bearer ${fresh?.key ?? 'hk_…'}"`);
</script>

<h1>API keys</h1>
<div class="stack" style="max-width:52rem">
  <form class="card row" onsubmit={create}>
    <input class="input grow" placeholder="Key name, e.g. claude-code" bind:value={name} required />
    <button class="btn primary" disabled={busy}>Create key</button>
  </form>
  {#if fresh?.key}
    <div class="reveal stack">
      <div><strong>{fresh.name}</strong> — copy it now, it is shown once.</div>
      <div class="linkbox"><code>{fresh.key}</code><Copy text={fresh.key} /></div>
      <div class="small muted">Connect Claude Code:</div>
      <div class="linkbox"><code>{mcpCmd}</code><Copy text={mcpCmd} /></div>
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <p class="muted small">MCP endpoint: <code>{session.owner?.mcp_url}</code>. OAuth clients (claude.ai) sign in with your owner password instead of a key.</p>
  {#if keys.length}
    <table class="list">
      <thead><tr><th>Name</th><th>Key</th><th>Created</th><th>Last used</th><th></th></tr></thead>
      <tbody>
        {#each keys as k (k.id)}
          <tr>
            <td><strong>{k.name}</strong></td>
            <td class="mono muted">{k.prefix}…</td>
            <td class="muted small">{formatDate(k.created_at)}</td>
            <td class="muted small">{k.last_used_at ? formatDate(k.last_used_at) : 'never'}</td>
            <td style="text-align:right">{#if k.revoked_at}<span class="badge off">revoked</span>{:else}<button class="btn small danger" onclick={() => revoke(k.id)}>Revoke</button>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
