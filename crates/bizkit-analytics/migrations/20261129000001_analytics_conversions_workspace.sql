-- Conversions carry workspace_id like every module table, and identify
-- moves them to the person by key, so they need a person index.
CREATE TABLE analytics_conversions_new (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  goal_id TEXT NOT NULL REFERENCES analytics_goals(id) ON DELETE CASCADE,
  day TEXT NOT NULL,
  person TEXT NOT NULL,
  PRIMARY KEY (goal_id, day, person)
) STRICT, WITHOUT ROWID;

INSERT INTO analytics_conversions_new (workspace_id, goal_id, day, person)
SELECT g.workspace_id, c.goal_id, c.day, c.person
FROM analytics_conversions c JOIN analytics_goals g ON g.id = c.goal_id;

DROP TABLE analytics_conversions;
ALTER TABLE analytics_conversions_new RENAME TO analytics_conversions;
CREATE INDEX analytics_conversions_person ON analytics_conversions (workspace_id, person);
