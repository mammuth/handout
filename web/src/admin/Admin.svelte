<script lang="ts">
  import './admin.css';
  import { session, boot, logout } from './auth.svelte';
  import Login from './Login.svelte';
  import Files from './Files.svelte';
  import Recipients from './Recipients.svelte';
  import Activity from './Activity.svelte';
  import Keys from './Keys.svelte';

  let { rest, navigate }: { rest: string[]; navigate: (to: string) => void } = $props();
  const section = $derived(rest[0] ?? 'files');
  const selected = $derived(rest[1] ?? null);

  $effect(() => {
    if (!session.booted) boot();
  });

  function go(e: MouseEvent, to: string) {
    if (e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    navigate(to);
  }
  async function out() {
    await logout();
    navigate('/admin');
  }
</script>

{#if !session.booted}
  <div class="empty">Loading…</div>
{:else if !session.owner}
  <Login />
{:else}
  <div class="admin">
    <nav class="side">
      <div class="brand"><span class="dot"></span>handout</div>
      <a href="/admin" class:active={section === 'files'} onclick={(e) => go(e, '/admin')}>Files</a>
      <a href="/admin/recipients" class:active={section === 'recipients'} onclick={(e) => go(e, '/admin/recipients')}>Recipients</a>
      <a href="/admin/activity" class:active={section === 'activity'} onclick={(e) => go(e, '/admin/activity')}>Activity</a>
      <a href="/admin/keys" class:active={section === 'keys'} onclick={(e) => go(e, '/admin/keys')}>API keys</a>
      <div class="who">
        <strong>{session.owner.name}</strong>
        <span class="mono">/s/{session.owner.slug}/</span>
        <div style="margin-top:.5rem"><button class="btn small" onclick={out}>Sign out</button></div>
      </div>
    </nav>
    <main>
      {#if section === 'files'}
        <Files selected={selected} {navigate} />
      {:else if section === 'recipients'}
        <Recipients selected={selected} {navigate} />
      {:else if section === 'activity'}
        <Activity {navigate} />
      {:else if section === 'keys'}
        <Keys />
      {:else}
        <div class="empty">Unknown section.</div>
      {/if}
    </main>
  </div>
{/if}
