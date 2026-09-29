-- Category suggested by the local LLM. Kept apart from the manual pin (`category_id`) so a
-- pin always wins and AI output can be wiped without touching the user's own choices.
ALTER TABLE activities ADD COLUMN ai_category_id INTEGER REFERENCES categories(id);
CREATE INDEX idx_activities_ai_category ON activities(ai_category_id);
