-- A signature pasted as HTML (logo, table, colours), kept as cleaned on saving; 0: one written in
-- the editor.
ALTER TABLE accounts ADD COLUMN signature_html INTEGER NOT NULL DEFAULT 0
  CHECK (signature_html IN (0, 1));
