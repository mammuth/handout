# handout — design (v1)

Status: spec agreed 2026-11-08 via kickoff questionnaire. Terminology in `AGENTS.md` is normative.

## Goals

- Owners' agents publish HTML / Markdown / PDF / image files and hand out links.
- A recipient keeps one identity across files. Their sidebar lists every file they were invited to, with a *new* marker since they last opened it.
- Fully agent-driven: REST + MCP (Streamable HTTP). OAuth 2.1 for claude.ai / Claude Code, API keys for scripts.
- One static binary, SQLite + blob directory. Homelab deploy behind Caddy, no mTLS on these two hosts.

## Non-goals (v1, deliberately deferred)

- Outbound email / notifications — the agent notifies through its own channel with the recipient link.
- Viewer comments, approvals, e-sign. QR codes.
- Multi-file bundles (HTML + assets, zip). Single files only; relative subresources inside uploaded HTML are unsupported.
- Recipient groups / company-level sharing.

## Stack

| Layer | Choice |
|---|---|
| HTTP | axum 0.8, tokio, tower-http (trace, set-header, body limit) |
| DB | SQLite via sqlx (migrations embedded), WAL mode |
| Blobs | content-addressed files `{DATA_DIR}/blobs/{sha256[0..2]}/{sha256}` |
| MCP | rmcp (official Rust SDK), Streamable HTTP, stateless mode |
| Auth | argon2id passwords, sha256-hashed tokens, hand-rolled OAuth 2.1 AS (small surface) |
| Markdown | pulldown-cmark → wrapped in minimal styled HTML |
| Frontend | Svelte 5 + Vite + TypeScript SPA, built to `web/dist`, embedded with rust-embed; fonts bundled |
| CLI | clap: `serve`, `owner add|list`, `key add|list|revoke` |

## Hosts and origins

| Role | Production | Dev |
|---|---|---|
| shell, API, MCP, OAuth | `https://handout.example.com` | `http://localhost:8080` |
| content (raw files) | `https://handout-content.example.com` | `http://127.0.0.1:8080` |

One listener. Requests whose `Host` matches `CONTENT_URL` hit the content router; everything else the shell router. Dev uses `localhost` vs `127.0.0.1` as two origins on one port.

Why two hosts: sibling hostnames are distinct *origins* (JS, DOM, storage, service workers isolated) but the same *site*. Remaining same-site leaks and their fixes:

1. Cookie tossing (`Domain=example.com` set from content) → shell uses only `__Host-` prefixed cookies; content host sets none.
2. CSRF across subdomains (`SameSite` does not help) → owner API is bearer-only; viewer POSTs are cookie-authed but `Origin`-checked and only unlock/identify.
3. Brand/phishing under the owner's domain → accepted for v1; `CONTENT_URL` is a config var, swap to a throwaway domain later with no code change.

Defense in depth independent of hosts: iframe `sandbox="allow-scripts allow-forms allow-popups allow-downloads"` (no `allow-same-origin` → opaque origin), and every content response carries `Content-Security-Policy: sandbox allow-scripts allow-forms allow-popups allow-downloads; frame-ancestors {PUBLIC_ORIGIN}` plus `X-Content-Type-Options: nosniff`, so a raw URL opened top-level stays sandboxed.

## Data model

```
owners         id, slug UNIQUE, name, password_hash, created_at
api_keys       id, owner_id, name, key_hash UNIQUE, key_prefix, created_at, last_used_at, revoked_at
sessions       id (random), owner_id, created_at, expires_at                      -- owner web login
oauth_clients  client_id, client_name, redirect_uris (json), created_at
oauth_codes    code_hash, client_id, owner_id, redirect_uri, code_challenge, scope, resource, expires_at, used_at
oauth_tokens   token_hash, kind (access|refresh), client_id, owner_id, scope, expires_at, revoked_at, parent_hash
files          id, owner_id, public_id UNIQUE, slug, title, kind, access (link|recipients|off),
               password_hash NULL, email_gate (bool), expires_at NULL, created_at, updated_at, deleted_at NULL
file_versions  id, file_id, number, blob_sha256, size, mime, filename, created_at   UNIQUE(file_id, number)
recipients     id (uuid), owner_id, name, email NULL, company NULL, token_hash UNIQUE, token_prefix,
               created_at, last_seen_at NULL                                        UNIQUE(owner_id, email)
invites        file_id, recipient_id, created_at, last_opened_at NULL, last_opened_version NULL   PK(file_id, recipient_id)
opens          id, file_id, version_number, recipient_id NULL, ip_hash, user_agent, created_at,
               visible_seconds, active_seconds, max_scroll_pct, last_heartbeat_at NULL
```

