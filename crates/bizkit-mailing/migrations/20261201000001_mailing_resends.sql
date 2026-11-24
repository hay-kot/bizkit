-- How often "Resend to failed" put a delivery back in the queue, so the send
-- log shows the retry. `attempts` counts the attempts of the current send
-- only, because each resend starts a new job.
ALTER TABLE mailing_deliveries ADD COLUMN resends INTEGER NOT NULL DEFAULT 0;
