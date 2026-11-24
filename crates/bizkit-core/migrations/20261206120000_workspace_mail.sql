-- A workspace's own mail account. Credentials live sealed in `secrets`
-- (JSON sealed with the instance key, bound to the workspace ID), never in
-- plain columns.
CREATE TABLE workspace_mail (
  workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  provider TEXT NOT NULL CHECK (provider IN ('smtp', 'mailgun')),
  from_name TEXT NOT NULL,
  from_address TEXT NOT NULL,
  reply_to TEXT,
  smtp_host TEXT,
  smtp_port INTEGER,
  smtp_security TEXT CHECK (smtp_security IN ('tls', 'starttls', 'none')),
  smtp_username TEXT,
  mailgun_domain TEXT,
  mailgun_region TEXT CHECK (mailgun_region IN ('us', 'eu')),
  secrets BLOB NOT NULL,
  verified_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
