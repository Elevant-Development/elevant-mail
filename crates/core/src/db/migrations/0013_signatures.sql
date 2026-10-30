-- Phase 4: each account's signature, as HTML from the editor; NULL or empty: none.
ALTER TABLE accounts ADD COLUMN signature TEXT;
