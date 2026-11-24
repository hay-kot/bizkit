-- Saved insights and dashboards. Config like funnels, so it lives in the
-- app database; the numbers come from the project file at read time.

CREATE TABLE analytics_insights (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  query TEXT NOT NULL,                   -- JSON insight query
  range TEXT NOT NULL DEFAULT '30d' CHECK (range IN ('today', '7d', '30d', '90d', 'month', 'last-month')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX analytics_insights_workspace ON analytics_insights (workspace_id, project_id);

CREATE TABLE analytics_dashboards (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX analytics_dashboards_workspace ON analytics_dashboards (workspace_id, project_id);

-- Deleting an insight takes it off every dashboard.
CREATE TABLE analytics_dashboard_tiles (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  dashboard_id TEXT NOT NULL REFERENCES analytics_dashboards(id) ON DELETE CASCADE,
  insight_id TEXT NOT NULL REFERENCES analytics_insights(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  PRIMARY KEY (dashboard_id, insight_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX analytics_dashboard_tiles_insight ON analytics_dashboard_tiles (insight_id);
