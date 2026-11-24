-- A public archive of a list's sent broadcasts, opt-in per list and per
-- broadcast. The archive URL names archive_key, not the list ID, and the
-- key stays the same when the archive is turned off and on.
ALTER TABLE mailing_lists ADD COLUMN archive INTEGER NOT NULL DEFAULT 0;
ALTER TABLE mailing_lists ADD COLUMN archive_key TEXT NOT NULL DEFAULT '';
UPDATE mailing_lists SET archive_key = lower(hex(randomblob(16)));
CREATE UNIQUE INDEX mailing_lists_archive_key ON mailing_lists (archive_key);

ALTER TABLE mailing_broadcasts ADD COLUMN archived INTEGER NOT NULL DEFAULT 0;
