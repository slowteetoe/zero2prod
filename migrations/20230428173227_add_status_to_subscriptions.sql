-- Add status column for subscriptions
ALTER TABLE
    subscriptions
ADD
    COLUMN status TEXT NULL;