Identifiers:

- file `public_id` = `{slug}-{token}`: slug derived from title (lowercase, `[a-z0-9-]`, max 40), token = 10 chars base58 (~58 bits). Stored plain (it is the lookup key).
- recipient `id` = uuid v4 (stable API id). Recipient token = 22 chars base58 (~128 bits), stored plain so `share_file` can always return a recipient link; rotatable.
- API key = `hk_` + 32 chars base58, same storage scheme.
- OAuth codes / tokens = 32 chars base58, stored hashed.
- Blob dedupe by sha256. History of live files is kept. Deleting a file removes every blob only its versions referenced, right away; a daily in-process sweep (1 h grace by mtime) removes orphans left by aborted uploads, crashes or cascading owner deletes, plus stale temp files.

## URL scheme

Shell host:

| Route | Purpose |
|---|---|
| `GET /` | minimal landing |
| `GET /s/{owner}/{file-id}` | viewer. `?r={token}` → validate, set recipient cookie, 302 to clean URL |
| `GET /s/{owner}` | recipient home (list of invites); 404 if anonymous |
| `GET /api/view/{owner}/{file-id}` | viewer API (cookies): meta, sidebar, gate state, signed content URL. Records an open |
| `POST /api/view/{owner}/{file-id}/unlock` | `{password}` → unlock cookie |
| `POST /api/view/{owner}/{file-id}/identify` | `{email, name?}` → find-or-create recipient, invite, recipient cookie |
| `POST /api/view/{owner}/{file-id}/heartbeat` | engagement deltas for an open (see Engagement) |
| `/api/*` | owner API, bearer |
| `/mcp` | MCP Streamable HTTP, bearer |
| `/login`, `/logout` | owner session (server-rendered, used by the OAuth flow) |
| `/auth/login`, `/auth/token`, `/auth/logout` | owner session for the SPA (JSON, Origin-checked) |
| `/admin`, `/admin/*` | owner web UI (SPA) |
| `/oauth/authorize`, `/oauth/token`, `/oauth/register` | OAuth 2.1 AS |
| `/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource` | metadata |
| `/assets/*` | embedded frontend |

Content host:

| Route | Purpose |
|---|---|
| `GET /{owner}/{file-id}?t={sig}[&v=N]` | raw bytes of latest (or version N), inline. Markdown rendered to HTML |
| `GET /{owner}/{file-id}/download?t={sig}[&v=N]` | `Content-Disposition: attachment` |

`t` = base64url(HMAC-SHA256(secret, `{file_id}|{version}|{exp}`)) + exp, 1 h validity. Minted by the viewer API only after access + gates pass.

## Viewer flow

1. `GET /s/max/proposal-7xKq2mNp4R?r=TOKEN`. Server validates the token belongs to a recipient of owner `max`; sets cookie `__Host-hr=TOKEN` (HttpOnly, Secure, SameSite=Lax, Path=/, 1 year), 303 → clean URL. Invalid token → dropped silently. Nothing is recorded here: link previewers and mail scanners fetch recipient links too. `last_seen_at` is set by the viewer API, which only a real page load calls.
2. SPA boots, calls `GET /api/view/max/proposal-7xKq2mNp4R`. Response:
   ```json
   { "owner": {"slug":"max","name":"Max"},
     "recipient": {"name":"Jon Doe"} | null,
     "file": {"id":"…","title":"Proposal","kind":"html","version":3,"updated_at":"…","filename":"proposal.html"},
     "state": "ok" | "password" | "email" | "expired" | "forbidden" | "off",
     "content_url": "http://127.0.0.1:8080/max/proposal-…?t=…" | null,
     "download_url": "…" | null,
     "sidebar": [ {"id":"…","title":"…","kind":"…","is_new":true,"updated_at":"…"} ] }
   ```
   `state: ok` records an open and sets `invites.last_opened_at` for the recipient.
