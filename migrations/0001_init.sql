-- handout v1 schema. Timestamps are unix seconds (INTEGER).

CREATE TABLE owners (
  id            INTEGER PRIMARY KEY,
  slug          TEXT    NOT NULL UNIQUE,
  name          TEXT    NOT NULL,
  password_hash TEXT    NOT NULL,
  created_at    INTEGER NOT NULL
);

CREATE TABLE api_keys (
  id           INTEGER PRIMARY KEY,
  owner_id     INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,
  key_hash     TEXT    NOT NULL UNIQUE,
  key_prefix   TEXT    NOT NULL,
  created_at   INTEGER NOT NULL,
  last_used_at INTEGER,
  revoked_at   INTEGER
);

-- owner web login (OAuth consent only)
CREATE TABLE sessions (
  id         TEXT    PRIMARY KEY,
  owner_id   INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);

CREATE TABLE oauth_clients (
  client_id     TEXT    PRIMARY KEY,
  client_name   TEXT    NOT NULL,
  redirect_uris TEXT    NOT NULL, -- JSON array of strings
  created_at    INTEGER NOT NULL
);

CREATE TABLE oauth_codes (
  code_hash      TEXT    PRIMARY KEY,
  client_id      TEXT    NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
  owner_id       INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  redirect_uri   TEXT    NOT NULL,
  code_challenge TEXT    NOT NULL,
  scope          TEXT    NOT NULL,
  resource       TEXT,
  expires_at     INTEGER NOT NULL,
  used_at        INTEGER
);

CREATE TABLE oauth_tokens (
  token_hash  TEXT    PRIMARY KEY,
  kind        TEXT    NOT NULL CHECK (kind IN ('access', 'refresh')),
  client_id   TEXT    NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
  owner_id    INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  scope       TEXT    NOT NULL,
  expires_at  INTEGER NOT NULL,
  revoked_at  INTEGER,
  parent_hash TEXT
);

CREATE TABLE files (
  id            INTEGER PRIMARY KEY,
  owner_id      INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  public_id     TEXT    NOT NULL UNIQUE,
  slug          TEXT    NOT NULL,
  title         TEXT    NOT NULL,
  kind          TEXT    NOT NULL CHECK (kind IN ('html', 'markdown', 'pdf', 'image', 'other')),
  access        TEXT    NOT NULL DEFAULT 'link' CHECK (access IN ('link', 'recipients', 'off')),
  password_hash TEXT,
  email_gate    INTEGER NOT NULL DEFAULT 0,
  expires_at    INTEGER,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  deleted_at    INTEGER
);
CREATE INDEX files_owner ON files(owner_id, deleted_at, updated_at);

CREATE TABLE file_versions (
  id          INTEGER PRIMARY KEY,
  file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  number      INTEGER NOT NULL,
  blob_sha256 TEXT    NOT NULL,
  size        INTEGER NOT NULL,
  mime        TEXT    NOT NULL,
  filename    TEXT    NOT NULL,
  created_at  INTEGER NOT NULL,
  UNIQUE (file_id, number)
);
CREATE INDEX file_versions_blob ON file_versions(blob_sha256);

CREATE TABLE recipients (
  id           TEXT    PRIMARY KEY, -- uuid v4
  owner_id     INTEGER NOT NULL REFERENCES owners(id) ON DELETE CASCADE,
  name         TEXT    NOT NULL,
  email        TEXT,                -- stored lowercased
  company      TEXT,
  token        TEXT    NOT NULL UNIQUE, -- link capability, stored plain (rotatable)
  created_at   INTEGER NOT NULL,
  last_seen_at INTEGER,
  UNIQUE (owner_id, email)
);

CREATE TABLE invites (
  file_id        INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  recipient_id   TEXT    NOT NULL REFERENCES recipients(id) ON DELETE CASCADE,
  created_at          INTEGER NOT NULL,
  last_opened_at      INTEGER,
  last_opened_version INTEGER,
  PRIMARY KEY (file_id, recipient_id)
);
CREATE INDEX invites_recipient ON invites(recipient_id);

CREATE TABLE opens (
  id             INTEGER PRIMARY KEY,
  file_id        INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  version_number INTEGER NOT NULL,
  recipient_id   TEXT    REFERENCES recipients(id) ON DELETE SET NULL,
  ip_hash        TEXT    NOT NULL,
  user_agent     TEXT    NOT NULL,
  created_at     INTEGER NOT NULL
);
CREATE INDEX opens_file ON opens(file_id, created_at);
CREATE INDEX opens_recipient ON opens(recipient_id, created_at);
