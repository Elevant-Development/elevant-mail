-- Phase 4: mail being written, kept across restarts. `message_id` names the draft's copy in the
-- server's drafts folder, which every upload replaces; `uploaded_to` is the account holding that
-- copy and `uploaded_at` the change it shows (NULL before the first upload).
CREATE TABLE drafts (
  key           TEXT PRIMARY KEY,
  account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  mode          TEXT NOT NULL CHECK (mode IN ('new', 'reply', 'replyAll', 'forward')),
  source_id     TEXT,
  to_addresses  TEXT NOT NULL,
  cc_addresses  TEXT NOT NULL,
  bcc_addresses TEXT NOT NULL,
  subject       TEXT NOT NULL,
  html          TEXT NOT NULL,
  text          TEXT NOT NULL,
  message_id    TEXT NOT NULL,
  updated_at    INTEGER NOT NULL,
  uploaded_to   TEXT,
  uploaded_at   INTEGER
);
