<script lang="ts">
  import { api, json, patch } from './auth.svelte';
  import { formatDate, formatDuration, formatSize, kindLabel } from '../lib/api';
  import type { ApiError } from '../lib/types';
  import type { FileDetail, ShareResult } from './types';
  import Copy from './Copy.svelte';

  let { id, onChanged, onDeleted, navigate }: { id: string; onChanged: () => void; onDeleted: () => void; navigate: (to: string) => void } = $props();

  let f = $state<FileDetail | null>(null);
  let error = $state('');
  let notice = $state('');
  // settings
  let title = $state('');
  let access = $state('link');
  let emailGate = $state(false);
  let expires = $state('');
  let password = $state('');
  let clearPassword = $state(false);
  // share
  let sName = $state('');
  let sEmail = $state('');
  let sCompany = $state('');
  let shared = $state<ShareResult | null>(null);
  let versionInput = $state<HTMLInputElement | null>(null);
  let busy = $state(false);

  function toLocal(iso: string | null): string {
    if (!iso) return '';
    const d = new Date(iso);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  async function load() {
    error = '';
    shared = null;
    try {
      f = await api<FileDetail>(`/api/files/${encodeURIComponent(id)}`);
      title = f.title;
      access = f.access;
      emailGate = f.email_gate;
      expires = toLocal(f.expires_at);
      password = '';
      clearPassword = false;
    } catch (e) {
      error = (e as ApiError).message;
      f = null;
    }
  }
  $effect(() => {
    id;
    load();
  });

  function flash(msg: string) {
    notice = msg;
    setTimeout(() => (notice = ''), 1800);
  }

  async function run(fn: () => Promise<void>, done?: string) {
    busy = true;
    error = '';
    try {
      await fn();
      if (done) flash(done);
    } catch (e) {
      error = (e as ApiError).message;
    } finally {
      busy = false;
    }
  }

  const save = () =>
    run(async () => {
      const body: Record<string, unknown> = { title, access, email_gate: emailGate, expires_at: expires ? new Date(expires).toISOString() : null };
      if (clearPassword) body.password = null;
      else if (password) body.password = password;
      f = await api<FileDetail>(`/api/files/${encodeURIComponent(id)}`, patch(body));
      password = '';
      clearPassword = false;
      onChanged();
    }, 'Saved');

  const share = () =>
    run(async () => {
      shared = await api<ShareResult>(`/api/files/${encodeURIComponent(id)}/share`, json({ name: sName || undefined, email: sEmail || undefined, company: sCompany || undefined }));
      sName = sEmail = sCompany = '';
      f = await api<FileDetail>(`/api/files/${encodeURIComponent(id)}`);
    });

  const revoke = (rid: string) =>
    run(async () => {
      await api(`/api/files/${encodeURIComponent(id)}/invites/${rid}`, { method: 'DELETE' });
      f = await api<FileDetail>(`/api/files/${encodeURIComponent(id)}`);
    }, 'Invite removed');

  const newVersion = (list: FileList | null) => {
    const file = list?.[0];
    if (!file) return;
    run(async () => {
      const fd = new FormData();
      fd.append('file', file, file.name);
      f = await api<FileDetail>(`/api/files/${encodeURIComponent(id)}/content`, { method: 'PUT', body: fd });
      if (versionInput) versionInput.value = '';
      onChanged();
    }, `Version ${f ? f.version + 1 : ''} published`);
  };

  const remove = () => {
    if (!confirm('Delete this file? All its links stop working immediately.')) return;
    run(async () => {
      await api(`/api/files/${encodeURIComponent(id)}`, { method: 'DELETE' });
      onDeleted();
    });
  };
</script>

<div class="card stack">
  {#if !f}
    <p class="muted">{error || 'Loading…'}</p>
  {:else}
    <div>
      <span class="badge">{kindLabel[f.kind]}</span>
      <span class="muted small"> · v{f.version} · {formatSize(f.size)} · {f.opens} {f.opens === 1 ? 'open' : 'opens'}{#if f.active_seconds} · {formatDuration(f.active_seconds)} active{/if}</span>
      <h2 style="margin-top:.3rem">{f.title}</h2>
      <div class="linkbox"><code>{f.file_link}</code><Copy text={f.file_link} /><a class="btn small" href={f.file_link} target="_blank" rel="noopener">Open</a></div>
    </div>

    <h3>Share with a recipient</h3>
    <form onsubmit={(e) => { e.preventDefault(); share(); }} class="stack">
      <div class="row">
        <input class="input grow" placeholder="Name" bind:value={sName} />
        <input class="input grow" placeholder="Email (matches existing recipients)" bind:value={sEmail} />
      </div>
      <div class="row">
        <input class="input grow" placeholder="Company (optional)" bind:value={sCompany} />
        <button class="btn primary" disabled={busy || (!sName && !sEmail)}>Share</button>
      </div>
    </form>
    {#if shared}
      <div class="reveal">
        <div class="small"><strong>{shared.recipient.name}</strong> {shared.recipient_created ? '(new recipient)' : '(existing recipient)'} — send them this link:</div>
        <div class="linkbox" style="margin-top:.4rem"><code>{shared.recipient_link}</code><Copy text={shared.recipient_link} /></div>
      </div>
    {/if}

    {#if f.invites.length}
      <h3>Invited</h3>
      <table class="list">
        <tbody>
          {#each f.invites as inv (inv.recipient.id)}
            <tr>
              <td>
                <a href="/admin/recipients/{inv.recipient.id}" onclick={(e) => { e.preventDefault(); navigate(`/admin/recipients/${inv.recipient.id}`); }}><strong>{inv.recipient.name}</strong></a>
                <div class="muted small">{inv.recipient.email ?? ''} {inv.last_opened_at ? `· opened ${formatDate(inv.last_opened_at)}` : '· not opened yet'}{#if inv.active_seconds} · {formatDuration(inv.active_seconds)} active{/if}{#if inv.max_scroll_pct} · read {inv.max_scroll_pct}%{/if}</div>
              </td>
              <td style="text-align:right;white-space:nowrap">
                <Copy text={inv.recipient_link} label="Copy link" />
                <button class="btn small danger" onclick={() => revoke(inv.recipient.id)} disabled={busy}>Remove</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <h3>Settings</h3>
    <form onsubmit={(e) => { e.preventDefault(); save(); }}>
      <label class="field">Title<input class="input" bind:value={title} /></label>
      <label class="field">Access
        <select class="select" bind:value={access}>
          <option value="link">Anyone with the link</option>
          <option value="recipients">Invited recipients only</option>
          <option value="off">Off (revoked)</option>
        </select>
      </label>
      <label class="field">Password {f.has_password ? '(set)' : '(none)'}<input class="input" type="password" bind:value={password} placeholder={f.has_password ? 'Change…' : 'Add a password'} autocomplete="new-password" disabled={clearPassword} /></label>
      {#if f.has_password}<label class="check"><input type="checkbox" bind:checked={clearPassword} /> Remove password</label>{/if}
      <label class="check"><input type="checkbox" bind:checked={emailGate} /> Ask viewers for their email (becomes a recipient)</label>
      <label class="field">Expires<input class="input" type="datetime-local" bind:value={expires} /></label>
      <div class="row" style="margin-top:.8rem"><button class="btn primary" disabled={busy}>Save</button>{#if notice}<span class="ok">{notice}</span>{/if}</div>
    </form>

    <h3>Versions</h3>
    <div class="row">
      <span class="muted small">{f.versions.length} {f.versions.length === 1 ? 'version' : 'versions'} · latest {formatDate(f.versions[0]?.created_at)} · {f.filename}</span>
      <button class="btn small" type="button" onclick={() => versionInput?.click()} disabled={busy}>Upload new version</button>
      <input bind:this={versionInput} type="file" hidden onchange={(e) => newVersion((e.target as HTMLInputElement).files)} />
    </div>

    <hr class="sep" />
    <div class="row"><button class="btn danger" onclick={remove} disabled={busy}>Delete file</button></div>
    {#if error}<p class="error">{error}</p>{/if}
  {/if}
</div>
