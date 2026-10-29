//! Identifiers for accounts, folders, messages and attachments.

/// A new UUIDv7: unique and sortable by creation time, so indexes stay compact.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Current time as Unix milliseconds, the unit of every timestamp in the database.
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_sort_by_creation() {
        let first = new_id();
        let second = new_id();
        assert_ne!(first, second);
        assert!(first < second);
    }
}
