# AGENTS.md — handout

Self-hosted sharing server for agent-generated files (HTML / Markdown / PDF / images).
Rust (axum) serves the REST API, an MCP server, an OAuth 2.1 authorization server and the
embedded Svelte viewer. SQLite for metadata, content-addressed blobs on disk.

Full design: `docs/DESIGN.md`. Human quickstart: `README.md`.

## Terminology — use these words verbatim (code, DB, API, MCP tools, UI, docs)

| Term | Meaning |
|---|---|
| **owner** | Tenant / admin user. Has a slug used in URLs (`/s/{owner}/…`), a password (web login for OAuth consent) and API keys. Multi-tenant: many owners per instance. |
| **recipient** | Persistent viewer identity created by an owner: name, email, company, uuid, rotatable **recipient token**. One identity across all of that owner's files. |
| **anonymous** | A viewer with no recipient cookie. |
| **file** | One uploaded item. Unguessable public id `{slug}-{token}` (e.g. `proposal-7xKq2mNp4R`). Kinds: `html`, `markdown`, `pdf`, `image`, `other`. |
| **version** | A re-upload of the same file. Latest is served, history kept. |
| **file link** | `{PUBLIC_URL}/s/{owner}/{file-id}` — the deep link to one file. |
| **recipient link** | file link + `?r={recipient token}`. Auto-logs the recipient in (cookie) and redirects to the clean file link. What `share_file` returns. |
| **invite** | Stored grant `file × recipient`. Fills the recipient's sidebar and carries "last opened" for the *new* badge. |
| **access** | Per-file setting: `link` (anyone with the link, default) · `recipients` (invited recipients only) · `off` (revoked). |
| **gate** | Optional per-file checks on top of access: `password`, `email` (viewer enters email → matched or created as recipient), `expires_at`. |
| **open** | Tracked view of a file version: timestamp, recipient or anonymous, IP hash, user agent. |
| **content host** | Second hostname (`CONTENT_URL`) serving raw file bytes inside the sandboxed iframe. Never serves HTML on the shell origin. |
| **shell** | The Svelte viewer page on `PUBLIC_URL`: sidebar + gate forms + iframe. |

Do **not** use: "user" (ambiguous — say owner or recipient), "guest" / "viewer" (say recipient
or anonymous), "share" as a noun (it is the verb that creates an invite), "document" (say file),
"space" / "handout" for a bundle (there is no bundle entity; the sidebar is derived from invites).

## Layout

```
Cargo.toml            single crate `handout` (bin)
src/                  axum server, sqlx migrations in migrations/
web/                  Svelte 5 + Vite viewer, built to web/dist (embedded via rust-embed)
docs/DESIGN.md        spec and decisions
deploy/               systemd unit, Caddy snippet, env example
```

## Commands

```
cd web && npm ci && npm run build      # frontend first (binary embeds web/dist)
cargo run -- serve                     # dev server on :8080 (shell=localhost, content=127.0.0.1)
cargo run -- owner add max --name "Max"   # create owner, prompts password
cargo run -- key add max --name agent     # prints API key once
cargo test                              # integration tests (in-memory sqlite + temp blob dir)
cd web && npm run check                 # svelte-check
```

## Conventions

- Conventional Commits. Small, reviewable commits; list paths explicitly.
- Owner API = bearer only (API key or OAuth token). Viewer API = cookies only, read-mostly, Origin-checked on POST.
- Public ids / tokens: base58, generated with `rand`. API keys, OAuth codes/tokens and sessions are stored sha256-hashed. File public ids and recipient tokens are link capabilities and stored plain.
- Cookies: `__Host-` prefix when `PUBLIC_URL` is https; plain names in http dev.
- Every raw response on the content host carries `Content-Security-Policy: sandbox …; frame-ancestors {PUBLIC_ORIGIN}` and `X-Content-Type-Options: nosniff`.
- No external requests from the shell (fonts bundled).
