-- Phase 3 settings. Senders whose mail shows its external images when opened ("always load
-- images from …"), by address in lower case.
CREATE TABLE image_senders (
  address  TEXT PRIMARY KEY,
  added_at INTEGER NOT NULL
) WITHOUT ROWID;
