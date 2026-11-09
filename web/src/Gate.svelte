<script lang="ts">
  import { api } from './lib/api';
  import type { ApiError, ViewStateKind } from './lib/types';

  let { state: gate, owner, file, onUnlocked }: { state: ViewStateKind; owner: string; file: string; onUnlocked: () => void } = $props();

  let password = $state('');
  let email = $state('');
  let name = $state('');
  let busy = $state(false);
  let error = $state('');

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    busy = true;
    error = '';
    try {
      if (gate === 'password') await api.unlock(owner, file, password);
      else await api.identify(owner, file, email, name);
      onUnlocked();
    } catch (e) {
      const err = e as ApiError;
      error = gate === 'password' && err.status === 403 ? 'That password is not right.' : err.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="gate">
  <div class="card glass">
    {#if gate === 'password'}
      <p class="eyebrow">Protected</p>
      <h2>This file needs a password.</h2>
      <form onsubmit={submit}>
        <label>Password<input type="password" bind:value={password} autocomplete="off" required /></label>
        {#if error}<p class="error">{error}</p>{/if}
        <button disabled={busy}>{busy ? 'Checking…' : 'Open'}</button>
      </form>
    {:else if gate === 'email'}
      <p class="eyebrow">Almost there</p>
      <h2>Tell us who you are.</h2>
      <p class="muted">Your email lets the sender know you got it, and keeps everything they share with you in one place.</p>
      <form onsubmit={submit}>
        <label>Email<input type="email" bind:value={email} autocomplete="email" required /></label>
        <label>Name <span class="opt">optional</span><input bind:value={name} autocomplete="name" /></label>
        {#if error}<p class="error">{error}</p>{/if}
        <button disabled={busy}>{busy ? 'One moment…' : 'Continue'}</button>
      </form>
    {:else if gate === 'expired'}
      <p class="eyebrow">Expired</p>
      <h2>This link has expired.</h2>
      <p class="muted">Ask the sender for a fresh one.</p>
    {:else if gate === 'forbidden'}
      <p class="eyebrow">Private</p>
      <h2>This file is only for invited people.</h2>
      <p class="muted">Open the personal link you received, or ask the sender to share it with you.</p>
    {:else}
      <p class="eyebrow">Unavailable</p>
      <h2>This file is no longer shared.</h2>
    {/if}
  </div>
</div>

<style>
  .gate { position: absolute; inset: 0; display: grid; place-items: center; padding: 1.5rem; }
  .card { width: min(26rem, 100%); background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-lg); padding: 1.3rem 1.4rem 1.2rem; box-shadow: var(--glass-edge), var(--shadow); animation: pop 0.4s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .eyebrow { margin: 0 0 0.4rem; font-size: 0.7rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--muted); font-weight: 700; }
  h2 { margin: 0 0 0.5rem; font-size: 1.5rem; letter-spacing: -0.02em; line-height: 1.15; }
  .muted { color: var(--muted); margin: 0 0 0.6rem; }
  label { display: block; margin: 0.9rem 0 0; font-size: 0.85rem; color: var(--muted); }
  .opt { font-size: 0.72rem; letter-spacing: 0.06em; text-transform: uppercase; }
  input { display: block; width: 100%; margin-top: 0.3rem; padding: 0.5rem 0.65rem; border: 1px solid var(--line); border-radius: var(--radius); background: var(--well); color: var(--ink); font: inherit; }
  input:focus { outline: 2px solid rgba(255, 255, 255, 0.6); outline-offset: 1px; border-color: transparent; }
  button { margin-top: 1rem; width: 100%; padding: 0.55rem 1rem; border: 0; border-radius: 999px; background: var(--ember); color: var(--ember-fg); font-weight: 700; cursor: pointer; }
  button:disabled { opacity: 0.6; cursor: wait; }
  .error { color: var(--danger); margin: 0.6rem 0 0; font-size: 0.9rem; }
  @keyframes pop { from { opacity: 0; transform: translateY(10px) scale(0.98); } to { opacity: 1; transform: none; } }
</style>
