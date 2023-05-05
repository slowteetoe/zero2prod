-- remove salt from users, we're using PHC string now
ALTER TABLE
    users DROP COLUMN salt;