BEGIN;
ALTER TABLE newsletters_issues RENAME n_tasks TO finished_n_tasks;
ALTER TABLE newsletters_issues ADD COLUMN required_n_tasks INT NULL;
COMMIT;