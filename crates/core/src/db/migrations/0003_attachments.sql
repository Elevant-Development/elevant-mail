-- Step 10, attachments. Bodies cached before only kept inline images, not the list of
-- attachments: fetch those that have any again when opened.
UPDATE messages SET body_state = 'none' WHERE body_state = 'full' AND has_attachments = 1;
