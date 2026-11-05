-- User-written categorization rules. `category_rules` holds what the person wrote (`value` is
-- stored normalized: lowercase, no scheme or `www.`); `rule_matches` holds, per activity, the one
-- rule that wins for it, the same way `ai_labels` holds the model's suggestion. It is rewritten
-- in Rust whenever a rule or an activity changes (categorization::rules_store), so reads never
-- evaluate a rule themselves. Both foreign keys cascade: deleting a rule or an activity takes its
-- matches with it.
CREATE TABLE category_rules (
    id          INTEGER PRIMARY KEY,
    category_id INTEGER NOT NULL REFERENCES categories(id),
    kind        TEXT NOT NULL CHECK(kind IN ('app','site','page','pattern')),
    field       TEXT NOT NULL DEFAULT '' CHECK(field IN ('','url','title','app')),
    value       TEXT NOT NULL,
    UNIQUE(kind, field, value)
);
CREATE INDEX idx_category_rules_category ON category_rules(category_id);

CREATE TABLE rule_matches (
    activity_id INTEGER PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE,
    rule_id     INTEGER NOT NULL REFERENCES category_rules(id) ON DELETE CASCADE
);
CREATE INDEX idx_rule_matches_rule ON rule_matches(rule_id);

-- The resolved category is now: the user's pin, else a rule's, else the model's unrejected
-- suggestion. `rule_id` is only present for `source = 'rule'`, and `confidence` only for 'ai'.
-- This is the SQL form of the precedence rules in categorization::domain::classification.
DROP VIEW activity_category;
CREATE VIEW activity_category AS
SELECT a.id AS activity_id,
       COALESCE(a.category_id, r.category_id, l.category_id) AS category_id,
       CASE WHEN a.category_id IS NOT NULL THEN 'manual'
            WHEN r.id IS NOT NULL THEN 'rule'
            ELSE 'ai' END AS source,
       CASE WHEN a.category_id IS NULL AND r.id IS NULL THEN l.confidence END AS confidence,
       CASE WHEN a.category_id IS NULL THEN r.id END AS rule_id
FROM activities a
LEFT JOIN rule_matches m ON m.activity_id = a.id
LEFT JOIN category_rules r ON r.id = m.rule_id
LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
WHERE a.category_id IS NOT NULL OR r.id IS NOT NULL OR l.activity_id IS NOT NULL;
