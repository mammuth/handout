<script lang="ts">
  import KindIcon from './KindIcon.svelte';
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

  const initials = (name: string) =>
    name.split(/\s+/).filter(Boolean).slice(0, 2).map((w) => w[0].toUpperCase()).join('');

  function go(e: MouseEvent, to: string) {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    open = false;
    navigate(to);
  }
</script>

<aside class="glass" class:open>
  <header>
    <span class="avatar">{initials(ownerName)}</span>
    <div class="who">
      <strong>{ownerName}</strong>
      <span>Shared with {recipient.name}</span>
    </div>
  </header>

  {#if entries.length}
    <p class="section">
      <span>Shared with you</span>
      {#if newCount}<span class="count">{newCount} new</span>{:else}<span>{entries.length}</span>{/if}
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
          <KindIcon kind={e.kind} />
          <span class="text">
            <span class="title">{e.title}</span>
            <span class="meta">{kindLabel[e.kind] ?? 'File'} · {formatDate(e.updated_at)}</span>
          </span>
          {#if e.is_new && e.id !== currentId}<span class="badge">New</span>{/if}
        </a>
      {/each}
    </nav>
  {:else}
    <p class="hint">Nothing else has been shared with you yet.</p>
  {/if}

  <footer><a href="https://github.com/mammuth/handout" target="_blank" rel="noopener"><img class="mark" src="/favicon.svg" alt="" />Powered by handout</a></footer>
</aside>

<style>
  aside {
    width: var(--sidebar-w);
    flex: 0 0 auto;
    margin: var(--gap) 0 var(--gap) var(--gap);
    overflow-y: auto;
    padding: 0.8rem 0.5rem 0.6rem;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  header { display: flex; align-items: center; gap: 0.6rem; padding: 0.2rem 0.5rem 0.8rem; }
  .avatar { width: 32px; height: 32px; flex: 0 0 auto; border-radius: 50%; display: grid; place-content: center; background: var(--aurora); color: #fff; font-weight: 700; font-size: 0.8rem; }
  .who { min-width: 0; display: flex; flex-direction: column; }
  .who strong { font-family: var(--font-display); font-size: 1.05rem; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .who span { font-size: 0.75rem; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .section { margin: 0.1rem 0 0.25rem; padding: 0 0.5rem; font-size: 0.72rem; font-weight: 500; color: var(--muted); display: flex; justify-content: space-between; }
  .count { color: var(--ink); font-weight: 600; }
  nav { display: flex; flex-direction: column; gap: 0.15rem; }
  nav a {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.35rem 0.5rem;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--ink);
    text-decoration: none;
    animation: rise 0.45s cubic-bezier(0.2, 0.7, 0.2, 1) both;
    animation-delay: calc(var(--i) * 45ms);
  }
  nav a:hover { background: var(--paper-2); }
  nav a.current { background: var(--ember-soft); border-color: rgba(255, 255, 255, 0.22); }
  nav a.current .title, nav a.new .title { font-weight: 600; }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .title { font-weight: 500; line-height: 1.3; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 0.72rem; color: var(--muted); }
  .badge { flex: 0 0 auto; font-size: 0.68rem; font-weight: 700; color: #fff; background: var(--aurora); padding: 0.05rem 0.5rem; border-radius: 999px; }
  .hint { color: var(--muted); font-size: 0.85rem; line-height: 1.4; padding: 0 0.5rem; }
  footer { margin-top: auto; padding: 0.7rem 0.5rem 0.1rem; font-size: 0.72rem; color: var(--muted); }
  footer a { display: flex; align-items: center; gap: 0.4rem; color: inherit; text-decoration: none; }
  footer a:hover { color: var(--ink); }
  footer .mark { width: 16px; height: 16px; }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @media (max-width: 800px) {
    aside {
      position: fixed; inset: var(--gap) auto var(--gap) 0; z-index: 20; width: min(86vw, var(--sidebar-w));
      transform: translateX(-110%); transition: transform 0.28s cubic-bezier(0.2, 0.7, 0.2, 1);
      background: rgba(12, 16, 40, 0.85); box-shadow: var(--shadow);
    }
    aside.open { transform: none; }
  }
</style>
