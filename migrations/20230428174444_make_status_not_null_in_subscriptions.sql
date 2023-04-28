-- Make 'status' not null in subscriptions
-- Now that the app code has been migrated and theoretically will never rollback too far, we can make the column not null
BEGIN;

-- Backfill 'status' for historical reasons
UPDATE
    subscriptions
SET
    status = 'confirmed'
WHERE
    status IS NULL;

-- Make status required
ALTER TABLE
    subscriptions
ALTER COLUMN
    status
SET
    NOT NULL;

COMMIT;