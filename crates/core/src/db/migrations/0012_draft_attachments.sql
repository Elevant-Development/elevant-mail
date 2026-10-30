-- Phase 4: files attached to a draft, with their bytes, so they survive a restart and go into the
-- draft's copy on the server. They go with the draft.
CREATE TABLE draft_attachments (
  id         TEXT PRIMARY KEY,
  draft_key  TEXT NOT NULL REFERENCES drafts (key) ON DELETE CASCADE,
  file_name  TEXT NOT NULL,
  mime_type  TEXT NOT NULL,
  size       INTEGER NOT NULL,
  data       BLOB NOT NULL,
  added_at   INTEGER NOT NULL
);
CREATE INDEX idx_draft_attachments_draft ON draft_attachments (draft_key);
