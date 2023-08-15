BEGIN;
UPDATE newsletters_issues SET status = 'COMPLETED' WHERE status = 'PUBLISHED';
UPDATE newsletters_issues SET status = 'AVAILABLE' WHERE status = 'IN PROCESS';

-- Because all completed tasks in newsletters_issues_delivery_queue are deleted
-- We are no longer be able to keep track of the number of tasks
-- So just set finished_n_tasks and required_n_tasks to 0
UPDATE newsletters_issues
SET finished_n_tasks = 0, required_n_tasks = 0
WHERE status = 'COMPLETED';

-- Like above, we can't keep track of the number of deleted tasks
-- We set required_n_tasks to remaining tasks in newsletters_issues_delivery_queue
-- And set finished_n_tasks to 0
UPDATE newsletters_issues
SET finished_n_tasks = 0, required_n_tasks = (SELECT COUNT(*) FROM newsletters_issues_delivery_queue WHERE id = newsletters_issues.id)
WHERE status = 'AVAILABLE';

ALTER TABLE newsletters_issues ALTER COLUMN required_n_tasks SET NOT NULL;
ALTER TABLE newsletters_issues ALTER COLUMN finished_n_tasks SET NOT NULL;
COMMIT;