-- Contact attributes: a flat JSON object of text, number and boolean values,
-- filled into mail as {{ attr.key }}.
ALTER TABLE mailing_contacts ADD COLUMN attributes TEXT NOT NULL DEFAULT '{}';

-- A broadcast targets a segment (tags, attribute conditions, join dates)
-- instead of one tag. DROP COLUMN rewrites the rows in place; rebuilding the
-- table would null mailing_deliveries.broadcast_id.
ALTER TABLE mailing_broadcasts ADD COLUMN segment TEXT NOT NULL DEFAULT '{}';
UPDATE mailing_broadcasts SET segment = json_object('tags', json_array(tag)) WHERE tag IS NOT NULL;
ALTER TABLE mailing_broadcasts DROP COLUMN tag;
