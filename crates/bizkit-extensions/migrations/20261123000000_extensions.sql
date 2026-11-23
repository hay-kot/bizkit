-- Scripts attached to hook points, per project.
CREATE TABLE extension_scripts (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  -- HookPoint.id; fixed after create.
  hook TEXT NOT NULL,
  -- Copied from the point at create, for the one-sync-script index.
  sync INTEGER NOT NULL,
  name TEXT NOT NULL,
  source TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX extension_scripts_one_sync ON extension_scripts (project_id, hook) WHERE sync = 1;
CREATE INDEX extension_scripts_lookup ON extension_scripts (workspace_id, project_id, hook);

-- Values scripts on async points read as ctx.secrets. Never returned by the API.
CREATE TABLE extension_secrets (
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (project_id, name)
) STRICT;

-- The newest 100 runs of each script, with secrets redacted.
CREATE TABLE extension_runs (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
  script_id TEXT NOT NULL REFERENCES extension_scripts (id) ON DELETE CASCADE,
  cause TEXT NOT NULL CHECK (cause IN ('sync', 'event', 'test')),
  status TEXT NOT NULL CHECK (status IN ('ok', 'error', 'timeout', 'overloaded', 'invalid_output')),
  attempt INTEGER,
  duration_ms INTEGER NOT NULL,
  error TEXT,
  logs TEXT NOT NULL,
  output TEXT,
  created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX extension_runs_script ON extension_runs (script_id, id);
