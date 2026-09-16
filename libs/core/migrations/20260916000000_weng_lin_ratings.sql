-- Weng-Lin (OpenSkill) ratings for issue #66.
--
-- Raw scale: new players start at rating=25.0, uncertainty=25/3. ratings live
-- in agent_ratings (one row per agent, created on first finished match);
-- matches + match_placements keep the full history with before/after
-- snapshots so ratings can be audited or recomputed. Failed games are not
-- recorded here at all (only Finished games move ratings).

CREATE TABLE matches (
    id          BIGSERIAL PRIMARY KEY NOT NULL,
    external_id TEXT NOT NULL UNIQUE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    num_agents  INT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'finished'
);

CREATE TABLE agent_ratings (
    agent_id      BIGINT NOT NULL PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
    rating        DOUBLE PRECISION NOT NULL DEFAULT 25.0,
    uncertainty   DOUBLE PRECISION NOT NULL DEFAULT 8.333333333333334,
    matches_played INT NOT NULL DEFAULT 0,
    wins          INT NOT NULL DEFAULT 0,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE match_placements (
    match_id           BIGINT NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    agent_id           BIGINT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    position           INT NOT NULL,
    score              INT NOT NULL,
    rating_before      DOUBLE PRECISION NOT NULL,
    uncertainty_before DOUBLE PRECISION NOT NULL,
    rating_after       DOUBLE PRECISION NOT NULL,
    uncertainty_after  DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (match_id, agent_id)
);

CREATE INDEX idx_match_placements_agent_id ON match_placements(agent_id);
CREATE INDEX idx_matches_external_id ON matches(external_id);
