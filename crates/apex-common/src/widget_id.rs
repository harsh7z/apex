use uuid::Uuid;

pub fn generate_widget_id() -> String {
    let id = Uuid::new_v4();
    format!("apex-{}", &id.to_string()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_id_format() {
        let id = generate_widget_id();
        assert!(id.starts_with("apex-"));
        assert_eq!(id.len(), 13); // "apex-" + 8 hex chars
    }
}
