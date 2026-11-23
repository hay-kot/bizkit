-- Analytics schema. Timestamps are unix milliseconds; days are 'YYYY-MM-DD'
-- in UTC; entity IDs are hyphenated UUIDv7 text. Every table goes with its
-- project.

CREATE TABLE analytics_sites (
  project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  site_key TEXT NOT NULL UNIQUE,         -- public, in the snippet tag
  hostnames TEXT NOT NULL DEFAULT '[]',  -- JSON array; empty means any host
  identity_mode TEXT NOT NULL DEFAULT 'cookieless'
    CHECK (identity_mode IN ('cookieless', 'after_identify', 'always')),
  last_event_day TEXT,
  rolled_through TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX analytics_sites_workspace ON analytics_sites (workspace_id);

-- Append-only and the largest table, so it keys on rowid and carries no
-- workspace FK: the project FK already removes its rows.
CREATE TABLE analytics_events (
  id INTEGER PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  ts INTEGER NOT NULL,                   -- server receive time
  day TEXT NOT NULL,
  name TEXT NOT NULL,                    -- 'pageview' or a custom event name
  path TEXT,
  host TEXT,
  referrer_host TEXT,
  utm_source TEXT,
  utm_medium TEXT,
  utm_campaign TEXT,
  country TEXT,
  device TEXT,
  browser TEXT,
  os TEXT,
  visitor INTEGER NOT NULL,              -- daily salted hash
  person TEXT NOT NULL,                  -- 'p:<person id>', 'a:<anon id>' or 'v:<day>:<visitor>'
  anon_id TEXT,
  user_id TEXT,
  props TEXT,                            -- JSON object
  source TEXT NOT NULL CHECK (source IN ('web', 'server'))
) STRICT;
CREATE INDEX analytics_events_day ON analytics_events (project_id, day, name);
CREATE INDEX analytics_events_person ON analytics_events (project_id, person, ts);
CREATE INDEX analytics_events_ts ON analytics_events (project_id, ts);

CREATE TABLE analytics_daily (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  dimension TEXT NOT NULL,               -- 'total' has value ''
  day TEXT NOT NULL,
  value TEXT NOT NULL,
  visitors INTEGER NOT NULL,
  pageviews INTEGER NOT NULL,
  events INTEGER NOT NULL,
  PRIMARY KEY (project_id, dimension, day, value)
) STRICT, WITHOUT ROWID;

-- One salt per UTC day, shared by every site; older ones are deleted.
CREATE TABLE analytics_salts (
  day TEXT PRIMARY KEY NOT NULL,
  salt BLOB NOT NULL
) STRICT;

CREATE TABLE analytics_people (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL,
  traits TEXT NOT NULL DEFAULT '{}',     -- JSON object
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  UNIQUE (project_id, user_id)
) STRICT;
CREATE INDEX analytics_people_seen ON analytics_people (workspace_id, project_id, last_seen);

CREATE TABLE analytics_aliases (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  anon_id TEXT NOT NULL,
  person_id TEXT NOT NULL REFERENCES analytics_people(id) ON DELETE CASCADE,
  PRIMARY KEY (project_id, anon_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX analytics_aliases_person ON analytics_aliases (person_id);

CREATE TABLE analytics_goals (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('page', 'event')),
  match TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (project_id, kind, match)
) STRICT;
CREATE INDEX analytics_goals_workspace ON analytics_goals (workspace_id, project_id);

-- A row per goal, UTC day and person key: the first conversion fires the hook.
CREATE TABLE analytics_conversions (
  goal_id TEXT NOT NULL REFERENCES analytics_goals(id) ON DELETE CASCADE,
  day TEXT NOT NULL,
  person TEXT NOT NULL,
  PRIMARY KEY (goal_id, day, person)
) STRICT, WITHOUT ROWID;

CREATE TABLE analytics_funnels (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  steps TEXT NOT NULL,                   -- JSON [{kind, match}], 2 to 8 steps
  window_days INTEGER NOT NULL DEFAULT 7 CHECK (window_days BETWEEN 1 AND 90),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX analytics_funnels_workspace ON analytics_funnels (workspace_id, project_id);
