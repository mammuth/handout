<script lang="ts">
  let { text, label = 'Copy' }: { text: string; label?: string } = $props();
  let done = $state(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      document.body.append(ta);
      ta.select();
      document.execCommand('copy');
      ta.remove();
    }
    done = true;
    setTimeout(() => (done = false), 1400);
  }
</script>

<button class="btn small" type="button" onclick={copy}>{done ? 'Copied' : label}</button>
