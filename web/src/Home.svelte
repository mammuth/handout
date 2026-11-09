<script lang="ts">
  import Sidebar from './Sidebar.svelte';
  import { api } from './lib/api';
  import type { ApiError, HomeResponse } from './lib/types';

  let { owner, navigate }: { owner: string; navigate: (to: string) => void } = $props();
  let data = $state<HomeResponse | null>(null);
  let error = $state<ApiError | null>(null);
  let menuOpen = $state(false);

  $effect(() => {
    api.home(owner).then((d) => { data = d; document.title = `Shared with you · ${d.owner.name}`; }).catch((e) => (error = e as ApiError));
  });
</script>

<div class="layout">
  {#if data}
    <Sidebar ownerName={data.owner.name} recipient={data.recipient} entries={data.sidebar} {navigate} bind:open={menuOpen} />
  {/if}
  <section class="main">
    <div class="center">
      {#if data}
        <p class="eyebrow">Shared with {data.recipient.name}</p>
        <h1>{data.sidebar.length ? 'Pick a file to open.' : 'Nothing here yet.'}</h1>
        <button class="menu" onclick={() => (menuOpen = true)}>Show files</button>
      {:else if error}
        <p class="eyebrow">handout</p>
        <h1>{error.status === 403 ? 'Open your personal link first.' : 'Nothing to see here.'}</h1>
      {/if}
    </div>
  </section>
</div>

<style>
  .layout { display: flex; height: 100%; }
  .main { flex: 1; display: grid; place-items: center; padding: 2rem; }
  .center { text-align: center; max-width: 30rem; }
  .eyebrow { font-size: 0.72rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--ember); font-weight: 700; margin: 0 0 0.4rem; }
  h1 { font-size: 2rem; letter-spacing: -0.03em; margin: 0; }
  .menu { display: none; margin-top: 1rem; padding: 0.6rem 1.1rem; border-radius: 999px; border: 1px solid var(--line); background: var(--card); cursor: pointer; }
  @media (max-width: 800px) { .menu { display: inline-block; } }
</style>
