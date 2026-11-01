-- Where an account's CardDAV address books are, once found; when looking found nothing (looked
-- for again a day later). Switched on and off with the calendars (`caldav_enabled`).
ALTER TABLE accounts ADD COLUMN carddav_url TEXT;
ALTER TABLE accounts ADD COLUMN carddav_checked_at INTEGER;
