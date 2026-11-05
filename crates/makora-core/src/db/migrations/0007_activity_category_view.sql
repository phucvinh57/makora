-- The category an activity is filed under, resolved once for every read model: the user's pin
-- (`activities.category_id`) wins over the model's suggestion (an `ai_labels` row the user has
-- not rejected). Activities with neither are absent, so joining this view with a LEFT JOIN
-- leaves them uncategorized. `confidence` is the model's, and only present for `source = 'ai'`.
-- This is the SQL form of the precedence rules in categorization::domain::classification.
CREATE VIEW activity_category AS
SELECT a.id AS activity_id,
       COALESCE(a.category_id, l.category_id) AS category_id,
       CASE WHEN a.category_id IS NOT NULL THEN 'manual' ELSE 'ai' END AS source,
       CASE WHEN a.category_id IS NULL THEN l.confidence END AS confidence
FROM activities a
LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
WHERE a.category_id IS NOT NULL OR l.activity_id IS NOT NULL;
