-- Scripts can bind to one entity of their hook point's target kind, such as
-- one form. NULL: the script is project-wide and runs for every entity.
-- The kind comes from the hook point, which is fixed after create.
ALTER TABLE extension_scripts ADD COLUMN target_id TEXT;

-- One sync script per point for each entity, plus one project-wide.
DROP INDEX extension_scripts_one_sync;
CREATE UNIQUE INDEX extension_scripts_one_sync
  ON extension_scripts (project_id, hook, COALESCE(target_id, '')) WHERE sync = 1;
