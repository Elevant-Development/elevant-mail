-- Step 18. The list pages through a folder newest first (mailbox::list_messages). This index
-- holds that order, so a page starts where the last one ended instead of sorting the folder.
CREATE INDEX idx_messages_folder_list
  ON messages (folder_id, coalesce(date_sent, date_received, fetched_at) DESC, uid DESC, id DESC);

-- Ordered by date_sent alone, which nothing reads any more.
DROP INDEX idx_messages_folder_date;
