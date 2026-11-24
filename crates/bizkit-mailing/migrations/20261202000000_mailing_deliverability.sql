-- Deliverability: provider IDs, bounce and complaint events, the
-- workspace-wide suppression list, the broadcast breaker and scheduling.
--
-- Additive only, one ADD COLUMN per column. Rebuilding mailing_broadcasts
-- to widen its status CHECK would null every mailing_deliveries.broadcast_id
-- through ON DELETE SET NULL, so the API derives `scheduled` (a draft with
-- send_at) and `paused` (paused_at set) instead.

-- The provider's message ID (Mailgun's id, or the Message-ID bizkit set).
ALTER TABLE mailing_deliveries ADD COLUMN provider_id TEXT;
-- What the provider reported after the send; status stays `sent`.
ALTER TABLE mailing_deliveries ADD COLUMN event TEXT CHECK (event IN ('bounced', 'complained'));
ALTER TABLE mailing_deliveries ADD COLUMN event_at INTEGER;
-- Set when a send found its broadcast paused. A parked row has no live
-- job; Resume queues it again.
ALTER TABLE mailing_deliveries ADD COLUMN parked_at INTEGER;
CREATE INDEX mailing_deliveries_provider ON mailing_deliveries (provider_id);

ALTER TABLE mailing_broadcasts ADD COLUMN send_at INTEGER;
ALTER TABLE mailing_broadcasts ADD COLUMN paused_at INTEGER;
ALTER TABLE mailing_broadcasts ADD COLUMN pause_reason TEXT;
-- Deliveries that failed in a row since the last success; the breaker
-- pauses the broadcast at a fixed limit.
ALTER TABLE mailing_broadcasts ADD COLUMN failures_in_row INTEGER NOT NULL DEFAULT 0;

-- Addresses no list in the workspace mails, whatever their contact rows say.
CREATE TABLE mailing_suppressions (
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  email TEXT NOT NULL,                   -- lower case
  reason TEXT NOT NULL CHECK (reason IN ('bounce', 'complaint', 'manual')),
  delivery_id TEXT,                      -- the delivery that bounced; no FK, the log may be pruned
  note TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (workspace_id, email)
) STRICT;

-- Mailgun webhook tokens seen in the last 7 days, to refuse replays. No
-- workspace_id: a token arrives before bizkit knows the workspace, and the
-- table holds nothing a tenant owns.
CREATE TABLE mailing_webhook_tokens (
  token TEXT PRIMARY KEY NOT NULL,
  seen_at INTEGER NOT NULL
) STRICT;
CREATE INDEX mailing_webhook_tokens_seen ON mailing_webhook_tokens (seen_at);
