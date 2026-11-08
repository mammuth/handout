<script lang="ts">
  import Sidebar from './Sidebar.svelte';
  import Gate from './Gate.svelte';
  import { api, formatSize, kindLabel } from './lib/api';
  import type { ApiError, ViewResponse } from './lib/types';

  let { owner, file, navigate }: { owner: string; file: string; navigate: (to: string) => void } = $props();

  let data = $state<ViewResponse | null>(null);
  let error = $state<ApiError | null>(null);
  let loading = $state(true);
  let menuOpen = $state(false);
  let frameLoaded = $state(false);

  async function load() {
    loading = true;
    error = null;
    frameLoaded = false;
    try {
      data = await api.view(owner, file);
      document.title = `${data.file.title} · ${data.owner.name}`;
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

  const sandbox = 'allow-scripts allow-forms allow-popups allow-downloads allow-modals';
</script>

<div class="layout">
  {#if data}
    <Sidebar ownerName={data.owner.name} recipient={data.recipient} entries={data.sidebar} currentId={data.file.id} {navigate} bind:open={menuOpen} />
  {:else}
    <Sidebar ownerName="…" recipient={null} entries={[]} {navigate} bind:open={menuOpen} />
  {/if}
  {#if menuOpen}<button class="scrim" aria-label="Close menu" onclick={() => (menuOpen = false)}></button>{/if}

  <section class="main">
    <div class="topbar">
      <button class="menu" aria-label="Open menu" onclick={() => (menuOpen = !menuOpen)}>
        <span></span><span></span><span></span>
      </button>
      {#if data}
        <div class="titles">
          <span class="kind">{kindLabel[data.file.kind]}{data.file.version > 1 ? ` · v${data.file.version}` : ''}</span>
          <h1>{data.file.title}</h1>
        </div>
        {#if data.download_url}
          <a class="download" href={data.download_url} download={data.file.filename}>
            Download <span class="size">{formatSize(data.file.size)}</span>
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
          <iframe title={data.file.title} src={data.content_url} class="frame" referrerpolicy="no-referrer"></iframe>
        {:else if data.file.kind === 'other'}
          <div class="center">
            <h2>{data.file.filename}</h2>
            <p class="muted">{data.file.mime} · {formatSize(data.file.size)}</p>
            <a class="button" href={data.download_url} download={data.file.filename}>Download</a>
          </div>
        {:else}
          {#if !frameLoaded}<div class="center muted veil">Loading…</div>{/if}
          <iframe
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
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; height: 100%; }
  .topbar { display: flex; align-items: center; gap: 0.9rem; padding: 0.8rem 1.2rem; border-bottom: 1px solid var(--line); background: color-mix(in oklab, var(--paper) 70%, transparent); }
  .titles { min-width: 0; flex: 1; display: flex; flex-direction: column; }
  .kind { font-size: 0.68rem; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); }
  h1 { font-size: 1.15rem; margin: 0; letter-spacing: -0.015em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .download { font-size: 0.85rem; font-weight: 600; color: var(--ink); text-decoration: none; border: 1px solid var(--line); border-radius: 999px; padding: 0.4rem 0.85rem; background: var(--card); white-space: nowrap; }
  .download:hover { border-color: var(--ember); color: var(--ember); }
  .size { color: var(--muted); font-weight: 400; margin-left: 0.3rem; }
  .menu { display: none; width: 38px; height: 38px; border-radius: 10px; border: 1px solid var(--line); background: var(--card); flex-direction: column; justify-content: center; gap: 4px; align-items: center; cursor: pointer; }
  .menu span { width: 16px; height: 2px; background: var(--ink); border-radius: 2px; }
  .scrim { display: none; position: fixed; inset: 0; z-index: 10; background: rgba(0, 0, 0, 0.35); border: 0; }
  .stage { flex: 1; position: relative; min-height: 0; background: var(--card); }
  .frame { position: absolute; inset: 0; width: 100%; height: 100%; border: 0; background: #fff; opacity: 0; transition: opacity 0.3s ease; }
  .frame.ready { opacity: 1; }
  .image { height: 100%; display: grid; place-items: center; padding: 1.5rem; overflow: auto; }
  .image img { max-width: 100%; max-height: 100%; box-shadow: var(--shadow); border-radius: 6px; }
  .center { position: absolute; inset: 0; display: grid; place-content: center; text-align: center; padding: 2rem; }
  .center h2 { margin: 0 0 0.4rem; letter-spacing: -0.02em; }
  .muted { color: var(--muted); }
  .veil { z-index: 1; }
  .button { display: inline-block; margin-top: 1rem; background: var(--ember); color: var(--ember-fg); text-decoration: none; padding: 0.6rem 1.2rem; border-radius: 999px; font-weight: 600; }
  @media (max-width: 800px) {
    .menu { display: flex; }
    .scrim { display: block; }
    .topbar { padding: 0.7rem 0.9rem; }
  }
</style>
