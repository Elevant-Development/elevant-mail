-- Which mail gets the account's signature by itself: new mail, replies, forwards and drafts AI
-- agents write. And in replies and forwards, whether it goes above the quote (right below the
-- text) or below it, at the end. Everything on, above the quote: as before.
ALTER TABLE accounts ADD COLUMN signature_new INTEGER NOT NULL DEFAULT 1
  CHECK (signature_new IN (0, 1));
ALTER TABLE accounts ADD COLUMN signature_replies INTEGER NOT NULL DEFAULT 1
  CHECK (signature_replies IN (0, 1));
ALTER TABLE accounts ADD COLUMN signature_forwards INTEGER NOT NULL DEFAULT 1
  CHECK (signature_forwards IN (0, 1));
ALTER TABLE accounts ADD COLUMN signature_agents INTEGER NOT NULL DEFAULT 1
  CHECK (signature_agents IN (0, 1));
ALTER TABLE accounts ADD COLUMN signature_below_quote INTEGER NOT NULL DEFAULT 0
  CHECK (signature_below_quote IN (0, 1));
