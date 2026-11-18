-- AI agents may attach files from this computer to their drafts (agents.rs): per account, on top
-- of writing drafts, and only from the folders listed here. Off and empty until the user allows
-- it; switching it on in the app lists the downloads folder.
ALTER TABLE accounts ADD COLUMN agent_attach INTEGER NOT NULL DEFAULT 0
  CHECK (agent_attach IN (0, 1));
CREATE TABLE agent_folders (
  path     TEXT PRIMARY KEY,
  added_at INTEGER NOT NULL
);
