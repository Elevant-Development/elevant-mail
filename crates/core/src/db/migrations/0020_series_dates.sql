-- Changing one date of a series. The iCalendar text holds the change (a changed date as its own
-- component with RECURRENCE-ID, a left-out one as EXDATE), which is all CalDAV needs. Microsoft
-- Graph changes such dates one by one, so they are listed here until sent.
CREATE TABLE pending_instances (
  event_id TEXT NOT NULL REFERENCES events (id) ON DELETE CASCADE,
  -- The date's original start (its RECURRENCE-ID), Unix milliseconds.
  instance INTEGER NOT NULL,
  kind     TEXT NOT NULL CHECK (kind IN ('update', 'delete')),
  PRIMARY KEY (event_id, instance)
);

-- What a change of a series touched, for Graph: the series' own details (1) or only some dates
-- (0), and whether its rule changed. Graph drops a series' changed dates when its rule is sent,
-- so the rule goes only when it changed.
ALTER TABLE events ADD COLUMN send_series INTEGER NOT NULL DEFAULT 1 CHECK (send_series IN (0, 1));
ALTER TABLE events ADD COLUMN send_rule INTEGER NOT NULL DEFAULT 0 CHECK (send_rule IN (0, 1));
