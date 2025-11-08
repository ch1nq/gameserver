-- Add image_url field and simplify status enum
ALTER TABLE agents ADD COLUMN image_url TEXT;

-- Drop old status type and recreate with simplified values
ALTER TABLE agents ALTER COLUMN status TYPE text;
DROP TYPE IF EXISTS agent_status CASCADE;
CREATE TYPE agent_status AS ENUM (
    'active',
    'inactive'
);
ALTER TABLE agents ALTER COLUMN status TYPE agent_status USING status::agent_status;

-- Remove build_id as we no longer use build service
ALTER TABLE agents DROP COLUMN build_id;

-- Add constraint to ensure active agents have an image URL
ALTER TABLE agents ADD CONSTRAINT agents_active_has_image
    CHECK (status != 'active' OR image_url IS NOT NULL);

-- Add index for faster user agent lookups
CREATE INDEX IF NOT EXISTS idx_agents_user_id ON agents(user_id);
