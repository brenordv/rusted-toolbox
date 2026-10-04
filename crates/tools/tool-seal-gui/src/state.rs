use crate::app::Tab;

pub struct AppState {
    pub active_tab: Tab,
    pub toasts: Vec<common_gui::widgets::Toast>,
    pub busy: bool,
}