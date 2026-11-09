import type { ApiError, HomeResponse, ViewResponse } from './types';

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, { credentials: 'same-origin', ...init });
  if (!res.ok) {
    let code = 'error';
    let message = res.statusText;
    try {
      const body = await res.json();
      code = body?.error?.code ?? code;
      message = body?.error?.message ?? message;
    } catch {}
    throw { status: res.status, code, message } satisfies ApiError;
  }
  return (await res.json()) as T;
}

const json = (body: unknown): RequestInit => ({
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(body),
});

export const api = {
  view: (owner: string, file: string) => call<ViewResponse>(`/api/view/${enc(owner)}/${enc(file)}`),
  home: (owner: string) => call<HomeResponse>(`/api/view/${enc(owner)}`),
  unlock: (owner: string, file: string, password: string) =>
    call<{ ok: boolean }>(`/api/view/${enc(owner)}/${enc(file)}/unlock`, json({ password })),
  identify: (owner: string, file: string, email: string, name: string) =>
    call<{ ok: boolean }>(`/api/view/${enc(owner)}/${enc(file)}/identify`, json({ email, name: name || undefined })),
};

const enc = encodeURIComponent;

export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const now = new Date();
  const sameYear = d.getFullYear() === now.getFullYear();
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', ...(sameYear ? {} : { year: 'numeric' }) });
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m < 60) return s ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

export const kindLabel: Record<string, string> = {
  html: 'Page',
  markdown: 'Document',
  pdf: 'PDF',
  image: 'Image',
  other: 'File',
};
