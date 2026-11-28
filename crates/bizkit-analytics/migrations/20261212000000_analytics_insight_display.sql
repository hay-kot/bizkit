-- How a saved insight draws its result. `auto` follows the result's view,
-- so insights saved before this column draw as they did.
ALTER TABLE analytics_insights
  ADD COLUMN display TEXT NOT NULL DEFAULT 'auto'
  CHECK (display IN ('auto', 'line', 'bar', 'number'));
