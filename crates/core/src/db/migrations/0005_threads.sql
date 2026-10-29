-- Step 16, conversations (crate::threads). Messages that answer each other share a thread id;
-- `thread_subject` is the subject without "Re:" and the like, for replies that lack the headers.
ALTER TABLE messages ADD COLUMN thread_id TEXT;
ALTER TABLE messages ADD COLUMN thread_subject TEXT;

CREATE INDEX idx_messages_thread ON messages (account_id, thread_id);
CREATE INDEX idx_messages_thread_subject ON messages (account_id, thread_subject)
  WHERE thread_subject IS NOT NULL;

-- Every Message-ID header a conversation has seen (`key`): its messages' own and the ones they
-- answer. A reply finds its conversation here even before the mail it answers is in the cache.
CREATE TABLE thread_keys (
  account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  key        TEXT NOT NULL,
  thread_id  TEXT NOT NULL,
  PRIMARY KEY (account_id, key)
) WITHOUT ROWID;

CREATE INDEX idx_thread_keys_thread ON thread_keys (account_id, thread_id);
