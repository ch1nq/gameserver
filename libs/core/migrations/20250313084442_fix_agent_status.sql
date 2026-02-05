
-- create agent_status enum with proper commas
drop type if exists agent_status;
create type agent_status as enum (
    'created',
    'building',
    'build_failed',
    'active',
    'inactive'
);

-- use the enum type in the agents table
create table if not exists agents (
    id bigserial primary key not null,
    name text not null,
    status agent_status not null,
    user_id bigint not null,
    build_id text
);
