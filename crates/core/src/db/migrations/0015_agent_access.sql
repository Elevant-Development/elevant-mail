-- AI agents reading mail over MCP (agents.rs): off until switched on, and then only the accounts
-- the user ticked. Every access is logged, the newest 1000 kept.
CREATE TABLE agent_settings (
  id      INTEGER PRIMARY KEY CHECK (id = 1),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1))
);
INSERT INTO agent_settings (id) VALUES (1);

ALTER TABLE accounts ADD COLUMN agent_access INTEGER NOT NULL DEFAULT 0
  CHECK (agent_access IN (0, 1));

CREATE TABLE agent_log (
  seq     INTEGER PRIMARY KEY,
  at      INTEGER NOT NULL,
  client  TEXT,
  tool    TEXT NOT NULL,
  detail  TEXT NOT NULL,
  results INTEGER NOT NULL,
  refused INTEGER NOT NULL DEFAULT 0 CHECK (refused IN (0, 1))
);
