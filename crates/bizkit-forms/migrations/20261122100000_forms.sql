-- Forms schema. Timestamps are unix milliseconds; IDs are hyphenated UUIDv7 text.

CREATE TABLE forms (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  fields TEXT NOT NULL,                  -- JSON array of FieldDef
  bot_check TEXT NOT NULL CHECK (bot_check IN ('honeypot', 'challenge')),
  min_fill_ms INTEGER NOT NULL,
  redirect_url TEXT,
  notify_emails TEXT NOT NULL,           -- JSON array of normalized emails
  challenge_key TEXT NOT NULL,           -- HMAC key for challenges; never returned
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX forms_workspace ON forms (workspace_id, project_id);

CREATE TABLE form_submissions (
  id TEXT PRIMARY KEY NOT NULL,          -- UUIDv7, so id order is arrival order
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  form_id TEXT NOT NULL REFERENCES forms(id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK (status IN ('inbox', 'spam')),
  spam_reason TEXT,
  data TEXT NOT NULL,                    -- JSON object: field name -> string or array of strings
  user_agent TEXT,
  referer TEXT,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX form_submissions_list ON form_submissions (workspace_id, form_id, status, id);
CREATE INDEX form_submissions_recent ON form_submissions (form_id, created_at);

-- Solved challenges, so each one is accepted once. Pruned as they expire.
CREATE TABLE form_used_challenges (
  challenge TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  form_id TEXT NOT NULL REFERENCES forms(id) ON DELETE CASCADE,
  expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX form_used_challenges_expiry ON form_used_challenges (workspace_id, expires_at);
