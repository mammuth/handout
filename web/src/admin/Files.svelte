<script lang="ts">
  import { api } from './auth.svelte';
  import { formatDate, formatSize, kindLabel } from '../lib/api';
  import type { ApiError } from '../lib/types';
  import type { FileSummary } from './types';
  import FileDetail from './FileDetail.svelte';

  let { selected, navigate }: { selected: string | null; navigate: (to: string) => void } = $props();

  let files = $state<FileSummary[]>([]);
  let q = $state('');
  let title = $state('');
  let picked = $state<File | null>(null);
  let over = $state(false);
  let busy = $state(false);
  let error = $state('');
  let input = $state<HTMLInputElement | null>(null);

  async function load() {
    const r = await api<{ files: FileSummary[] }>(`/api/files?limit=500${q ? `&q=${encodeURIComponent(q)}` : ''}`);
    files = r.files;
  }
  $effect(() => {
    q;
    load();
  });

  function pick(list: FileList | null) {
    picked = list?.[0] ?? null;
    if (picked && !title) title = picked.name.replace(/\.[^.]+$/, '');
  }

  async function upload(ev: SubmitEvent) {
    ev.preventDefault();
    if (!picked) return;
    busy = true;
    error = '';
    try {
      const fd = new FormData();
      fd.append('title', title);
      fd.append('file', picked, picked.name);
      const f = await api<FileSummary>('/api/files', { method: 'POST', body: fd });
      picked = null;
      title = '';
      if (input) input.value = '';
      await load();
      navigate(`/admin/files/${f.id}`);
    } catch (e) {
      error = (e as ApiError).message;
    } finally {
      busy = false;
    }
  }
</script>

<h1>Files</h1>
<div class="split">
  <div class="stack">
    <form class="card" onsubmit={upload}>
      <div
        class="drop"
        class:over
        role="button"
        tabindex="0"
        ondragover={(e) => { e.preventDefault(); over = true; }}
        ondragleave={() => (over = false)}
        ondrop={(e) => { e.preventDefault(); over = false; pick(e.dataTransfer?.files ?? null); }}
        onclick={() => input?.click()}
        onkeydown={(e) => e.key === 'Enter' && input?.click()}
      >
        {#if picked}<strong>{picked.name}</strong> · {formatSize(picked.size)}{:else}Drop an HTML, Markdown, PDF or image here, or click to choose{/if}
        <input bind:this={input} type="file" hidden onchange={(e) => pick((e.target as HTMLInputElement).files)} />
      </div>
      <div class="row" style="margin-top:.7rem">
        <input class="input grow" placeholder="Title (defaults to the filename)" bind:value={title} />
        <button class="btn primary" disabled={!picked || busy}>{busy ? 'Uploading…' : 'Upload'}</button>
      </div>
      {#if error}<p class="error">{error}</p>{/if}
    </form>

    <div class="row">
      <input class="input grow" placeholder="Search title or id…" bind:value={q} />
      <span class="muted small">{files.length} {files.length === 1 ? 'file' : 'files'}</span>
    </div>

    {#if files.length}
      <table class="list">
        <thead><tr><th>Title</th><th>Kind</th><th>Access</th><th>Updated</th></tr></thead>
        <tbody>
          {#each files as f (f.id)}
            <tr class="click" class:selected={f.id === selected} onclick={() => navigate(`/admin/files/${f.id}`)}>
              <td><strong>{f.title}</strong><div class="muted small mono">{f.id}{f.version > 1 ? ` · v${f.version}` : ''}</div></td>
              <td>{kindLabel[f.kind]}</td>
              <td>
                {#if f.access === 'off'}<span class="badge off">off</span>
                {:else if f.access === 'recipients'}<span class="badge ember">recipients</span>
                {:else}<span class="badge">link</span>{/if}
                {#if f.has_password}<span class="badge">password</span>{/if}
                {#if f.email_gate}<span class="badge">email</span>{/if}
                {#if f.expires_at}<span class="badge">expires</span>{/if}
              </td>
              <td class="muted small">{formatDate(f.updated_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="empty">No files yet. Upload one above, or let an agent do it over MCP.</p>
    {/if}
  </div>

  {#if selected}
    <FileDetail id={selected} onChanged={load} onDeleted={() => { load(); navigate('/admin'); }} {navigate} />
  {:else}
    <div class="card muted">Select a file to see its links, invites and settings.</div>
  {/if}
</div>
