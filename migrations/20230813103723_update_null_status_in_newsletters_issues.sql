BEGIN;
-- issues delivery queue task and newsletters issue are created in the same transaction
-- If there are tasks in the queue related to a newsletter issue, assume that issue is in process
UPDATE newsletters_issues 
SET status = 'IN PROCESS'
WHERE status IS NULL AND id IN (SELECT id FROM newsletters_issues_delivery_queue);

-- Because all queries are in one transaction, don't need to check matching id in two tables again
-- Update remaining issues to published, because there aren't delivery tasks related to these issue
UPDATE newsletters_issues
SET status = 'PUBLISHED'
WHERE status IS NULL;

ALTER TABLE newsletters_issues ALTER COLUMN status SET NOT NULL;

COMMIT; 
