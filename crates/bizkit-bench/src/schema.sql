-- The analytics_events table and indexes from the analytics migration,
-- without the projects FK: a bench file has no projects table.
CREATE TABLE IF NOT EXISTS analytics_events (
  id INTEGER PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  ts INTEGER NOT NULL,
  day TEXT NOT NULL,
  name TEXT NOT NULL,
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
  visitor INTEGER NOT NULL,
  person TEXT NOT NULL,
  anon_id TEXT,
  user_id TEXT,
  props TEXT,
  source TEXT NOT NULL CHECK (source IN ('web', 'server'))
) STRICT;
CREATE INDEX IF NOT EXISTS analytics_events_day ON analytics_events (project_id, day, name);
CREATE INDEX IF NOT EXISTS analytics_events_person ON analytics_events (project_id, person, ts);
CREATE INDEX IF NOT EXISTS analytics_events_ts ON analytics_events (project_id, ts);

CREATE TABLE IF NOT EXISTS analytics_daily (
  project_id TEXT NOT NULL,
  day TEXT NOT NULL,
  dimension TEXT NOT NULL,
  value TEXT NOT NULL,
  visitors INTEGER NOT NULL,
  pageviews INTEGER NOT NULL,
  events INTEGER NOT NULL,
  PRIMARY KEY (project_id, day, dimension, value)
) STRICT, WITHOUT ROWID;
