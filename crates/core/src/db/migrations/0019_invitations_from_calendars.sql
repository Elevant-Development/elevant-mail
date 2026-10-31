-- Events with attendees: the calendar's server sends the invitations, changes and cancellations
-- (CalDAV scheduling, RFC 6638, or Exchange). Calendars whose server does not get no attendees.
-- NULL: not asked yet; the next sync asks.
ALTER TABLE calendars ADD COLUMN can_invite INTEGER CHECK (can_invite IN (0, 1));
