-- Database schema owned by apps/website
-- apps/registry-auth has read-only access to this table for token validation

create table registry_tokens (
    id bigserial primary key not null,
    user_id bigint not null references users(id) on delete cascade,
    token_hash text not null,
    name text not null,
    created_at timestamp not null default now(),
    revoked_at timestamp
);

-- Fast lookups during Docker registry authentication
create index idx_registry_tokens_token_hash on registry_tokens(token_hash);

-- List tokens for a user
create index idx_registry_tokens_user_id on registry_tokens(user_id);

-- Only allow non-revoked tokens to be used
create index idx_registry_tokens_active on registry_tokens(user_id, revoked_at) where revoked_at is null;
