<script lang="ts">
  import { api } from './auth.svelte';
  import { formatDate, formatSize } from '../lib/api';
  import KindIcon from '../KindIcon.svelte';
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

<div class="hdr">
  <h1>Files <span class="muted small">{files.length} {files.length === 1 ? 'file' : 'files'}</span></h1>
  <input class="input" placeholder="Search title or id…" bind:value={q} />
  <button class="btn primary" onclick={() => input?.click()}>+ Upload</button>
  <input bind:this={input} type="file" hidden onchange={(e) => pick((e.target as HTMLInputElement).files)} />
</div>
<div class="split">
  <!-- the whole list is a drop target; "+ Upload" opens the picker -->
  <div
    class="stack drop-zone"
    class:over
    role="region"
    aria-label="Files"
    ondragover={(e) => { e.preventDefault(); over = true; }}
    ondragleave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) over = false; }}
    ondrop={(e) => { e.preventDefault(); over = false; pick(e.dataTransfer?.files ?? null); }}
  >
    {#if picked}
      <form class="card row" onsubmit={upload}>
        <span class="small"><strong>{picked.name}</strong> · {formatSize(picked.size)}</span>
        <input class="input grow" placeholder="Title (defaults to the filename)" bind:value={title} />
        <button class="btn primary" disabled={busy}>{busy ? 'Uploading…' : 'Upload'}</button>
        <button type="button" class="btn" onclick={() => { picked = null; title = ''; if (input) input.value = ''; }}>Cancel</button>
      </form>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}

    {#if files.length}
      <div class="tbl"><table class="list">
        <thead><tr><th>Name</th><th>Access</th><th>Updated</th><th></th></tr></thead>
        <tbody>
          {#each files as f (f.id)}
            <tr class="click" class:selected={f.id === selected} onclick={() => navigate(`/admin/files/${f.id}`)}>
              <td><div class="fc"><KindIcon kind={f.kind} /><div><strong>{f.title}</strong><div class="muted small mono">{f.id}{f.version > 1 ? ` · v${f.version}` : ''}</div></div></div></td>
              <td>
                <span class="status {f.access}">{f.access === 'off' ? 'Off' : f.access === 'recipients' ? 'Recipients' : 'Link'}</span>
                {#if f.has_password}<span class="badge">password</span>{/if}
                {#if f.email_gate}<span class="badge">email</span>{/if}
                {#if f.expires_at}<span class="badge">expires</span>{/if}
              </td>
              <td class="muted small">{formatDate(f.updated_at)}</td>
              <td><a class="btn small" href={f.file_link} target="_blank" rel="noopener" title="View as a recipient would (your visit is not counted)" onclick={(e) => e.stopPropagation()}>Open ↗</a></td>
            </tr>
          {/each}
        </tbody>
      </table></div>
    {:else}
      <p class="empty">No files yet. Drop one here, use “+ Upload”, or let an agent do it over MCP.</p>
    {/if}
  </div>

  {#if selected}
    <FileDetail id={selected} onChanged={load} onDeleted={() => { load(); navigate('/admin'); }} {navigate} />
  {:else}
    <div class="card muted">Select a file to see its links, invites and settings.</div>
  {/if}
</div>
