-- Calendars and their events. Events keep their iCalendar text as the source of truth; the
-- columns beside it are what the views need to find and show them without parsing.
CREATE TABLE calendars (
  id         TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts (id) ON DELETE CASCADE, -- NULL: only on this computer
  remote_id  TEXT,    -- the calendar on the server (CalDAV collection, Graph id), once synced
  name       TEXT NOT NULL, -- empty for the calendar on this computer: the interface names it
  color      TEXT NOT NULL,
  read_only  INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1)),
  visible    INTEGER NOT NULL DEFAULT 1 CHECK (visible IN (0, 1)),
  sync_token TEXT,
  position   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE events (
  id          TEXT PRIMARY KEY,
  calendar_id TEXT NOT NULL REFERENCES calendars (id) ON DELETE CASCADE,
  uid         TEXT NOT NULL,
  ics         TEXT NOT NULL,
  title       TEXT,
  location    TEXT,
  all_day     INTEGER NOT NULL DEFAULT 0 CHECK (all_day IN (0, 1)),
  -- Unix milliseconds of the first date; all-day events at midnight UTC of their days.
  starts_at   INTEGER NOT NULL,
  ends_at     INTEGER NOT NULL,
  -- A series: expanded from `ics` when a range is asked for.
  recurring   INTEGER NOT NULL DEFAULT 0 CHECK (recurring IN (0, 1)),
  remote_id   TEXT,   -- the event on the server (CalDAV href, Graph id), once synced
  etag        TEXT,
  -- A change made here that still has to reach the server.
  pending     TEXT CHECK (pending IN ('create', 'update', 'delete')),
  changed_at  INTEGER NOT NULL,
  UNIQUE (calendar_id, uid)
);

CREATE INDEX idx_events_time ON events (starts_at, ends_at);

-- Events made here go into this calendar until an account's calendars are synced.
INSERT INTO calendars (id, account_id, name, color) VALUES ('local', NULL, '', 'oklch(0.68 0.14 295)');
