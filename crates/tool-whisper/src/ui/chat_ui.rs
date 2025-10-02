use anyhow::Result;
use ratatui::crossterm::event::{poll, KeyEventKind};
use ratatui::layout::Position;
use ratatui::widgets::{List, ListItem};
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};
use std::cmp::PartialEq;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use tracing::{debug, info};

fn get_banner() -> String {
    format!(
        "{} | v{}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
}

enum InputMode {
    Normal,
    Editing,
}

enum MessageKind {
    Own,
    Peer,
}

enum ChatState {
    Ok,
    Exit,
}

struct Message {
    text: String,
    kind: MessageKind,
}

impl Message {
    pub fn format(&'_ self) -> Line<'_> {
        match self.kind {
            MessageKind::Own => {
                let color = Color::Green;
                let text = format!("> {}", self.text);
                Line::from(Span::styled(text, Style::default().fg(color)))
            }
            MessageKind::Peer => {
                let color = Color::White;
                let text = format!("< {}", self.text);
                Line::from(Span::styled(text, Style::default().fg(color)))
            }
        }
    }
}

pub struct ChatUi {
    // Base properties
    role_name: String,
    outgoing_tx: Sender<String>,
    incoming_rx: Receiver<String>,
    // Ui properties
    /// Current value of the input box
    input: String,
    /// Position of the cursor in the editor area (character index, not byte)
    character_index: usize,
    /// Current input mode
    input_mode: InputMode,
    /// History of messages (both sent and received)
    messages: Vec<Message>,
}

impl PartialEq for ChatState {
    fn eq(&self, other: &Self) -> bool {
        match self {
            ChatState::Ok => match other {
                ChatState::Ok => true,
                _ => false,
            },
            ChatState::Exit => match other {
                ChatState::Exit => true,
                _ => false,
            },
        }
    }
}

impl ChatUi {
    pub fn new(
        outgoing_tx: Sender<String>,
        incoming_rx: Receiver<String>,
        role_name: String,
    ) -> Self {
        Self {
            role_name: format!("{}-Ui", role_name),
            outgoing_tx,
            incoming_rx,
            input: String::new(),
            character_index: 0,
            input_mode: InputMode::Editing,
            messages: Vec::new(),
        }
    }

    pub fn run(self) -> Result<()> {
        info!("Starting chat UI...");
        let terminal = ratatui::init();
        let _ = self.chat_loop(terminal)?;
        ratatui::restore();
        Ok(())
    }

    fn chat_loop(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        loop {
            // Processing received messages
            while let Ok(msg) = self.incoming_rx.try_recv() {
                debug!("[{}] Received message: {}", self.role_name, msg);
                self.messages.push(Message {
                    text: msg,
                    kind: MessageKind::Peer,
                });
            }

            terminal.draw(|frame| self.draw(frame))?;

            if self.process_key_inputs()? == ChatState::Exit {
                return Ok(());
            };
        }
    }

    fn draw(&self, frame: &mut Frame) {
        // Layout:
        // [banner (1)] - app name and version.
        // [messages (flex)] - message history
        // [input (3)] - input box and cursor
        // [helper (1)] - helper text (e.g. "Press q to exit")

        let vertical = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ]);
        let [banner_area, messages_area, input_area, help_area] = vertical.areas(frame.area());

        // Banner (top)
        frame.render_widget(Paragraph::new(get_banner()), banner_area);

        // Messages (no borders)
        let items: Vec<ListItem> = self
            .messages
            .iter()
            .map(|m| {
                let line = m.format();
                ListItem::new(line)
            })
            .collect();
        frame.render_widget(List::new(items), messages_area);

        // Input (bottom-1)
        let input = Paragraph::new(self.input.as_str())
            .style(match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Green),
            })
            .block(Block::bordered().title("What's on your mind?"));
        frame.render_widget(input, input_area);

        // Cursor in input field when editing
        if let InputMode::Editing = self.input_mode {
            #[allow(clippy::cast_possible_truncation)]
            frame.set_cursor_position(Position::new(
                input_area.x + self.character_index as u16 + 1,
                input_area.y + 1,
            ));
        }

        // Helper (very bottom)
        let (helper_line, style) = match self.input_mode {
            InputMode::Normal => (
                vec![
                    "Press ".into(),
                    "q".bold(),
                    " to exit, ".into(),
                    "e".bold(),
                    " to start editing.".into(),
                ],
                Style::default().add_modifier(Modifier::RAPID_BLINK),
            ),
            InputMode::Editing => (
                vec![
                    "Press ".into(),
                    "Esc".bold(),
                    " to stop editing, ".into(),
                    "Enter".bold(),
                    " to send".into(),
                ],
                Style::default(),
            ),
        };
        frame.render_widget(
            Paragraph::new(Line::from(helper_line)).style(style),
            help_area,
        );
    }

    //region: Ui Logic
    fn process_key_inputs(&mut self) -> Result<ChatState> {
        if poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match self.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('e') => {
                            self.input_mode = InputMode::Editing;
                        }
                        KeyCode::Char('q') => {
                            return Ok(ChatState::Exit);
                        }
                        _ => {}
                    },
                    InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Enter => self.submit_message()?,
                        KeyCode::Char(to_insert) => self.enter_char(to_insert),
                        KeyCode::Backspace => self.delete_char(),
                        KeyCode::Left => self.move_cursor_left(),
                        KeyCode::Right => self.move_cursor_right(),
                        KeyCode::Esc => self.input_mode = InputMode::Normal,
                        _ => {}
                    },
                    InputMode::Editing => {}
                }
            }
        }
        Ok(ChatState::Ok)
    }

    fn submit_message(&mut self) -> Result<()> {
        if self.input.is_empty() {
            debug!("[{}] Input is empty, not submitting", self.role_name);
            return Ok(());
        }
        let msg = std::mem::take(&mut self.input);

        debug!("[{}] Submitting message: {}", self.role_name, msg);
        self.outgoing_tx.send(msg.clone())?;

        debug!(
            "[{}] Message sent. Pushing to display history...",
            self.role_name
        );
        self.messages.push(Message {
            text: msg,
            kind: MessageKind::Own,
        });

        self.reset_cursor();

        Ok(())
    }
    //endregion: Ui Logic

    //region: Ui
    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        if self.character_index != 0 {
            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            let before = self.input.chars().take(from_left_to_current_index);
            let after = self.input.chars().skip(current_index);

            self.input = before.chain(after).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }
    //endregion: Ui
}
