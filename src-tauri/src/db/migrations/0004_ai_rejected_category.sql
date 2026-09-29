-- The AI label a user rejected. Moved out of `ai_category_id` so reads stop showing it,
-- and kept so the model can be told "not this" next time.
ALTER TABLE activities ADD COLUMN ai_rejected_category_id INTEGER REFERENCES categories(id);
