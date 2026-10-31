-- Invitations in the reader: the message's iCalendar part (`text/calendar`), as sent, and what
-- the user answered.
ALTER TABLE messages ADD COLUMN calendar TEXT;

-- One answer per event (and occurrence, for an invitation to one date of a series), so it still
-- shows when the invitation comes again. `sequence` is the invitation's version: an answer to an
-- earlier one is out of date once the organizer changed the time or place.
CREATE TABLE invitation_replies (
  account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
  event_uid     TEXT NOT NULL,
  recurrence_id TEXT NOT NULL DEFAULT '', -- empty: the whole event or series
  answer        TEXT NOT NULL CHECK (answer IN ('accepted', 'tentative', 'declined')),
  sequence      INTEGER NOT NULL DEFAULT 0,
  replied_at    INTEGER NOT NULL,
  PRIMARY KEY (account_id, event_uid, recurrence_id)
);

-- Invitations cached before kept their calendar part only as an attachment: fetch those again
-- when opened.
UPDATE messages SET body_state = 'none'
WHERE body_state = 'full'
  AND id IN (SELECT message_id FROM attachments
             WHERE mime_type IN ('text/calendar', 'application/ics'));
