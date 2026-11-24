-- Signature requests: a PDF, its signers, the audit trail and the sealed result.

CREATE TABLE signing_requests (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  message TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('draft', 'sent', 'completed', 'voided', 'failed')),
  file_name TEXT NOT NULL,
  original_sha256 TEXT NOT NULL,
  signed_sha256 TEXT,
  created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
  -- Who sent it, kept for the audit page after the account goes.
  sender TEXT NOT NULL,
  -- Why the seal failed, while status is failed.
  failure TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  sent_at INTEGER,
  completed_at INTEGER,
  voided_at INTEGER
) STRICT;
CREATE INDEX signing_requests_project ON signing_requests (workspace_id, project_id, created_at);

-- The PDFs themselves, in the database so they go with their request.
CREATE TABLE signing_files (
  request_id TEXT NOT NULL REFERENCES signing_requests(id) ON DELETE CASCADE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('original', 'signed')),
  bytes BLOB NOT NULL,
  PRIMARY KEY (request_id, kind)
) STRICT;

CREATE TABLE signing_signers (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL REFERENCES signing_requests(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  name TEXT NOT NULL,
  email TEXT NOT NULL,
  -- SHA-256 of the link token; set when the request is sent.
  token_hash TEXT UNIQUE,
  -- SHA-256 of the sealed PDF's download link token; set on completion.
  download_token_hash TEXT UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('pending', 'signed')),
  -- The signature as JSON: drawn strokes or a typed name.
  signature TEXT,
  signed_at INTEGER,
  UNIQUE (request_id, position)
) STRICT;

-- The audit trail. Rows are only ever added.
CREATE TABLE signing_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL REFERENCES signing_requests(id) ON DELETE CASCADE,
  signer_id TEXT REFERENCES signing_signers(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('created', 'sent', 'viewed', 'signed', 'completed', 'voided', 'failed')),
  at INTEGER NOT NULL,
  ip TEXT,
  user_agent TEXT,
  detail TEXT
) STRICT;
CREATE INDEX signing_events_request ON signing_events (request_id, id);

-- The instance's seal: a P-256 key and its self-signed certificate. One per
-- instance, so no workspace_id.
CREATE TABLE signing_seal (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  certificate BLOB NOT NULL,
  private_key BLOB NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
