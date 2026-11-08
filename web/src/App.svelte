<script lang="ts">
  import Viewer from './Viewer.svelte';
  import Home from './Home.svelte';
  import Admin from './admin/Admin.svelte';

  let path = $state(location.pathname);

  function parse(p: string): { kind: 'viewer'; owner: string; file: string } | { kind: 'home'; owner: string } | { kind: 'admin'; rest: string[] } | { kind: 'none' } {
    const parts = p.split('/').filter(Boolean).map(decodeURIComponent);
    if (parts[0] === 'admin') return { kind: 'admin', rest: parts.slice(1) };
    if (parts[0] === 's' && parts.length === 3) return { kind: 'viewer', owner: parts[1], file: parts[2] };
    if (parts[0] === 's' && parts.length === 2) return { kind: 'home', owner: parts[1] };
    return { kind: 'none' };
  }

  const route = $derived(parse(path));

  export function navigate(to: string) {
    history.pushState({}, '', to);
    path = to;
  }

  $effect(() => {
    const onPop = () => (path = location.pathname);
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  });
</script>

{#if route.kind === 'viewer'}
  <Viewer owner={route.owner} file={route.file} {navigate} />
{:else if route.kind === 'home'}
  <Home owner={route.owner} {navigate} />
{:else if route.kind === 'admin'}
  <Admin rest={route.rest} {navigate} />
{:else}
  <main class="empty"><h1>handout</h1><p>Nothing to see at this address.</p></main>
{/if}

<style>
  .empty { min-height: 100%; display: grid; place-content: center; text-align: center; color: var(--muted); }
  .empty h1 { color: var(--ink); letter-spacing: -0.03em; font-size: 2.6rem; margin: 0 0 0.3rem; }
</style>
