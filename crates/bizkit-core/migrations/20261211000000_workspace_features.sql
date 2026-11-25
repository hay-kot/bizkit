-- Features (compiled-in modules) an admin turned off in a workspace. No row
-- means on, so existing workspaces and modules added later start on.
CREATE TABLE workspace_disabled_features (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  feature TEXT NOT NULL,
  PRIMARY KEY (workspace_id, feature)
) STRICT;
