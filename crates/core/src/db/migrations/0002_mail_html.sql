-- Step 8, HTML mail. `messages.body_html` now holds the original HTML of a message; it is
-- sanitized each time the message is shown (mail/sanitize.rs), so a stricter sanitizer also
-- covers mail that is already cached.

-- Small inline images (`cid:` in mail HTML), cached together with the body.
ALTER TABLE attachments ADD COLUMN data BLOB;

-- Bodies cached before had no HTML and no inline images: fetch them again when opened.
UPDATE messages SET body_state = 'none' WHERE body_state = 'full';
