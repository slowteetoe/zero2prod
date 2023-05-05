-- rename password column
ALTER TABLE
    users RENAME password TO password_hash;