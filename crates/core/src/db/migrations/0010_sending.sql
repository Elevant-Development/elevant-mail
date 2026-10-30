-- Phase 4, sending. The name mail goes out under; NULL until the user sets one, then the name on
-- the account's own mail in Sent is offered.
ALTER TABLE accounts ADD COLUMN sender_name TEXT;

-- Where the sender wants replies to go (mailing lists, newsletters), JSON like to_addresses.
-- Mail cached before this is answered to its sender.
ALTER TABLE messages ADD COLUMN reply_to TEXT;
