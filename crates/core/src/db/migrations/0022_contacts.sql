-- Address books and their contacts. A contact keeps its vCard as the source of truth; the columns
-- beside it are what the list and search need without parsing.
CREATE TABLE address_books (
  id         TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts (id) ON DELETE CASCADE, -- NULL: only on this computer
  remote_id  TEXT,    -- the address book on the server (CardDAV collection, Graph folder)
  name       TEXT NOT NULL, -- empty for the address book on this computer: the interface names it
  read_only  INTEGER NOT NULL DEFAULT 0 CHECK (read_only IN (0, 1)),
  sync_token TEXT,
  position   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE contacts (
  id              TEXT PRIMARY KEY,
  address_book_id TEXT NOT NULL REFERENCES address_books (id) ON DELETE CASCADE,
  uid             TEXT NOT NULL,
  vcard           TEXT NOT NULL,
  name            TEXT NOT NULL, -- as lists show it
  company         TEXT,
  email           TEXT,          -- the first address, under the name in lists
  -- Name, company, addresses and numbers in lower case without accents, for sorting and search.
  sort_key        TEXT NOT NULL,
  search          TEXT NOT NULL,
  remote_id       TEXT,          -- the contact on the server (CardDAV href, Graph id), once synced
  etag            TEXT,
  -- A change made here that still has to reach the server.
  pending         TEXT CHECK (pending IN ('create', 'update', 'delete')),
  changed_at      INTEGER NOT NULL,
  UNIQUE (address_book_id, uid)
);

CREATE INDEX idx_contacts_sort ON contacts (sort_key);

-- Every mail address of a contact, in lower case: suggestions and "is this a contact".
CREATE TABLE contact_addresses (
  contact_id TEXT NOT NULL REFERENCES contacts (id) ON DELETE CASCADE,
  address    TEXT NOT NULL,
  position   INTEGER NOT NULL,
  PRIMARY KEY (contact_id, position)
);

CREATE INDEX idx_contact_addresses ON contact_addresses (address);

-- Contacts made here go into this address book until an account's address books are synced.
INSERT INTO address_books (id, account_id, name) VALUES ('local', NULL, '');
