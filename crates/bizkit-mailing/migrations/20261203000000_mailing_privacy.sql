-- Privacy: hashed erasure records and the manage-link key.

-- Addresses erased on request. Only a hash stays: the SHA-256 hex of the
-- workspace ID, a NUL and the lower-case address, so the same address hashes
-- differently in each workspace. Sends and adds check it, so an erased
-- address is never mailed or added again.
CREATE TABLE mailing_erasures (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  email_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (workspace_id, email_hash)
) STRICT;

-- The key that signs manage-subscriptions links, one per workspace and
-- created on first use. It never leaves the database; deleting the row
-- revokes every link of the workspace.
CREATE TABLE mailing_link_keys (
  workspace_id TEXT PRIMARY KEY NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  key TEXT NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;

-- Export and erase find an address's deliveries; to_email is lower case.
CREATE INDEX mailing_deliveries_to ON mailing_deliveries (workspace_id, to_email);
