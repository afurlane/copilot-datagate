//! Policy engine: tables/columns allow-lists, row/complexity limits.
//! Evaluated before any query is built. Placeholder for v0.1.

// Placeholder policy engine; only exercised by tests until wired into query builder (v0.1).
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct Policy {
    pub allowed_tables: Vec<String>,
}

#[allow(dead_code)]
impl Policy {
    pub fn allows_table(&self, table: &str) -> bool {
        self.allowed_tables.iter().any(|t| t == table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denies_table_not_in_allowlist() {
        let policy = Policy {
            allowed_tables: vec!["users".into()],
        };
        assert!(policy.allows_table("users"));
        assert!(!policy.allows_table("secrets"));
    }
}
