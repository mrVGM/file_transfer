use std::{io::stdout, net::SocketAddr, str::FromStr, time::Duration};

use tokio::time::sleep;

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod files;
mod streaming;
mod ui;
mod pairing;
mod utils;

pub struct ScopedRoutine(tokio::task::JoinHandle<()>);
impl ScopedRoutine {
    pub fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        ScopedRoutine(handle)
    } 
}

impl Drop for ScopedRoutine {
    fn drop(&mut self) {
        self.0.abort();
    }
}


fn get_event() -> ui::InputCommand {
    let mut command = ui::InputCommand::None;
    if event::poll(std::time::Duration::from_millis(50)).unwrap() {

        if let Event::Key(key) = event::read().unwrap() {
            if key.kind == event::KeyEventKind::Press {
                command = match &key.code {
                    KeyCode::Esc => ui::InputCommand::Exit,
                    KeyCode::Up => ui::InputCommand::Up,
                    KeyCode::Down => ui::InputCommand::Down,
                    KeyCode::Enter => ui::InputCommand::Confirm,
                    _ => ui::InputCommand::None
                };
            }
        }
    }

    command
}


#[tokio::main]
async fn main() {

    enable_raw_mode().unwrap();
    stdout().execute(EnterAlternateScreen).unwrap();
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout())).unwrap();

    let mut app = ui::App::new();

    while !app.is_shut_down() {
        let event = get_event();
        app.handle_input(event);

        terminal.draw(|frame| {
            app.tick(frame);
        }).unwrap();
    }

    disable_raw_mode().unwrap();
    stdout().execute(LeaveAlternateScreen).unwrap();
}
