<script lang="ts">
  import Sidebar from './Sidebar.svelte';
  import Gate from './Gate.svelte';
  import { api, formatSize, kindLabel } from './lib/api';
  import { startTracking } from './lib/track';
  import type { ApiError, ViewResponse } from './lib/types';

  let { owner, file, navigate }: { owner: string; file: string; navigate: (to: string) => void } = $props();

  let data = $state<ViewResponse | null>(null);
  let error = $state<ApiError | null>(null);
  let loading = $state(true);
  let menuOpen = $state(false);
  let frameLoaded = $state(false);
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let stopTracking: (() => void) | null = null;

  async function load() {
    loading = true;
    error = null;
    frameLoaded = false;
    try {
      data = await api.view(owner, file);
      document.title = `${data.file.title} · ${data.owner.name}`;
      stopTracking?.();
      stopTracking = data.open ? startTracking(owner, file, data.open, () => frameEl) : null;
    } catch (e) {
      error = e as ApiError;
      data = null;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    owner;
    file;
    load();
  });
  $effect(() => () => stopTracking?.());

  const sandbox = 'allow-scripts allow-forms allow-popups allow-downloads allow-modals';
</script>

<!-- The sidebar lists what was shared with a recipient; anonymous viewers have no such list, so they
     get the full width and the owner attribution moves into the top bar. -->
<div class="layout">
  {#if data?.recipient}
    <Sidebar ownerName={data.owner.name} recipient={data.recipient} entries={data.sidebar} currentId={data.file.id} {navigate} bind:open={menuOpen} />
  {/if}
  {#if menuOpen}<button class="scrim" aria-label="Close menu" onclick={() => (menuOpen = false)}></button>{/if}

  <section class="main">
    <div class="topbar glass">
      {#if data?.recipient}
        <button class="menu" aria-label="Open menu" onclick={() => (menuOpen = !menuOpen)}>
          <span></span><span></span><span></span>
        </button>
      {/if}
      {#if data?.is_owner}
        <a class="btn" href="/admin" title="You own this file. Your visits are not counted.">← Admin</a>
      {/if}
      {#if data}
        <!-- breadcrumb carries the owner attribution, with or without the sidebar -->
        <h1 class="crumb"><span class="owner">{data.owner.name} /</span> {data.file.title}</h1>
        <span class="chip">{kindLabel[data.file.kind]}{data.file.version > 1 ? ` · v${data.file.version}` : ''}</span>
        <span class="spacer"></span>
        {#if data.download_url}
          {#if data.content_url && (data.file.kind === 'html' || data.file.kind === 'markdown')}
            <!-- the viewer's own browser renders the PDF: new tab + print dialog, "Save as PDF" there -->
            <button
              class="btn"
              title="Opens the page in a new tab with the print dialog; pick “Save as PDF” there."
              onclick={() => window.open(`${data?.content_url}#print`, '_blank', 'noopener')}
            >
              Save as PDF
            </button>
          {/if}
          <a class="btn primary" href={data.download_url} download={data.file.filename}>
            Download · {formatSize(data.file.size)}
          </a>
        {/if}
      {/if}
    </div>

    <div class="stage">
      {#if loading && !data}
        <div class="center muted">Loading…</div>
      {:else if error}
        <div class="center">
          <h2>{error.status === 404 ? 'This link goes nowhere.' : 'Something went wrong.'}</h2>
          <p class="muted">{error.status === 404 ? 'The file may have been removed, or the address is incomplete.' : error.message}</p>
        </div>
      {:else if data && data.state !== 'ok'}
        <Gate state={data.state} {owner} {file} onUnlocked={load} />
      {:else if data && data.content_url}
        {#if data.file.kind === 'image'}
          <div class="image"><img src={data.content_url} alt={data.file.title} /></div>
        {:else if data.file.kind === 'pdf'}
          <iframe bind:this={frameEl} title={data.file.title} src={data.content_url} class="frame" referrerpolicy="no-referrer"></iframe>
        {:else if data.file.kind === 'other'}
          <div class="center">
            <h2>{data.file.filename}</h2>
            <p class="muted">{data.file.mime} · {formatSize(data.file.size)}</p>
            <a class="button" href={data.download_url} download={data.file.filename}>Download</a>
          </div>
        {:else}
          {#if !frameLoaded}<div class="center muted veil">Loading…</div>{/if}
          <iframe
            bind:this={frameEl}
            title={data.file.title}
            src={data.content_url}
            class="frame"
            class:ready={frameLoaded}
            {sandbox}
            referrerpolicy="no-referrer"
            onload={() => (frameLoaded = true)}
          ></iframe>
        {/if}
      {/if}
    </div>
  </section>
</div>

<style>
  .layout { display: flex; height: 100%; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; height: 100%; padding: var(--gap); gap: var(--gap); }
  .topbar { display: flex; align-items: center; gap: 0.6rem; padding: 0.4rem 0.45rem 0.4rem 0.9rem; }
  .crumb { min-width: 0; margin: 0; font-size: 1.05rem; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .owner { color: var(--muted); font-weight: 400; }
  .chip { flex: 0 0 auto; font-size: 0.72rem; color: var(--ink-2); background: var(--ember-soft); border-radius: 999px; padding: 0.1rem 0.6rem; white-space: nowrap; }
  .spacer { flex: 1; }
  .btn { font: inherit; font-size: 0.8rem; font-weight: 600; line-height: 1.3; color: var(--ink); text-decoration: none; border: 1px solid rgba(255, 255, 255, 0.25); border-radius: 999px; padding: 0.35rem 0.85rem; background: rgba(255, 255, 255, 0.1); white-space: nowrap; cursor: pointer; transition: background 0.15s; }
  .btn:hover { background: rgba(255, 255, 255, 0.18); }
  .btn.primary { background: #fff; border-color: #fff; color: var(--ember-fg); }
  .btn.primary:hover { background: #e0e7ff; }
  .menu { display: none; width: 32px; height: 32px; border-radius: 999px; border: 1px solid var(--line); background: var(--paper-2); flex-direction: column; justify-content: center; gap: 4px; align-items: center; cursor: pointer; }
  .menu span { width: 14px; height: 2px; background: var(--ink); border-radius: 2px; }
  .scrim { display: none; position: fixed; inset: 0; z-index: 10; background: rgba(3, 5, 20, 0.5); border: 0; }
  .stage { flex: 1; position: relative; min-height: 0; }
  .frame { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; border-radius: var(--radius-lg); box-shadow: 0 30px 60px -20px rgba(0, 0, 0, 0.6); background: #fff; opacity: 0; transition: opacity 0.3s ease; }
  .frame.ready { opacity: 1; }
  .image { height: 100%; display: grid; place-items: center; padding: 1rem; overflow: auto; }
  .image img { max-width: 100%; max-height: 100%; box-shadow: var(--shadow); border-radius: var(--radius); }
  .center { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; padding: 2rem; }
  .center h2 { margin: 0 0 0.4rem; letter-spacing: -0.02em; }
  .muted { color: var(--muted); }
  .veil { z-index: 1; }
  .button { display: inline-block; margin-top: 1rem; background: #fff; color: var(--ember-fg); text-decoration: none; padding: 0.5rem 1.1rem; border-radius: 999px; font-weight: 600; }
  @media (max-width: 800px) {
    .menu { display: flex; }
    .scrim { display: block; }
    .main { padding: 6px; gap: 6px; }
    .topbar { padding: 0.35rem 0.4rem; }
    .chip { display: none; }
  }
</style>
