-- How sure the model was of `ai_category_id`, from 0 to 1. NULL for labels made before it was
-- recorded; the review queue treats those as uncertain.
ALTER TABLE activities ADD COLUMN ai_confidence REAL;
