-- What an agent last wrote into a draft (agents.rs): its recipients, subject and the words of its
-- text. An agent changes the draft again only while it still reads so, never over the user's
-- changes. NULL for the user's own drafts.
ALTER TABLE drafts ADD COLUMN agent_snapshot TEXT;
