//! Hand-written to match `db/migrations/*.sql` (the tables after migration 7). There is no
//! `diesel print-schema` step: this crate does not keep a `DATABASE_URL` around to generate
//! against, and the table set is small enough to maintain by hand. The `activity_category` view
//! is not declared here: only the raw-SQL read models in [`crate::reporting`] use it.

diesel::table! {
    categories (id) {
        id -> BigInt,
        name -> Text,
        color -> Text,
        parent_id -> Nullable<BigInt>,
        description -> Nullable<Text>,
    }
}

diesel::table! {
    sources (id) {
        id -> BigInt,
        kind -> Text,
        key -> Text,
        name -> Text,
    }
}

diesel::table! {
    activities (id) {
        id -> BigInt,
        source_id -> BigInt,
        path -> Text,
        title -> Text,
        category_id -> Nullable<BigInt>,
    }
}

diesel::table! {
    sessions (id) {
        id -> BigInt,
        activity_id -> BigInt,
        start_time -> Text,
        end_time -> Nullable<Text>,
    }
}

diesel::table! {
    ai_labels (activity_id) {
        activity_id -> BigInt,
        category_id -> BigInt,
        confidence -> Nullable<Double>,
        rejected -> Bool,
    }
}

diesel::joinable!(activities -> sources (source_id));
diesel::joinable!(activities -> categories (category_id));
diesel::joinable!(sessions -> activities (activity_id));
diesel::joinable!(ai_labels -> activities (activity_id));
diesel::joinable!(ai_labels -> categories (category_id));

diesel::allow_tables_to_appear_in_same_query!(activities, ai_labels, categories, sessions, sources,);
