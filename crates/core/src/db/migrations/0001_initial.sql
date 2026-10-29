-- Initial schema. Timestamps are Unix milliseconds, ids are UUIDv7 strings.

CREATE TABLE accounts (
  id             TEXT PRIMARY KEY,
  display_name   TEXT NOT NULL,
  email_address  TEXT NOT NULL,
  color          TEXT,
  sort_order     INTEGER NOT NULL DEFAULT 0,
  auth_method    TEXT NOT NULL CHECK (auth_method IN ('password', 'oauth2')),
  oauth_provider TEXT,
  imap_host      TEXT NOT NULL,
  imap_port      INTEGER NOT NULL CHECK (imap_port BETWEEN 1 AND 65535),
  imap_security  TEXT NOT NULL CHECK (imap_security IN ('tls', 'starttls')),
  imap_username  TEXT NOT NULL,
  -- SMTP is used from phase 4 on.
  smtp_host      TEXT,
  smtp_port      INTEGER CHECK (smtp_port BETWEEN 1 AND 65535),
  smtp_security  TEXT CHECK (smtp_security IN ('tls', 'starttls')),
  smtp_username  TEXT,
  created_at     INTEGER NOT NULL
);

CREATE TABLE folders (
  id             TEXT PRIMARY KEY,
  account_id     TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  name           TEXT NOT NULL,
  path           TEXT NOT NULL,
  delimiter      TEXT,
  -- SPECIAL-USE role, NULL for ordinary folders.
  role           TEXT CHECK (role IN ('inbox', 'sent', 'drafts', 'trash', 'archive', 'junk', 'all')),
  uid_validity   INTEGER,
  uid_next       INTEGER,
  highest_modseq INTEGER,
  unread_count   INTEGER NOT NULL DEFAULT 0,
  total_count    INTEGER NOT NULL DEFAULT 0,
  last_synced_at INTEGER,
  UNIQUE (account_id, path)
);

-- `seq` is the stable rowid the full-text index refers to; `id` is what the rest of the app
-- uses. A table without INTEGER PRIMARY KEY may renumber rowids on VACUUM.
CREATE TABLE messages (
  seq               INTEGER PRIMARY KEY,
  id                TEXT NOT NULL UNIQUE,
  account_id        TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  folder_id         TEXT NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
  uid               INTEGER NOT NULL,
  message_id        TEXT,
  in_reply_to       TEXT,
  references_header TEXT,
  subject           TEXT,
  from_name         TEXT,
  from_address      TEXT,
  to_addresses      TEXT, -- JSON
  cc_addresses      TEXT, -- JSON
  date_sent         INTEGER,
  date_received     INTEGER, -- IMAP INTERNALDATE
  size_bytes        INTEGER,
  snippet           TEXT,
  is_read           INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
  is_flagged        INTEGER NOT NULL DEFAULT 0 CHECK (is_flagged IN (0, 1)),
  has_attachments   INTEGER NOT NULL DEFAULT 0 CHECK (has_attachments IN (0, 1)),
  body_state        TEXT NOT NULL DEFAULT 'none' CHECK (body_state IN ('none', 'full')),
  body_text         TEXT,
  body_html         TEXT, -- already sanitized
  fetched_at        INTEGER NOT NULL,
  UNIQUE (folder_id, uid)
);

CREATE INDEX idx_messages_folder_date ON messages (folder_id, date_sent DESC);
CREATE INDEX idx_messages_folder_read ON messages (folder_id, is_read);
CREATE INDEX idx_messages_flagged ON messages (folder_id) WHERE is_flagged = 1;
CREATE INDEX idx_messages_message_id ON messages (message_id);
CREATE INDEX idx_messages_account ON messages (account_id);

CREATE TABLE attachments (
  id         TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
  part_id    TEXT NOT NULL, -- MIME part, to fetch it later
  filename   TEXT,
  mime_type  TEXT,
  size_bytes INTEGER,
  content_id TEXT,
  is_inline  INTEGER NOT NULL DEFAULT 0 CHECK (is_inline IN (0, 1)),
  local_path TEXT -- set once downloaded
);

CREATE INDEX idx_attachments_message ON attachments (message_id);

-- Local changes that still have to reach the server.
CREATE TABLE pending_actions (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  message_id TEXT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
  action     TEXT NOT NULL CHECK (action IN ('set_read', 'set_flagged', 'move', 'delete')),
  payload    TEXT, -- JSON
  attempts   INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);

CREATE INDEX idx_pending_actions_account ON pending_actions (account_id, id);

-- Full-text search over the message cache. External content: the text lives in `messages`
-- only, the triggers keep the index in step. `remove_diacritics 2` lets "muller" find "Müller".
CREATE VIRTUAL TABLE messages_fts USING fts5 (
  subject,
  from_name,
  from_address,
  body_text,
  content = 'messages',
  content_rowid = 'seq',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts (rowid, subject, from_name, from_address, body_text)
  VALUES (new.seq, new.subject, new.from_name, new.from_address, new.body_text);
END;

CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts (messages_fts, rowid, subject, from_name, from_address, body_text)
  VALUES ('delete', old.seq, old.subject, old.from_name, old.from_address, old.body_text);
END;

CREATE TRIGGER messages_fts_update AFTER UPDATE OF subject, from_name, from_address, body_text
ON messages BEGIN
  INSERT INTO messages_fts (messages_fts, rowid, subject, from_name, from_address, body_text)
  VALUES ('delete', old.seq, old.subject, old.from_name, old.from_address, old.body_text);
  INSERT INTO messages_fts (rowid, subject, from_name, from_address, body_text)
  VALUES (new.seq, new.subject, new.from_name, new.from_address, new.body_text);
END;
