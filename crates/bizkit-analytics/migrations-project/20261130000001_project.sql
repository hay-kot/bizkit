-- One project's analytics, in its own file. Timestamps are unix
-- milliseconds; days are 'YYYY-MM-DD' in UTC; entity IDs are hyphenated
-- UUIDv7 text. Sites, goals and funnels stay in the app database, so
-- nothing here references them: project deletion removes the whole file,
-- and goal deletion removes its conversions here.
--
-- Versions share one _sqlx_migrations table with the app migrations in the
-- sqlx prepare database, so they must stay unique across both sets.

-- Append-only and the largest table, so it keys on rowid.
CREATE TABLE analytics_events (
  id INTEGER PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
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
  project_id TEXT NOT NULL,
  dimension TEXT NOT NULL,               -- 'total' has value ''
  day TEXT NOT NULL,
  value TEXT NOT NULL,
  visitors INTEGER NOT NULL,
  pageviews INTEGER NOT NULL,
  events INTEGER NOT NULL,
  PRIMARY KEY (project_id, dimension, day, value)
) STRICT, WITHOUT ROWID;

CREATE TABLE analytics_people (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  traits TEXT NOT NULL DEFAULT '{}',     -- JSON object
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  UNIQUE (project_id, user_id)
) STRICT;
CREATE INDEX analytics_people_seen ON analytics_people (workspace_id, project_id, last_seen);

CREATE TABLE analytics_aliases (
  workspace_id TEXT NOT NULL,
  project_id TEXT NOT NULL,
  anon_id TEXT NOT NULL,
  person_id TEXT NOT NULL REFERENCES analytics_people(id) ON DELETE CASCADE,
  PRIMARY KEY (project_id, anon_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX analytics_aliases_person ON analytics_aliases (person_id);

-- A row per goal, UTC day and person key: the first conversion fires the hook.
CREATE TABLE analytics_conversions (
  workspace_id TEXT NOT NULL,
  goal_id TEXT NOT NULL,
  day TEXT NOT NULL,
  person TEXT NOT NULL,
  PRIMARY KEY (goal_id, day, person)
) STRICT, WITHOUT ROWID;
CREATE INDEX analytics_conversions_person ON analytics_conversions (workspace_id, person);
