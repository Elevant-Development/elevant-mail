-- A colour chosen for a calendar; NULL shows it in its account's colour (the one on this
-- computer in its own).
ALTER TABLE calendars ADD COLUMN own_color TEXT;
