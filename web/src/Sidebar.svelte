<script lang="ts">
  import { formatDate, kindLabel } from './lib/api';
  import type { SidebarEntry } from './lib/types';

  let {
    ownerName,
    recipient,
    entries,
    currentId = null,
    navigate,
    open = $bindable(false),
  }: {
    ownerName: string;
    recipient: { name: string } | null;
    entries: SidebarEntry[];
    currentId?: string | null;
    navigate: (to: string) => void;
    open?: boolean;
  } = $props();

  const newCount = $derived(entries.filter((e) => e.is_new && e.id !== currentId).length);

  function go(e: MouseEvent, to: string) {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    open = false;
    navigate(to);
  }
</script>

<aside class:open>
  <header>
    <div class="brand">
      <span class="dot"></span>
      <span class="from">from</span>
      <strong>{ownerName}</strong>
    </div>
    {#if recipient}
      <p class="for">Shared with <strong>{recipient.name}</strong></p>
    {/if}
  </header>

  {#if entries.length}
    <p class="section">
      {entries.length} {entries.length === 1 ? 'file' : 'files'}
      {#if newCount}<span class="count">{newCount} new</span>{/if}
    </p>
    <nav>
      {#each entries as e, i (e.id)}
        <a
          href={e.link}
          class:current={e.id === currentId}
          class:new={e.is_new && e.id !== currentId}
          style="--i: {i}"
          onclick={(ev) => go(ev, e.link)}
        >
          <span class="kind">{kindLabel[e.kind] ?? 'File'}</span>
          <span class="title">{e.title}</span>
          <span class="meta">
            {formatDate(e.updated_at)}
            {#if e.is_new && e.id !== currentId}<span class="badge">new</span>{/if}
          </span>
        </a>
      {/each}
    </nav>
  {:else if recipient}
    <p class="hint">Nothing else has been shared with you yet.</p>
  {:else}
    <p class="hint">Open a personal link to see everything shared with you in one place.</p>
  {/if}

  <footer><img class="mark" src="/favicon.svg" alt="" />handout</footer>
</aside>

<style>
  aside {
    width: var(--sidebar-w);
    flex: 0 0 auto;
    height: 100%;
    overflow-y: auto;
    padding: 1.4rem 1.1rem 1rem;
    border-right: 1px solid var(--line);
    background: color-mix(in oklab, var(--paper-2) 80%, transparent);
    backdrop-filter: blur(12px);
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  header { margin-bottom: 0.6rem; }
  .brand { display: flex; align-items: baseline; gap: 0.45rem; font-size: 1.05rem; letter-spacing: -0.01em; }
  .brand strong { font-weight: 700; font-variation-settings: 'wdth' 90; }
  .from { color: var(--muted); font-size: 0.85rem; }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--ember); box-shadow: 0 0 0 4px var(--ember-soft); align-self: center; }
  .for { margin: 0.6rem 0 0; color: var(--ink-2); font-size: 0.92rem; }
  .section { margin: 0.8rem 0 0.4rem; font-size: 0.72rem; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); display: flex; justify-content: space-between; }
  .count { color: var(--new); font-weight: 700; }
  nav { display: flex; flex-direction: column; gap: 0.3rem; }
  nav a {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.1rem;
    padding: 0.6rem 0.75rem;
    border-radius: 10px;
    border: 1px solid transparent;
    color: var(--ink);
    text-decoration: none;
    animation: rise 0.45s cubic-bezier(0.2, 0.7, 0.2, 1) both;
    animation-delay: calc(var(--i) * 45ms);
  }
  nav a:hover { background: var(--card); border-color: var(--line); }
  nav a.current { background: var(--card); border-color: var(--line); box-shadow: var(--shadow); }
  nav a.new .title { font-weight: 700; }
  .kind { font-size: 0.68rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--muted); }
  .title { font-size: 0.98rem; line-height: 1.25; letter-spacing: -0.01em; }
  .meta { font-size: 0.78rem; color: var(--muted); display: flex; gap: 0.5rem; align-items: center; }
  .badge { font-size: 0.66rem; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--new); background: var(--new-soft); padding: 0.08rem 0.4rem; border-radius: 999px; }
  .hint { color: var(--muted); font-size: 0.9rem; line-height: 1.4; }
  footer { display: flex; align-items: center; gap: 0.4rem; margin-top: auto; padding-top: 1rem; font-size: 0.72rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--muted); }
  footer .mark { width: 18px; height: 18px; }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @media (max-width: 800px) {
    aside {
      position: fixed; inset: 0 auto 0 0; z-index: 20; width: min(86vw, var(--sidebar-w));
      transform: translateX(-104%); transition: transform 0.28s cubic-bezier(0.2, 0.7, 0.2, 1);
      background: var(--paper-2); box-shadow: var(--shadow);
    }
    aside.open { transform: none; }
  }
</style>
