-- Add migration script here
INSERT INTO users (user_id, username, password_hash)
VALUES ('6249a16a-cbe3-4714-bdcd-9331bed572b1',
        'admin',
        '$argon2d$v=19$m=15000,t=2,p=1$j7/zqxD+gvw2KitKyTHsfQ$rOa3IV0Bi4JqocHNqsaOVwxSKJpVaNw45pkyXMcRCk8'); 