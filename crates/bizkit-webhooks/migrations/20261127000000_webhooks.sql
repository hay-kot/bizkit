-- Webhooks schema. Timestamps are unix milliseconds; IDs are hyphenated UUIDv7 text.

CREATE TABLE webhook_endpoints (
  id TEXT PRIMARY KEY NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  forward_url TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX webhook_endpoints_workspace ON webhook_endpoints (workspace_id, project_id);

CREATE TABLE webhook_requests (
  id TEXT PRIMARY KEY NOT NULL,          -- UUIDv7: id order is arrival order
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  endpoint_id TEXT NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
  method TEXT NOT NULL,
  query TEXT,
  headers TEXT NOT NULL,                 -- JSON array of [name, value]
  content_type TEXT,
  body BLOB NOT NULL,
  delivery TEXT,                         -- JSON Delivery, the latest one
  received_at INTEGER NOT NULL
) STRICT;
CREATE INDEX webhook_requests_list ON webhook_requests (workspace_id, endpoint_id, id);
CREATE INDEX webhook_requests_recent ON webhook_requests (endpoint_id, received_at);
