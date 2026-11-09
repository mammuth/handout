<script lang="ts">
  import Brand from './Brand.svelte';
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
    recipient: { name: string };
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
    <Brand {ownerName} />
    <p class="for">Shared with <strong>{recipient.name}</strong></p>
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
  {:else}
    <p class="hint">Nothing else has been shared with you yet.</p>
  {/if}

  <footer><img class="mark" src="/favicon.svg" alt="" />handout</footer>
</aside>

<style>
  aside {
    /* dark teal rail: remap the page tokens so nested components (Brand) read on it */
    --ink: var(--rail-ink);
    --ink-2: var(--rail-ink);
    --muted: var(--rail-muted);
    --line: var(--rail-line);
    --ember: var(--rail-accent);
    --ember-soft: color-mix(in oklab, var(--rail-accent) 25%, transparent);
    width: var(--sidebar-w);
    flex: 0 0 auto;
    height: 100%;
    overflow-y: auto;
    padding: 1rem 0.6rem 0.8rem;
    border-right: 1px solid var(--line);
    background: var(--rail);
    color: var(--ink);
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  header { margin-bottom: 0.4rem; padding: 0 0.5rem; }
  .for { margin: 0.45rem 0 0; color: var(--ink-2); font-size: 0.88rem; }
  .section { margin: 0.6rem 0 0.2rem; padding: 0 0.5rem; font-size: 0.7rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--muted); display: flex; justify-content: space-between; }
  .count { color: var(--rail-accent); font-weight: 700; }
  nav { display: flex; flex-direction: column; gap: 0.1rem; }
  nav a {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.05rem;
    padding: 0.45rem 0.55rem;
    border-radius: var(--radius);
    color: var(--ink);
    text-decoration: none;
    animation: rise 0.45s cubic-bezier(0.2, 0.7, 0.2, 1) both;
    animation-delay: calc(var(--i) * 45ms);
  }
  nav a:hover { background: var(--rail-active); }
  nav a.current { background: var(--rail-active); box-shadow: inset 2px 0 0 var(--rail-accent); }
  nav a.new .title { font-weight: 700; }
  .kind { font-size: 0.66rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--muted); }
  .title { font-size: 0.93rem; line-height: 1.25; }
  .meta { font-size: 0.75rem; color: var(--muted); display: flex; gap: 0.5rem; align-items: center; }
  .badge { font-size: 0.64rem; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--rail-accent-fg); background: var(--rail-accent); padding: 0.05rem 0.4rem; border-radius: 999px; }
  .hint { color: var(--muted); font-size: 0.88rem; line-height: 1.4; padding: 0 0.5rem; }
  footer { display: flex; align-items: center; gap: 0.4rem; margin-top: auto; padding: 0.8rem 0.5rem 0; font-size: 0.7rem; letter-spacing: 0.14em; text-transform: uppercase; color: var(--muted); }
  footer .mark { width: 18px; height: 18px; }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @media (max-width: 800px) {
    aside {
      position: fixed; inset: 0 auto 0 0; z-index: 20; width: min(86vw, var(--sidebar-w));
      transform: translateX(-104%); transition: transform 0.28s cubic-bezier(0.2, 0.7, 0.2, 1);
      box-shadow: var(--shadow);
    }
    aside.open { transform: none; }
  }
</style>
