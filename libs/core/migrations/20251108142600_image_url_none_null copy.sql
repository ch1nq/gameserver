-- make image_url non-nullable
ALTER TABLE agents ALTER COLUMN image_url SET NOT NULL;
