-- System tokens for internal services to act on behalf of users
-- These tokens are used by the website instance to authenticate registry operations
-- User context is passed separately (not in token)
create table registry_tokens_internal (
    id bigserial primary key not null,
    token_hash text not null unique,
    created_at timestamp not null default now(),
    expires_at timestamp not null default (now() + interval '15 minutes')
);

-- Fast lookups during Docker registry authentication
create index idx_registry_tokens_internal_token_hash on registry_tokens_internal(token_hash);
