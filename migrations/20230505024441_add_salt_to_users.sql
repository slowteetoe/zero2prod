-- add salt for password hash
ALTER TABLE
    users
ADD
    COLUMN salt TEXT NOT NULL;