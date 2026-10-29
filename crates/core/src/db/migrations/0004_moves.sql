-- Step 14, archiving, deleting and moving. A message on its way out of its folder stays in the
-- cache (the queued action refers to it) but no longer shows. Moves wait a moment before they
-- go to the server, so they can still be undone.
ALTER TABLE messages ADD COLUMN leaving INTEGER NOT NULL DEFAULT 0 CHECK (leaving IN (0, 1));
ALTER TABLE pending_actions ADD COLUMN due_at INTEGER;
