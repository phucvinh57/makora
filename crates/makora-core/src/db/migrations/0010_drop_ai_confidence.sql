-- The model's confidence is no longer kept: it drops out of `ai_labels` and the view. The view
-- reads the column, so it is dropped first and recreated without it.
DROP VIEW activity_category;
ALTER TABLE ai_labels DROP COLUMN confidence;
CREATE VIEW activity_category AS
SELECT a.id AS activity_id,
       COALESCE(a.category_id, r.category_id, l.category_id) AS category_id,
       CASE WHEN a.category_id IS NOT NULL THEN 'manual'
            WHEN r.id IS NOT NULL THEN 'rule'
            ELSE 'ai' END AS source,
       CASE WHEN a.category_id IS NULL THEN r.id END AS rule_id
FROM activities a
LEFT JOIN rule_matches m ON m.activity_id = a.id
LEFT JOIN category_rules r ON r.id = m.rule_id
LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
WHERE a.category_id IS NOT NULL OR r.id IS NOT NULL OR l.activity_id IS NOT NULL;
