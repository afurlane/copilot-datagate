//! Policy-filterable database schema model and PostgreSQL loader.

use sqlx::postgres::PgPool;
use sqlx::FromRow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaLoaderError {
    #[error("unable to load database schema metadata")]
    Query(#[source] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaCatalog {
    pub tables: Vec<TableInfo>,
    pub views: Vec<ViewInfo>,
    pub indexes: Vec<IndexInfo>,
    pub triggers: Vec<TriggerInfo>,
    pub routines: Vec<RoutineInfo>,
    pub sequences: Vec<SequenceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewInfo {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ColumnInfo {
    pub schema: String,
    pub relation: String,
    pub name: String,
    pub data_type: String,
    pub ordinal_position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct IndexInfo {
    pub schema: String,
    pub name: String,
    pub relation: String,
    pub is_unique: bool,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TriggerInfo {
    pub schema: String,
    pub name: String,
    pub relation: String,
    pub event_manipulation: String,
    pub action_timing: String,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct RoutineInfo {
    pub schema: String,
    pub name: String,
    pub routine_type: String,
    pub data_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SequenceInfo {
    pub schema: String,
    pub name: String,
    pub data_type: String,
}

impl SchemaCatalog {
    pub async fn load(pool: &PgPool) -> Result<Self, SchemaLoaderError> {
        let tables = sqlx::query_as::<_, RelationRow>(
            r#"
            SELECT table_schema AS schema, table_name AS name
            FROM information_schema.tables
            WHERE table_type = 'BASE TABLE'
              AND table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY table_schema, table_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let views = sqlx::query_as::<_, RelationRow>(
            r#"
            SELECT table_schema AS schema, table_name AS name
            FROM information_schema.views
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY table_schema, table_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let columns = sqlx::query_as::<_, ColumnInfo>(
            r#"
            SELECT table_schema AS schema, table_name AS relation, column_name AS name,
                   data_type, ordinal_position::int4
            FROM information_schema.columns
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY table_schema, table_name, ordinal_position
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let indexes = sqlx::query_as::<_, IndexInfo>(
            r#"
            SELECT n.nspname AS schema, i.relname AS name, t.relname AS relation,
                   ix.indisunique AS is_unique, ix.indisprimary AS is_primary
            FROM pg_catalog.pg_class i
            JOIN pg_catalog.pg_index ix ON ix.indexrelid = i.oid
            JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid
            JOIN pg_catalog.pg_namespace n ON n.oid = i.relnamespace
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
            ORDER BY n.nspname, i.relname
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let triggers = sqlx::query_as::<_, TriggerInfo>(
            r#"
            SELECT trigger_schema AS schema, trigger_name AS name, event_object_table AS relation,
                   event_manipulation, action_timing
            FROM information_schema.triggers
            WHERE trigger_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY trigger_schema, trigger_name, event_manipulation
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let routines = sqlx::query_as::<_, RoutineInfo>(
            r#"
            SELECT routine_schema AS schema, routine_name AS name, routine_type,
                   data_type
            FROM information_schema.routines
            WHERE routine_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY routine_schema, routine_name, routine_type
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        let sequences = sqlx::query_as::<_, SequenceInfo>(
            r#"
            SELECT sequence_schema AS schema, sequence_name AS name, data_type
            FROM information_schema.sequences
            WHERE sequence_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY sequence_schema, sequence_name
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(SchemaLoaderError::Query)?;

        Ok(Self {
            tables: attach_columns(tables, &columns)
                .into_iter()
                .map(|relation| TableInfo {
                    schema: relation.schema,
                    name: relation.name,
                    columns: relation.columns,
                })
                .collect(),
            views: attach_columns(views, &columns)
                .into_iter()
                .map(|relation| ViewInfo {
                    schema: relation.schema,
                    name: relation.name,
                    columns: relation.columns,
                })
                .collect(),
            indexes,
            triggers,
            routines,
            sequences,
        })
    }
}

#[derive(Debug, FromRow)]
struct RelationRow {
    schema: String,
    name: String,
}

#[derive(Debug)]
struct RelationWithColumns {
    schema: String,
    name: String,
    columns: Vec<ColumnInfo>,
}

fn attach_columns(relations: Vec<RelationRow>, columns: &[ColumnInfo]) -> Vec<RelationWithColumns> {
    relations
        .into_iter()
        .map(|relation| RelationWithColumns {
            columns: columns
                .iter()
                .filter(|column| {
                    column.schema == relation.schema && column.relation == relation.name
                })
                .cloned()
                .collect(),
            schema: relation.schema,
            name: relation.name,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attaches_only_columns_belonging_to_each_relation() {
        let relations = vec![
            RelationRow {
                schema: "public".into(),
                name: "users".into(),
            },
            RelationRow {
                schema: "public".into(),
                name: "orders".into(),
            },
        ];
        let columns = vec![
            ColumnInfo {
                schema: "public".into(),
                relation: "users".into(),
                name: "id".into(),
                data_type: "integer".into(),
                ordinal_position: 1,
            },
            ColumnInfo {
                schema: "public".into(),
                relation: "orders".into(),
                name: "id".into(),
                data_type: "integer".into(),
                ordinal_position: 1,
            },
        ];

        let result = attach_columns(relations, &columns);
        assert_eq!(result[0].columns.len(), 1);
        assert_eq!(result[0].columns[0].relation, "users");
        assert_eq!(result[1].columns.len(), 1);
        assert_eq!(result[1].columns[0].relation, "orders");
    }

    #[test]
    fn keeps_relations_with_no_matching_columns() {
        let relations = vec![RelationRow {
            schema: "public".into(),
            name: "events".into(),
        }];
        let columns = vec![ColumnInfo {
            schema: "public".into(),
            relation: "users".into(),
            name: "id".into(),
            data_type: "integer".into(),
            ordinal_position: 1,
        }];

        let result = attach_columns(relations, &columns);
        assert_eq!(result.len(), 1);
        assert!(result[0].columns.is_empty());
    }

    #[test]
    fn distinguishes_same_relation_name_across_schemas() {
        let relations = vec![
            RelationRow {
                schema: "public".into(),
                name: "users".into(),
            },
            RelationRow {
                schema: "audit".into(),
                name: "users".into(),
            },
        ];
        let columns = vec![
            ColumnInfo {
                schema: "public".into(),
                relation: "users".into(),
                name: "id".into(),
                data_type: "integer".into(),
                ordinal_position: 1,
            },
            ColumnInfo {
                schema: "audit".into(),
                relation: "users".into(),
                name: "action".into(),
                data_type: "text".into(),
                ordinal_position: 1,
            },
        ];

        let result = attach_columns(relations, &columns);
        assert_eq!(result[0].columns[0].name, "id");
        assert_eq!(result[1].columns[0].name, "action");
    }
}
