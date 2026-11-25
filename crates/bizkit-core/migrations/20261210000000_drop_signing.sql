-- Doc signing left bizkit. Its tables go, children first, and so do its jobs,
-- which no handler would ever run. IF EXISTS: databases that never ran the
-- signing migrations have none of these.

DROP TABLE IF EXISTS signing_fields;
DROP TABLE IF EXISTS signing_events;
DROP TABLE IF EXISTS signing_signers;
DROP TABLE IF EXISTS signing_files;
DROP TABLE IF EXISTS signing_requests;
DROP TABLE IF EXISTS signing_seal;

DELETE FROM jobs WHERE kind LIKE 'signing.%';

-- Signing sent its mails through core's mail.send job. Queued ones would link
-- signers to /sign?t= and senders to a request's /signing/ page, both gone.
DELETE FROM jobs
WHERE kind = 'mail.send' AND status = 'queued'
  AND (payload LIKE '%/sign?t=%' OR payload LIKE '%/signing/%');
