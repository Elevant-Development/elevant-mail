-- What AI agents may do per account (agents.rs), as bits: 1 read, 2 mark read and starred,
-- 4 move and archive, 8 into the trash, 16 write drafts; nothing without reading. Accounts shared
-- before keep reading. `agent_access` from 0015 is no longer read.
ALTER TABLE accounts ADD COLUMN agent_permissions INTEGER NOT NULL DEFAULT 0
  CHECK (agent_permissions BETWEEN 0 AND 31);
UPDATE accounts SET agent_permissions = 1 WHERE agent_access = 1;

-- An agent changed mail of this account: the app takes the change to the server.
ALTER TABLE agent_log ADD COLUMN account_id TEXT;
ALTER TABLE agent_log ADD COLUMN changed INTEGER NOT NULL DEFAULT 0 CHECK (changed IN (0, 1));
