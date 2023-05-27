-- Relax null checks so we can handle concurrent requests more gracefully
ALTER TABLE
    idempotency
ALTER COLUMN
    response_status_code DROP NOT NULL;

ALTER TABLE
    idempotency
ALTER COLUMN
    response_body DROP NOT NULL;

ALTER TABLE
    idempotency
ALTER COLUMN
    response_headers DROP NOT NULL;