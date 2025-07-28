use std::io::stdin;

pub fn get_user_response(required: bool) -> String {
    let mut user_response = String::new();

    while user_response.is_empty() {
        stdin()
            .read_line(&mut user_response)
            .expect("Failed to read line");

        if !required && user_response.is_empty() {
            return String::new();
        }
    }

    user_response.trim().to_string()
}
