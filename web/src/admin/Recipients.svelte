<script lang="ts">
  import { api, json, patch } from './auth.svelte';
  import { formatDate, formatDuration, kindLabel } from '../lib/api';
  import type { ApiError } from '../lib/types';
  import type { RecipientDetail, RecipientJson } from './types';
  import Copy from './Copy.svelte';

  let { selected, navigate }: { selected: string | null; navigate: (to: string) => void } = $props();

  let list = $state<RecipientJson[]>([]);
  let q = $state('');
  let detail = $state<RecipientDetail | null>(null);
  let name = $state('');
  let email = $state('');
  let company = $state('');
  let cName = $state('');
  let cEmail = $state('');
  let cCompany = $state('');
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');

  async function load() {
    const r = await api<{ recipients: RecipientJson[] }>(`/api/recipients?limit=500${q ? `&q=${encodeURIComponent(q)}` : ''}`);
    list = r.recipients;
  }
  async function loadDetail() {
    if (!selected) {
      detail = null;
      return;
    }
    try {
      detail = await api<RecipientDetail>(`/api/recipients/${selected}`);
      name = detail.name;
      email = detail.email ?? '';
      company = detail.company ?? '';
    } catch (e) {
      error = (e as ApiError).message;
    }
  }
  $effect(() => {
    q;
    load();
  });
  $effect(() => {
    selected;
    loadDetail();
  });

  async function run(fn: () => Promise<void>, done?: string) {
    busy = true;
    error = '';
    try {
      await fn();
      if (done) {
        notice = done;
        setTimeout(() => (notice = ''), 1800);
      }
    } catch (e) {
      error = (e as ApiError).message;
    } finally {
      busy = false;
    }
  }

  const create = () =>
    run(async () => {
      const r = await api<RecipientJson>('/api/recipients', json({ name: cName || undefined, email: cEmail || undefined, company: cCompany || undefined }));
      cName = cEmail = cCompany = '';
      await load();
      navigate(`/admin/recipients/${r.id}`);
    });
  const save = () =>
    run(async () => {
      await api(`/api/recipients/${selected}`, patch({ name, email: email || null, company: company || null }));
      await Promise.all([load(), loadDetail()]);
    }, 'Saved');
  const rotate = () => {
    if (!confirm('Rotate the token? Every recipient link sent so far stops auto-signing them in.')) return;
    run(async () => {
      await api(`/api/recipients/${selected}/rotate-token`, { method: 'POST' });
      await loadDetail();
    }, 'Token rotated');
  };
  const remove = () => {
    if (!confirm('Delete this recipient and all their invites?')) return;
    run(async () => {
      await api(`/api/recipients/${selected}`, { method: 'DELETE' });
      await load();
      navigate('/admin/recipients');
    });
  };
</script>

<h1>Recipients</h1>
<div class="split">
  <div class="stack">
    <form class="card" onsubmit={(e) => { e.preventDefault(); create(); }}>
      <div class="row">
        <input class="input grow" placeholder="Name" bind:value={cName} />
        <input class="input grow" placeholder="Email" bind:value={cEmail} />
        <input class="input grow" placeholder="Company" bind:value={cCompany} />
        <button class="btn primary" disabled={busy || (!cName && !cEmail)}>Add</button>
      </div>
    </form>
    <div class="row">
      <input class="input grow" placeholder="Search name, email, company…" bind:value={q} />
      <span class="muted small">{list.length}</span>
    </div>
    {#if list.length}
      <table class="list">
        <thead><tr><th>Name</th><th>Email</th><th>Company</th><th>Last seen</th></tr></thead>
        <tbody>
          {#each list as r (r.id)}
            <tr class="click" class:selected={r.id === selected} onclick={() => navigate(`/admin/recipients/${r.id}`)}>
              <td><strong>{r.name}</strong></td>
              <td class="muted">{r.email ?? ''}</td>
              <td class="muted">{r.company ?? ''}</td>
              <td class="muted small">{r.last_seen_at ? formatDate(r.last_seen_at) : 'never'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="empty">No recipients yet. Sharing a file creates one.</p>
    {/if}
  </div>

  {#if selected && detail}
    <div class="card stack">
      <h2 style="margin:0">{detail.name}</h2>
      <form onsubmit={(e) => { e.preventDefault(); save(); }}>
        <label class="field">Name<input class="input" bind:value={name} /></label>
        <label class="field">Email<input class="input" bind:value={email} /></label>
        <label class="field">Company<input class="input" bind:value={company} /></label>
        <div class="row" style="margin-top:.8rem"><button class="btn primary" disabled={busy}>Save</button>{#if notice}<span class="ok">{notice}</span>{/if}</div>
      </form>

      <h3>Files shared with them</h3>
      {#if detail.files.length}
        <table class="list">
          <tbody>
            {#each detail.files as f (f.id)}
              <tr>
                <td>
                  <a href="/admin/files/{f.id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/files/${f.id}`); }}><strong>{f.title}</strong></a>
                  <div class="muted small">{kindLabel[f.kind]} · {f.last_opened_at ? `opened ${formatDate(f.last_opened_at)}` : 'not opened yet'}{#if f.active_seconds} · {formatDuration(f.active_seconds)} active{/if}{#if f.max_scroll_pct} · read {f.max_scroll_pct}%{/if}</div>
                </td>
                <td style="text-align:right"><Copy text={f.recipient_link} label="Copy link" /></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="muted small">Nothing shared yet.</p>
      {/if}

      <h3>Home link</h3>
      <div class="linkbox"><code>{location.origin}/s/{detail.files[0] ? detail.files[0].file_link.split('/s/')[1].split('/')[0] : ''}?r={detail.token}</code><Copy text={`${location.origin}/s/${detail.files[0] ? detail.files[0].file_link.split('/s/')[1].split('/')[0] : ''}?r=${detail.token}`} /></div>
      <p class="muted small">Signs them in and lists everything shared with them.</p>

      <hr class="sep" />
      <div class="row">
        <button class="btn" onclick={rotate} disabled={busy}>Rotate token</button>
        <button class="btn danger" onclick={remove} disabled={busy}>Delete recipient</button>
      </div>
      {#if error}<p class="error">{error}</p>{/if}
    </div>
  {:else}
    <div class="card muted">Select a recipient to edit them and copy their links.</div>
  {/if}
</div>
