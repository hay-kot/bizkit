-- Secret values are sealed with the instance key (secrets_key_env). Rows
-- from before stay plaintext in value until the app starts with a key and
-- seals them; nothing can seal them here, because SQL never sees the key.
CREATE TABLE extension_secrets_sealed (
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  value TEXT,
  sealed BLOB,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (project_id, name),
  CHECK ((value IS NULL) != (sealed IS NULL))
) STRICT;

INSERT INTO extension_secrets_sealed (workspace_id, project_id, name, value, updated_at)
SELECT workspace_id, project_id, name, value, updated_at FROM extension_secrets;

DROP TABLE extension_secrets;
ALTER TABLE extension_secrets_sealed RENAME TO extension_secrets;
