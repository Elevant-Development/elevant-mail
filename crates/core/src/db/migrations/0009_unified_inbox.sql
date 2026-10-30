-- Whether the account's inbox is part of the unified inbox ("All inboxes"); settings → Reading.
ALTER TABLE accounts ADD COLUMN in_unified_inbox INTEGER NOT NULL DEFAULT 1
  CHECK (in_unified_inbox IN (0, 1));
