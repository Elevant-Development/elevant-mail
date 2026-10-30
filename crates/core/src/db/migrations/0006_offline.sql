-- Step 17. How far back each account keeps mail in the cache, in days; NULL keeps everything.
-- Every folder keeps at least its newest 100 messages anyway (mail::sync).
ALTER TABLE accounts ADD COLUMN keep_days INTEGER DEFAULT 90 CHECK (keep_days > 0);

-- Mail fetched by a search on the server, older than what the sync keeps. It shows in search
-- results and conversations, but not in its folder's list until the sync reaches it.
ALTER TABLE messages ADD COLUMN by_search INTEGER NOT NULL DEFAULT 0 CHECK (by_search IN (0, 1));
