-- First-party OAuth client used by the owner web UI (/admin). Tokens for it are minted from an
-- owner session instead of the authorization-code flow.
INSERT OR IGNORE INTO oauth_clients (client_id, client_name, redirect_uris, created_at)
VALUES ('web', 'handout web UI', '[]', 0);
