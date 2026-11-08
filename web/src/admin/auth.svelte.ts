import type { Owner } from './types';
import type { ApiError } from '../lib/types';

export const session = $state<{ owner: Owner | null; token: string | null; booted: boolean }>({ owner: null, token: null, booted: false });

async function readError(res: Response): Promise<ApiError> {
  let code = 'error';
  let message = res.statusText;
  try {
    const body = await res.json();
    code = body?.error?.code ?? code;
    message = body?.error?.message ?? message;
  } catch {}
  return { status: res.status, code, message };
}

/** Mint a short-lived API token from the session cookie (page load / token expiry). */
export async function boot(): Promise<boolean> {
  try {
    const res = await fetch('/auth/token', { method: 'POST', credentials: 'same-origin' });
    if (!res.ok) {
      session.owner = null;
      session.token = null;
      return false;
    }
    const body = await res.json();
    session.owner = body.owner;
    session.token = body.access_token;
    return true;
  } finally {
    session.booted = true;
  }
}

export async function login(body: { slug?: string; password?: string; api_key?: string }) {
  const res = await fetch('/auth/login', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw await readError(res);
  const data = await res.json();
  session.owner = data.owner;
  session.token = data.access_token;
}

export async function logout() {
  await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin' });
  session.owner = null;
  session.token = null;
}

/** Bearer call to the owner API. Retries once with a fresh token on 401. */
export async function api<T>(path: string, init: RequestInit = {}, retry = true): Promise<T> {
  const headers = new Headers(init.headers);
  if (session.token) headers.set('authorization', `Bearer ${session.token}`);
  if (init.body && !(init.body instanceof FormData) && !headers.has('content-type')) headers.set('content-type', 'application/json');
  const res = await fetch(path, { ...init, headers, credentials: 'omit' });
  if (res.status === 401 && retry && (await boot())) return api<T>(path, init, false);
  if (!res.ok) throw await readError(res);
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const json = (body: unknown): RequestInit => ({ method: 'POST', body: JSON.stringify(body) });
export const patch = (body: unknown): RequestInit => ({ method: 'PATCH', body: JSON.stringify(body) });
