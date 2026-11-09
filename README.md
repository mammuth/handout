# handout

Share agent-generated files (HTML, Markdown, PDF, images) with people, via links.
Give a recipient one identity; every file you later share with them shows up in their sidebar.
Everything is driven by agents over MCP or REST.

- One binary: API + MCP (Streamable HTTP) + OAuth 2.1 + embedded viewer
- SQLite + blob directory, no other services
- Raw files served from a separate hostname inside a sandboxed iframe
- Opens, watch time (visible / active seconds) and scroll depth per recipient

Terminology and conventions: [AGENTS.md](AGENTS.md). Design: [docs/DESIGN.md](docs/DESIGN.md).

## Quickstart (dev)

```sh
cd web && npm ci && npm run build && cd ..
cargo run -- owner add max --name "Max"       # prompts for a password
cargo run -- key add max --name agent          # prints hk_… once
cargo run -- serve                             # http://localhost:8080
```

Upload and share:

```sh
curl -H "Authorization: Bearer hk_…" -F title="Proposal" -F file=@proposal.html \
  http://localhost:8080/api/files
# → { "file_id": "proposal-7xKq2mNp4R", "file_link": "http://localhost:8080/s/max/proposal-7xKq2mNp4R" }

curl -H "Authorization: Bearer hk_…" -H 'content-type: application/json' \
  -d '{"name":"Jon Doe","email":"jon@acme.com"}' \
  http://localhost:8080/api/files/proposal-7xKq2mNp4R/share
# → { "recipient_link": "http://localhost:8080/s/max/proposal-7xKq2mNp4R?r=…" }
```

## Owner web UI

`{PUBLIC_URL}/admin` — sign in with your owner slug + password, or paste an API key. Upload files, share them (get recipient links), manage recipients, see who opened what, create API keys.

## MCP

Endpoint: `{PUBLIC_URL}/mcp` (Streamable HTTP). Auth: OAuth (claude.ai, Claude Code `claude mcp add --transport http handout https://…/mcp`) or `Authorization: Bearer hk_…`.

Tools: `upload_file`, `update_file`, `set_file`, `list_files`, `get_file`, `delete_file`,
`find_recipients`, `create_recipient`, `share_file`, `revoke_invite`, `list_opens`, `whoami`.
