-- What an agent needs from the mail server (agents.rs): the whole of a large mail, or the bytes of
-- an attachment. The MCP server has no credentials, so it leaves a request here; the running app
-- fetches it, stores it in the cache and marks the request done or failed.
CREATE TABLE agent_requests (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('body', 'attachment')),
  message_id TEXT NOT NULL,
  attachment_id TEXT,
  state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'done', 'failed')),
  error TEXT,
  created_at INTEGER NOT NULL,
  done_at INTEGER
);
CREATE INDEX agent_requests_open ON agent_requests (state, created_at);

-- Attachment bytes fetched for agents. Kept for a day, so reading a file twice does not fetch
-- it twice; the app's own attachment cache is a folder emptied at start and not shared.
CREATE TABLE agent_files (
  attachment_id TEXT PRIMARY KEY REFERENCES attachments (id) ON DELETE CASCADE,
  data BLOB NOT NULL,
  fetched_at INTEGER NOT NULL
);
