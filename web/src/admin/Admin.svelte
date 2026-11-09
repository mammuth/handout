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
    <nav class="side glass">
      <div class="brand"><img class="mark" src="/favicon.svg" alt="" />handout</div>
      <a href="/admin" class:active={section === 'files'} onclick={(e) => go(e, '/admin')}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/></svg>Files</a>
      <a href="/admin/recipients" class:active={section === 'recipients'} onclick={(e) => go(e, '/admin/recipients')}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M16 4.5a3.5 3.5 0 0 1 0 7"/><path d="M18.5 14.5A6.5 6.5 0 0 1 21.5 20"/></svg>Recipients</a>
      <a href="/admin/activity" class:active={section === 'activity'} onclick={(e) => go(e, '/admin/activity')}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 12h4l3 7 4-14 3 7h4"/></svg>Activity</a>
      <a href="/admin/keys" class:active={section === 'keys'} onclick={(e) => go(e, '/admin/keys')}><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="8" cy="15" r="4"/><path d="M11 12l9-9"/><path d="M17 6l3 3"/></svg>API keys</a>
      <div class="who">
        <span class="avatar">{session.owner.name.slice(0, 1).toUpperCase()}</span>
        <div><strong>{session.owner.name}</strong><span class="mono">/s/{session.owner.slug}/</span></div>
        <button class="btn small" onclick={out}>Sign out</button>
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
