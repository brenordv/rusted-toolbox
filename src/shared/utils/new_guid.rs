use uuid::Uuid;

pub fn new_guid() -> String {
    Uuid::new_v4().to_string()
}
