-- Add a default seeded user
INSERT INTO
    users (user_id, username, password_hash)
VALUES
    (
        'df30b81a-bc97-45f8-a9bb-0ce644625aad',
        'admin',
        '$argon2id$v=19$m=19456,t=2,p=1$YaN4DsesbG3ep9O+ulOZmg$x2n6UtlQpyUfby/DQ0p8AJzx/TGD743QHBbZe0/0x2I'
    )