3. Access rules: `off` → `off`; `expires_at` passed → `expired`; `recipients` without an invited recipient cookie → `forbidden`; password set and no valid unlock cookie → `password`; `email_gate` and anonymous → `email`; else `ok`.
4. `is_new` = invite has no `last_opened_version`, or the latest version number is higher.
5. Gate cookies: `__Host-hu_{file_public_id}` = HMAC(secret, file_id|password_hash) so a password change invalidates unlocks.

## Engagement (watch time)

- `GET /api/view/…` returns `open: {id, token}` (token = HMAC of the open id). The shell runs a tracker for that open: every second, if the tab is visible, `visible_seconds += 1`, and `active_seconds += 1` if there was input in the last 10 s (shell events or beacon messages). Max scroll depth comes from the beacon.
- Beacon: a `<script>` injected before `</body>` into HTML served inline (and into rendered Markdown). It listens to mousemove/scroll/key/pointer/touch/wheel inside the sandboxed iframe and posts `{handout:"activity", active, scroll}` to the parent once a second. Opaque origin, no cookies, no network — postMessage only. The shell accepts messages only from its own iframe's `contentWindow`.
- `POST /api/view/{owner}/{file}/heartbeat` `{open_id, open_token, visible_seconds, active_seconds, max_scroll_pct}` every 15 s, on `visibilitychange`→hidden and `pagehide` (via `sendBeacon`, so the body is parsed from raw bytes). Origin-checked; per-heartbeat deltas capped at 60 s, active ≤ visible; opens older than 24 h stop accepting.
- Roll-ups: `opens` rows carry the counters; file detail sums them and lists per-invite `opens/active_seconds/max_scroll_pct`; recipient detail does the same per file. Exposed over REST, MCP (`get_file`, `get_recipient`, `list_opens`) and the admin UI.
- Limits: PDFs and images cannot be instrumented (browser viewer) → visible time only, active time undercounted. Uploaded HTML carrying its own CSP meta tag may block the beacon → shell-side signals only. All of it is client-reported, so it is a signal, not an audit trail.

## Owner API (bearer)

```
GET    /api/me
POST   /api/files                 multipart: file, title?, slug?, access?, password?, email_gate?, expires_at?
                                  or JSON: {title, filename, content | content_base64, …}
GET    /api/files?q=&limit=
GET    /api/files/{id}            meta + versions + invites + open count
PATCH  /api/files/{id}            {title?, access?, password? (null clears), email_gate?, expires_at? (null clears)}
PUT    /api/files/{id}/content    new version (multipart or JSON as above)
DELETE /api/files/{id}            soft delete; links go `off`
GET    /api/files/{id}/opens
POST   /api/files/{id}/share      {recipient_id} | {name, email?, company?} → find-or-create + invite → {recipient, recipient_link}
DELETE /api/files/{id}/invites/{recipient_id}
GET    /api/recipients?q=
POST   /api/recipients            {name, email?, company?} → recipient + token (once)
GET    /api/recipients/{id}       + invited files
PATCH  /api/recipients/{id}
POST   /api/recipients/{id}/rotate-token
DELETE /api/recipients/{id}
GET    /api/opens?file_id=&recipient_id=&since=
GET    /api/keys  POST /api/keys  DELETE /api/keys/{id}
```

Lookup by email is owner-scoped and case-insensitive. `find` for `share` by name is exact-insensitive match first, then substring; ambiguity returns 409 with candidates.

## MCP tools

Same semantics as the API, one tool per verb. Text content (html/md) passed as `content`; binary as `content_base64`.

`whoami`, `upload_file`, `update_file`, `set_file`, `list_files`, `get_file`, `delete_file`,
`find_recipients`, `create_recipient`, `update_recipient`, `rotate_recipient_token`,
`share_file` (file + recipient_id | name/email → recipient link), `revoke_invite`, `list_opens`.

Tool results are JSON text. Links in results are absolute (`PUBLIC_URL`).

## Auth

