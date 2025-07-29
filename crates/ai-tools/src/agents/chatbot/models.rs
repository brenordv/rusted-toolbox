use crate::utils::printer::RolePrinter;

pub struct ChatBotAgent {
    pub user_name: String,
    pub ai_name: String,
    pub ai_personality: String,
    pub agent_printer: RolePrinter,
    pub user_printer: RolePrinter,
    pub first_message_to_ai: Option<String>,
}
