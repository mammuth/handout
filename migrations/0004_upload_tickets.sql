-- One-time upload URLs: minted by an authenticated owner (MCP/REST), redeemed by a plain PUT of the
-- bytes. The token is a short-lived credential, stored sha256-hashed.
CREATE TABLE upload_tickets (
  token_hash TEXT    PRIMARY KEY,
  owner_id   INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  file_id    INTEGER REFERENCES files(id) ON DELETE CASCADE, -- set: the PUT adds a version
  meta       TEXT    NOT NULL, -- JSON: title, slug, access, email_gate, expires_at, filename, mime
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  used_at    INTEGER
);
CREATE INDEX upload_tickets_expiry ON upload_tickets(expires_at);
