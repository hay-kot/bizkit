-- A script's saved test cases: a JSON array of {name, input, expect?}.
ALTER TABLE extension_scripts ADD COLUMN tests TEXT NOT NULL DEFAULT '[]';
