-- Fields on the PDF's pages, signing-order groups and adopted initials.

-- Signers sign in groups: the lowest pending order gets its links first,
-- and equal orders sign in parallel.
ALTER TABLE signing_signers ADD COLUMN signing_order INTEGER NOT NULL DEFAULT 1
  CHECK (signing_order BETWEEN 1 AND 10);
-- The adopted initials as Signature JSON, like `signature`.
ALTER TABLE signing_signers ADD COLUMN initials TEXT;
-- Set at create. NULL only on rows from before this migration; fields.put
-- fills it from the file.
ALTER TABLE signing_requests ADD COLUMN page_count INTEGER;

-- A rect is fractions of the page's visible box as displayed, top-left origin.
CREATE TABLE signing_fields (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  request_id TEXT NOT NULL REFERENCES signing_requests(id) ON DELETE CASCADE,
  signer_id TEXT NOT NULL REFERENCES signing_signers(id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('signature', 'initials', 'date', 'text', 'checkbox')),
  page INTEGER NOT NULL CHECK (page >= 1),
  x REAL NOT NULL CHECK (x >= 0 AND x < 1),
  y REAL NOT NULL CHECK (y >= 0 AND y < 1),
  width REAL NOT NULL CHECK (width > 0 AND x + width <= 1),
  height REAL NOT NULL CHECK (height > 0 AND y + height <= 1),
  required INTEGER NOT NULL DEFAULT 1,
  label TEXT NOT NULL DEFAULT '',
  -- FieldValue JSON once filled.
  value TEXT,
  filled_at INTEGER,
  UNIQUE (request_id, position)
) STRICT;
CREATE INDEX signing_fields_signer ON signing_fields (signer_id);
