-- Update 'admin' because I forgot the password
UPDATE
    users
SET
    password_hash = '$argon2id$v=19$m=19456,t=2,p=1$J2qXFJr8m6UJqdXNBwkehw$0unqSJNBpCV3emL65Wer7pDjnCosEWCJNDkQLAi6xmE'
WHERE
    username = 'admin';