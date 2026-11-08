<script lang="ts">
  import { login } from './auth.svelte';
  import type { ApiError } from '../lib/types';

  let mode = $state<'password' | 'key'>('password');
  let slug = $state('');
  let password = $state('');
  let key = $state('');
  let busy = $state(false);
  let error = $state('');

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    busy = true;
    error = '';
    try {
      await login(mode === 'password' ? { slug, password } : { api_key: key });
    } catch (e) {
      const err = e as ApiError;
      error = err.status === 403 ? 'Not accepted. Check the slug and password, or the key.' : err.status === 429 ? 'Too many attempts. Try again in a few minutes.' : err.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <form class="card" onsubmit={submit}>
    <p class="eyebrow">handout</p>
    <h1>Sign in as owner</h1>
    <div class="tabs">
      <button type="button" class:active={mode === 'password'} onclick={() => (mode = 'password')}>Password</button>
      <button type="button" class:active={mode === 'key'} onclick={() => (mode = 'key')}>API key</button>
    </div>
    {#if mode === 'password'}
      <label class="field">Owner slug<input class="input" bind:value={slug} autocomplete="username" required /></label>
      <label class="field">Password<input class="input" type="password" bind:value={password} autocomplete="current-password" required /></label>
    {:else}
      <label class="field">API key<input class="input mono" type="password" bind:value={key} placeholder="hk_…" autocomplete="off" required /></label>
      <p class="muted small">Any key created with <code>handout key add</code> signs you in as its owner.</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    <button class="btn primary" disabled={busy}>{busy ? 'Signing in…' : 'Sign in'}</button>
  </form>
</div>

<style>
  .wrap { min-height: 100%; display: grid; place-items: center; padding: 1.5rem; }
  form { width: min(24rem, 100%); animation: pop 0.4s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
  .eyebrow { margin: 0 0 0.3rem; font-size: 0.7rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--ember); font-weight: 700; }
  h1 { margin: 0 0 1rem; font-size: 1.5rem; letter-spacing: -0.02em; }
  .tabs { display: flex; gap: 0.3rem; background: var(--paper); border-radius: 9px; padding: 0.25rem; }
  .tabs button { flex: 1; border: 0; border-radius: 7px; padding: 0.4rem; background: transparent; color: var(--muted); cursor: pointer; font-weight: 600; }
  .tabs button.active { background: var(--card); color: var(--ink); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08); }
  .btn { margin-top: 1.1rem; width: 100%; }
  @keyframes pop { from { opacity: 0; transform: translateY(10px) scale(0.98); } to { opacity: 1; transform: none; } }
</style>
