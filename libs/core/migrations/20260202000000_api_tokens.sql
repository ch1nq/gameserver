CREATE TABLE api_tokens (
    id         BIGSERIAL PRIMARY KEY NOT NULL,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    name       TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT now(),
    revoked_at TIMESTAMP
);

CREATE INDEX idx_api_tokens_user_id ON api_tokens(user_id);
CREATE INDEX idx_api_tokens_active ON api_tokens(user_id, revoked_at) WHERE revoked_at IS NULL;
