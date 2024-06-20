use std::{fmt::Display, io::stdout, net::SocketAddr, str::FromStr, time::Duration};

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

pub struct BytesDisplay(u64);
impl Display for BytesDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 < 1024 {
            return write!(f, "{}B", self.0)
        }

        {
            let divisor = 1024 as u64;
            let kbs = self.0 / divisor;
            if kbs < 1024 {
                let reminder = self.0 - kbs * divisor;
                let reminder = reminder as f64 / divisor as f64;
                let reminder = (100.0 * reminder).round() / 100.0;
                return write!(f, "{}KB", kbs as f64 + reminder)
            }
        }

        {

            let divisor = 1024 * 1024 as u64;
            let mbs = self.0 / divisor;
            if mbs < 1024 {
                let reminder = self.0 - mbs * divisor;
                let reminder = reminder as f64 / divisor as f64;
                let reminder = (100.0 * reminder).round() / 100.0;
                return write!(f, "{}MB", mbs as f64 + reminder)
            }
        }

        {

            let divisor = 1024 * 1024 * 1024 as u64;
            let gbs = self.0 / divisor;
            let reminder = self.0 - gbs * divisor;
            let reminder = reminder as f64 / divisor as f64;
            let reminder = (100.0 * reminder).round() / 100.0;
            return write!(f, "{}GB", gbs as f64 + reminder)
        }
    }
}

pub struct FilePathDisplay<'a>(&'a str);
impl<'a> Display for FilePathDisplay<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let character_limit: u8 = 100;
        let len = self.0.len();
        if len <= character_limit as usize {
            write!(f, "{}", self.0)
        }
        else {
            write!(f, "...{}", &self.0[len - character_limit as usize..])
        }
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
