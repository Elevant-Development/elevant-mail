-- Calendars of mail accounts over CalDAV, with the account's password.
-- Where the account's calendars are (the calendar home), once found or set by the user.
ALTER TABLE accounts ADD COLUMN caldav_url TEXT;
-- Calendars are synced unless the user switched them off.
ALTER TABLE accounts ADD COLUMN caldav_enabled INTEGER NOT NULL DEFAULT 1
  CHECK (caldav_enabled IN (0, 1));
-- When looking for them last found nothing: the app looks again a day later.
ALTER TABLE accounts ADD COLUMN caldav_checked_at INTEGER;
