-- Mailing schema. Timestamps are unix milliseconds; IDs are hyphenated UUIDv7 text.

CREATE TABLE mailing_lists (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  description TEXT,
  double_opt_in INTEGER NOT NULL DEFAULT 1,
  footer TEXT,                           -- shown in every mail, such as a postal address
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX mailing_lists_workspace ON mailing_lists (workspace_id, project_id);

-- Forms whose submitters are added to a list. form_id has no foreign key:
-- forms is another module, and links to deleted forms are dropped on read.
CREATE TABLE mailing_list_forms (
  list_id TEXT NOT NULL REFERENCES mailing_lists(id) ON DELETE CASCADE,
  form_id TEXT NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  PRIMARY KEY (list_id, form_id)
) STRICT;
CREATE INDEX mailing_list_forms_form ON mailing_list_forms (workspace_id, form_id);

-- One row per address and list; consent is per list. An unsubscribed row
-- stays as the record that this address must not get mail.
CREATE TABLE mailing_contacts (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  list_id TEXT NOT NULL REFERENCES mailing_lists(id) ON DELETE CASCADE,
  email TEXT NOT NULL,                   -- lower case
  name TEXT,
  tags TEXT NOT NULL DEFAULT '[]',       -- JSON array of strings
  status TEXT NOT NULL CHECK (status IN ('pending', 'subscribed', 'unsubscribed')),
  source TEXT NOT NULL,                  -- api, admin, seed or form:<id>
  confirm_token TEXT UNIQUE,
  confirm_sent_at INTEGER,
  unsubscribe_token TEXT NOT NULL UNIQUE,
  subscribed_at INTEGER,
  unsubscribed_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE (list_id, email)
) STRICT;
CREATE INDEX mailing_contacts_list ON mailing_contacts (workspace_id, list_id, status, id);

CREATE TABLE mailing_broadcasts (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  list_id TEXT NOT NULL REFERENCES mailing_lists(id) ON DELETE CASCADE,
  subject TEXT NOT NULL,
  body_md TEXT NOT NULL,
  tag TEXT,                              -- only contacts with this tag
  status TEXT NOT NULL CHECK (status IN ('draft', 'sending', 'sent')),
  recipients INTEGER NOT NULL DEFAULT 0,
  sent_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX mailing_broadcasts_workspace ON mailing_broadcasts (workspace_id, list_id);

CREATE TABLE mailing_sequences (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  list_id TEXT NOT NULL REFERENCES mailing_lists(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  trigger TEXT NOT NULL CHECK (trigger IN ('on_join', 'manual')),
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX mailing_sequences_list ON mailing_sequences (workspace_id, list_id);

-- Step IDs stay stable across edits, so deliveries record which steps an
-- enrollment reached.
CREATE TABLE mailing_steps (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  sequence_id TEXT NOT NULL REFERENCES mailing_sequences(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  delay_hours INTEGER NOT NULL CHECK (delay_hours BETWEEN 0 AND 8760),
  subject TEXT NOT NULL,
  body_md TEXT NOT NULL,
  UNIQUE (sequence_id, position)
) STRICT;

CREATE TABLE mailing_enrollments (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  sequence_id TEXT NOT NULL REFERENCES mailing_sequences(id) ON DELETE CASCADE,
  contact_id TEXT NOT NULL REFERENCES mailing_contacts(id) ON DELETE CASCADE,
  status TEXT NOT NULL CHECK (status IN ('active', 'completed', 'exited', 'unsubscribed')),
  next_run_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE (sequence_id, contact_id)
) STRICT;
CREATE INDEX mailing_enrollments_contact ON mailing_enrollments (contact_id);

-- Every mail the module sends. The row is the send log and the job's input.
CREATE TABLE mailing_deliveries (
  id TEXT PRIMARY KEY NOT NULL,          -- UUIDv7: id order is creation order
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  list_id TEXT NOT NULL REFERENCES mailing_lists(id) ON DELETE CASCADE,
  contact_id TEXT REFERENCES mailing_contacts(id) ON DELETE SET NULL,  -- NULL for test sends
  kind TEXT NOT NULL CHECK (kind IN ('confirm', 'broadcast', 'step', 'test')),
  broadcast_id TEXT REFERENCES mailing_broadcasts(id) ON DELETE SET NULL,
  enrollment_id TEXT REFERENCES mailing_enrollments(id) ON DELETE SET NULL,
  step_id TEXT REFERENCES mailing_steps(id) ON DELETE SET NULL,
  to_email TEXT NOT NULL,
  subject TEXT NOT NULL,                 -- for the log; the body is rendered from the source row at send time
  status TEXT NOT NULL CHECK (status IN ('queued', 'sent', 'failed', 'skipped')),
  error TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  sent_at INTEGER,
  UNIQUE (broadcast_id, contact_id),
  UNIQUE (enrollment_id, step_id)
) STRICT;
CREATE INDEX mailing_deliveries_list ON mailing_deliveries (workspace_id, list_id, kind, created_at);
CREATE INDEX mailing_deliveries_broadcast ON mailing_deliveries (broadcast_id, id);
CREATE INDEX mailing_deliveries_contact ON mailing_deliveries (contact_id, id);
