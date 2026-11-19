-- Drafts agents threw away (agents.rs, discard_draft): gone here at once, their copy in the
-- drafts folder still to go into the account's trash, which the running app does. `message_id`
-- is the copy's Message-ID header, with angle brackets like drafts.message_id.
CREATE TABLE agent_discarded_drafts (
  account_id   TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  message_id   TEXT NOT NULL,
  discarded_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, message_id)
);
