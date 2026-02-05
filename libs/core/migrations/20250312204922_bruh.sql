-- Create users table.
create table if not exists users
(
    id bigserial primary key not null,
    username text not null unique,
    access_token text not null
);

create table if not exists agents (
    id bigserial primary key not null,
    name text not null,
    status agent_status not null,
    user_id bigint not null,
    build_id text
);