- Bearer resolution: `hk_` prefix → api_keys; otherwise oauth_tokens (kind access, unexpired, not revoked). Both yield an owner.
- Owner web session: `__Host-hs` cookie, 30 days, used only by `/oauth/authorize` and `/login`.
- OAuth 2.1 AS (what claude.ai and Claude Code need):
  - `/.well-known/oauth-authorization-server`: issuer, authorization/token/registration endpoints, `code_challenge_methods_supported: [S256]`, `grant_types: [authorization_code, refresh_token]`, `token_endpoint_auth_methods_supported: [none]`, `scopes_supported: [handout]`.
  - `/.well-known/oauth-protected-resource`: `resource`, `authorization_servers`, `bearer_methods_supported: [header]`.
  - `POST /oauth/register` (RFC 7591, public clients): `client_name`, `redirect_uris` → `client_id`. Unauthenticated, rate-limited.
  - `GET /oauth/authorize`: requires session (else login form, then back); consent page "Allow {client_name} to act as {owner}?"; PKCE S256 mandatory; `resource` (RFC 8707) accepted and recorded; code 5 min, single use.
  - `POST /oauth/token`: `authorization_code` (+ verifier) → access 1 h + refresh 30 d; `refresh_token` rotates.
  - Unauthenticated `/mcp` and `/api/*` → 401 + `WWW-Authenticate: Bearer resource_metadata="{PUBLIC_URL}/.well-known/oauth-protected-resource"`.
- Owners are created by CLI only (`handout owner add`). Rate limits (in-memory): login, unlock, register.
- Owner web UI (`/admin`, same SPA bundle): the owner **session cookie** is the browser identity, created by `POST /auth/login` (JSON: slug+password, or just an API key — `hk_…` identifies its owner) or by the server-rendered `/login` form used in the OAuth flow. The SPA never sends cookies to `/api/*`; on boot it calls `POST /auth/token` (cookie + Origin check) and receives a 1 h access token for the first-party OAuth client `web` (same `oauth_tokens` table), held in memory and sent as a bearer like any agent. `POST /auth/logout` deletes the session and revokes the owner's `web` tokens. Net effect: one session concept (OAuth consent + admin), one bearer concept on the API, no CSRF surface on `/api/*`.

## Security checklist

- HTML only ever served from the content host; shell host returns JSON or its own templates.
- `__Host-` cookies in production; `Origin` header must equal `PUBLIC_ORIGIN` on viewer POSTs.
- IP hash = sha256(ip ‖ secret ‖ date). User agent truncated to 200 chars.
- Upload limit `MAX_UPLOAD_MB` (default 100) via body limit; multipart streamed to a temp file, hashed, moved into the blob dir.
- Blob dir is never listed or served directly; content-type comes from the version row, not the request.
- Every response on both hosts carries `X-Robots-Tag: noindex, nofollow, noarchive` and defaults to `Cache-Control: no-store`; assets, favicon and `robots.txt` opt into caching. `robots.txt` disallows owner, auth and agent paths only — file links stay fetchable, since a crawler only honors `noindex` on pages it may fetch.
- Markdown rendering escapes raw HTML blocks? No — raw HTML passes through (owner-authored); it is sandboxed anyway.

## Config

| Env | Default | Meaning |
|---|---|---|
| `HANDOUT_DATA_DIR` | `./data` | sqlite file, blobs, generated secret |
| `HANDOUT_LISTEN` | `0.0.0.0:8080` | bind address |
| `HANDOUT_PUBLIC_URL` | `http://localhost:8080` | shell origin, used in links and metadata |
| `HANDOUT_CONTENT_URL` | `http://127.0.0.1:8080` | content origin |
| `HANDOUT_SECRET` | generated into `DATA_DIR/secret` | HMAC key for signed URLs and cookies |
| `HANDOUT_MAX_UPLOAD_MB` | `100` | |
| `RUST_LOG` | `handout=info,tower_http=info` | |

## Deployment (homelab)

- New Debian LXC, binary at `/usr/local/bin/handout`, data at `/var/lib/handout`, `deploy/handout.service` (DynamicUser, ProtectSystem=strict, ReadWritePaths=/var/lib/handout), env file `/etc/handout.env`.
- Caddy: two site blocks without mTLS, `reverse_proxy handout.lan:8080` for both hostnames (`deploy/Caddyfile.snippet`).
- Build: `cargo build --release --target x86_64-unknown-linux-musl` (needs `musl-tools`) or build inside a Debian container matching the CT's glibc.
- Backups: back up the container regularly; `DATA_DIR` holds everything.

## Later

- Multi-file bundles: cookie hop on the content host so relative subresources resolve.
- Email notifications, QR, comments, recipient groups, admin UI.
