-- Events, rollups, people, aliases and conversions move to one SQLite file
-- per project (data_dir/analytics/<project_id>.db, migrations-project/).
-- The rows here are dropped, not copied: analytics shipped without real
-- installs to carry over. Site day markers reset so the rollup starts clean.
DROP TABLE analytics_conversions;
DROP TABLE analytics_aliases;
DROP TABLE analytics_events;
DROP TABLE analytics_daily;
DROP TABLE analytics_people;
UPDATE analytics_sites SET rolled_through = NULL, last_event_day = NULL;
