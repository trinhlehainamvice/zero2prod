-- Wrap whole migration process inside a transaction to make process atomically, mean process can only be success or fail as a whole
BEGIN;
    -- Loop and backfill all empty values on `status` column to `confirmed`
    UPDATE subscriptions
        SET status = 'confirmed'
        WHERE status IS NULL;
    -- Change type of values in `status` column to mandatory aka `not null`
    ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;
COMMIT;