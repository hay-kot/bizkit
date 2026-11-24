-- A dashboard tile's width in columns of a 12-column grid: a third, a
-- half, two thirds or the full row. Rows grow to their tallest tile, so
-- there is no height.
ALTER TABLE analytics_dashboard_tiles
  ADD COLUMN width INTEGER NOT NULL DEFAULT 6 CHECK (width IN (4, 6, 8, 12));
