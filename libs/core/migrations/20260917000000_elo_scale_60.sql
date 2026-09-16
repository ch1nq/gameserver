-- Rescale Weng-Lin ratings from raw OpenSkill to Elo scale (× 60).
--
-- For databases that already ran 20260916000000 with raw defaults
-- (25.0 ± 8.33): every stored value moves to Elo scale (1500 ± 500).
-- Fresh databases are unaffected (the base migration already creates
-- Elo-scale defaults, so these UPDATEs match zero rows).
--
-- The rescale is exactly consistent with the rating math: the Weng-Lin
-- update equations are homogeneous of degree 1 in (mu, sigma, beta), and
-- beta moved 25/6 → 250 alongside, so replaying history at Elo scale
-- reproduces these values.

UPDATE agent_ratings
SET rating = rating * 60.0,
    uncertainty = uncertainty * 60.0;

UPDATE match_placements
SET rating_before = rating_before * 60.0,
    uncertainty_before = uncertainty_before * 60.0,
    rating_after = rating_after * 60.0,
    uncertainty_after = uncertainty_after * 60.0;

ALTER TABLE agent_ratings
    ALTER COLUMN rating SET DEFAULT 1500.0,
    ALTER COLUMN uncertainty SET DEFAULT 500.0;
