use crossterm::{
    style::{Attribute, Color, ResetColor, SetAttribute, SetForegroundColor},
    ExecutableCommand,
};

use crate::tools::ai::models::models::Role;
use std::io::stdout;

pub struct RolePrinter {
    color: Color,
    name: String,
    max_padding: usize,
}

impl RolePrinter {
    pub fn new(role: Role, name: String, max_padding: usize) -> Self {
        Self {
            color: role.get_tag_color(),
            name,
            max_padding,
        }
    }

    pub fn print_tag(&self) {
        self.set_style();
        print!("[{:<width$}] ", self.name, width = self.max_padding);
        self.reset_style();
    }

    pub fn print(&self, message: String) {
        self.print_tag();
        println!("{}", message);
    }

    fn set_style(&self) {
        stdout()
            .execute(SetForegroundColor(self.color))
            .expect("Failed to set color");
        stdout()
            .execute(SetAttribute(Attribute::Bold))
            .expect("Failed to set bold");
    }

    fn reset_style(&self) {
        stdout().execute(ResetColor).expect("Failed to reset color");
    }
}