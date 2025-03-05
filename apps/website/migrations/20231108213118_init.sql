-- Create users table.
create table if not exists users
(
    id bigserial primary key not null,
    username text not null unique,
    access_token text not null
);
