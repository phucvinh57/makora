-- Rule suggestions the user dismissed. A suggestion is a matcher (`kind`, `field`, `value`, stored
-- normalized as in `category_rules`) and the category it would file under; a dismissed pair is
-- never suggested again, while the same matcher for another category still is. Deleting the
-- category takes its dismissals with it.
CREATE TABLE rule_suggestion_dismissals (
    kind         TEXT NOT NULL CHECK(kind IN ('app','site','page','pattern')),
    field        TEXT NOT NULL DEFAULT '' CHECK(field IN ('','url','title','app')),
    value        TEXT NOT NULL,
    category_id  INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    dismissed_at TEXT NOT NULL,
    PRIMARY KEY (kind, field, value, category_id)
);
CREATE INDEX idx_rule_suggestion_dismissals_category ON rule_suggestion_dismissals(category_id);
