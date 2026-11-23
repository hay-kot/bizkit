-- Each ctx.mail.send, kept so the per-script hourly cap survives job
-- cleanup.
CREATE TABLE extension_mail_sends (
  id INTEGER PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  script_id TEXT NOT NULL REFERENCES extension_scripts (id) ON DELETE CASCADE,
  sent_at INTEGER NOT NULL
) STRICT;

CREATE INDEX extension_mail_sends_recent ON extension_mail_sends (workspace_id, script_id, sent_at);
