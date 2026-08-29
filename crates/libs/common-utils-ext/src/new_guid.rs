use uuid::Uuid;

/// Returns a new random (v4) UUID as a hyphenated, lowercase string.
pub fn new_guid() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_guid_is_a_valid_hyphenated_uuid() {
        let guid = new_guid();

        assert!(Uuid::parse_str(&guid).is_ok());
        assert_eq!(guid.len(), 36);
    }

    #[test]
    fn new_guid_values_differ() {
        assert_ne!(new_guid(), new_guid());
    }
}
