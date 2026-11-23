-- A script targets a set of entities of its hook point's kind, such as
-- forms A and C. No rows: the script runs for every entity of the kind.
-- The kind comes from the hook point, which is fixed after create.
CREATE TABLE extension_script_targets (
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  script_id TEXT NOT NULL REFERENCES extension_scripts (id) ON DELETE CASCADE,
  target_id TEXT NOT NULL,
  PRIMARY KEY (script_id, target_id)
) STRICT;

INSERT INTO extension_script_targets (workspace_id, script_id, target_id)
SELECT workspace_id, id, target_id FROM extension_scripts WHERE target_id IS NOT NULL;

-- The one-sync-script rule now spans two tables, so the write transaction
-- checks it (see `store::sync_conflict`).
DROP INDEX extension_scripts_one_sync;
ALTER TABLE extension_scripts DROP COLUMN target_id;
