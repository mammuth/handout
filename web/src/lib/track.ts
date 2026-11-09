/**
 * Engagement tracker for one open (view session). Counts seconds the tab is visible and seconds
 * with recent input (shell events or beacon messages from the content iframe), tracks max scroll
 * depth, and reports deltas to the viewer API every 15 s, on tab hide and on unload.
 */
export interface OpenRef { id: number; token: string }

const ACTIVE_WINDOW_MS = 10_000;
const FLUSH_EVERY_MS = 15_000;

export function startTracking(owner: string, file: string, open: OpenRef, frame: () => HTMLIFrameElement | null): () => void {
  let visible = 0;
  let active = 0;
  let maxScroll = 0;
  let lastActivity = Date.now();
  const url = `/api/view/${encodeURIComponent(owner)}/${encodeURIComponent(file)}/heartbeat`;

  const touch = () => (lastActivity = Date.now());
  const onMessage = (e: MessageEvent) => {
    const win = frame()?.contentWindow;
    if (!win || e.source !== win || !e.data || e.data.handout !== 'activity') return;
    if (e.data.active) touch();
    if (typeof e.data.scroll === 'number') maxScroll = Math.max(maxScroll, Math.min(100, e.data.scroll));
  };
  const inputEvents = ['mousemove', 'keydown', 'pointerdown', 'touchstart', 'wheel', 'scroll'] as const;
  inputEvents.forEach((ev) => window.addEventListener(ev, touch, { passive: true, capture: true }));
  window.addEventListener('message', onMessage);

  const tick = setInterval(() => {
    if (document.visibilityState !== 'visible') return;
    visible += 1;
    if (Date.now() - lastActivity < ACTIVE_WINDOW_MS) active += 1;
  }, 1000);

  function flush(beacon = false) {
    if (visible === 0 && maxScroll === 0) return;
    const body = JSON.stringify({ open_id: open.id, open_token: open.token, visible_seconds: visible, active_seconds: active, max_scroll_pct: maxScroll });
    visible = 0;
    active = 0;
    if (beacon && navigator.sendBeacon) {
      navigator.sendBeacon(url, body);
    } else {
      fetch(url, { method: 'POST', body, keepalive: true, credentials: 'same-origin' }).catch(() => {});
    }
  }

  const flusher = setInterval(() => flush(false), FLUSH_EVERY_MS);
  const onVisibility = () => {
    if (document.visibilityState === 'hidden') flush(true);
  };
  const onPageHide = () => flush(true);
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('pagehide', onPageHide);

  return () => {
    clearInterval(tick);
    clearInterval(flusher);
    inputEvents.forEach((ev) => window.removeEventListener(ev, touch, { capture: true } as EventListenerOptions));
    window.removeEventListener('message', onMessage);
    document.removeEventListener('visibilitychange', onVisibility);
    window.removeEventListener('pagehide', onPageHide);
    flush(true);
  };
}
