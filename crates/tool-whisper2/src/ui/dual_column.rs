use std::sync::mpsc::Receiver;
use std::time::Duration;
use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    DefaultTerminal, Frame,
};
use anyhow::Result;
use ratatui::widgets::{List, ListItem, ListState};
use tracing::{error, info};

pub struct DualColumnUi {
    app_title: String,
    outgoing_message_receiver: Receiver<String>,
    incoming_message_receiver: Receiver<String>,
    incoming_messages: Vec<String>,
    outgoing_messages: Vec<String>,
    incoming_list_state: ListState,
    outgoing_list_state: ListState,

}

impl DualColumnUi {
    pub fn new(incoming_rx: Receiver<String>, outgoing_rx: Receiver<String>, role: String) -> Self {
        Self {
            app_title: format!("[{}] {} | v{}", role, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
            incoming_message_receiver: incoming_rx,
            outgoing_message_receiver: outgoing_rx,
            incoming_messages: Vec::new(),
            outgoing_messages: Vec::new(),
            incoming_list_state: ListState::default(),
            outgoing_list_state: ListState::default(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        color_eyre::install()
            .map_err(|e| anyhow::anyhow!("Error installing color-eyre: {}", e))?;

        let mut terminal = ratatui::init();

        let mut should_quit = false;
        while !should_quit {
            match self.incoming_message_receiver.try_recv() {
                Ok(incoming_message) => {
                    if incoming_message.is_empty() {
                        continue;
                    }

                    self.incoming_messages.push(incoming_message.clone());

                    // Auto-scroll to the latest message
                    if !self.incoming_messages.is_empty() {
                        self.incoming_list_state.select(Some(self.incoming_messages.len() - 1));
                    }
                }
                Err(e) => {
                    //error!("Error reading incoming message: {}", e);
                }
            }

            match self.outgoing_message_receiver.try_recv() {
                Ok(outgoing_message) => {
                    if outgoing_message.is_empty() {
                        continue;
                    }

                    self.outgoing_messages.push(outgoing_message.clone());

                    // Auto-scroll to the latest message
                    if !self.outgoing_messages.is_empty() {
                        self.outgoing_list_state.select(Some(self.outgoing_messages.len() - 1));
                    }
                }
                Err(_e) => {
                    //error!("Error reading outgoing message: {}", e);
                }
            }

            terminal.draw(|frame: &mut Frame| self.layout(frame))?;
            should_quit = self.handle_events()?;
        }

        ratatui::restore();

        Ok(())
    }

    fn layout(&mut self, frame: &mut Frame) {
        let vertical = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ]);
        let horizontal = Layout::horizontal([Constraint::Ratio(1, 2); 2]);
        let [title_bar, main_area, status_bar] = vertical.areas(frame.area());
        let [left, right] = horizontal.areas(main_area);

        frame.render_widget(
            Block::new().borders(Borders::TOP).title(self.app_title.as_str()),
            title_bar,
        );
        frame.render_widget(
            Block::new().borders(Borders::TOP).title("Status Bar"),
            status_bar,
        );

        let inc_messages: Vec<ListItem> = self
            .incoming_messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = Line::from(Span::raw(format!("{i:0>5}: {m}")));
                ListItem::new(content)
            })
            .collect();

        let inc_messages = List::new(inc_messages).block(Block::bordered().title("Incoming"));

        let out_messages: Vec<ListItem> = self
            .outgoing_messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let content = Line::from(Span::raw(format!("{i:0>5}: {m}")));
                ListItem::new(content)
            })
            .collect();

        let out_messages = List::new(out_messages).block(Block::bordered().title("Outgoing"));

        frame.render_stateful_widget(inc_messages, left, &mut self.incoming_list_state);
        frame.render_stateful_widget(out_messages, right, &mut self.outgoing_list_state);
    }

    fn handle_events(&self) -> std::io::Result<bool> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    return Ok(true);
                }
            }
        }
        Ok(false)

    }
}