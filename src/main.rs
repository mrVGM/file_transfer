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


    let start_time = std::time::SystemTime::now();

    let command_line: Vec<String> = std::env::args().collect();

    if command_line.len() < 2 {
        return;
    }
    let mode = &command_line[1];

    let cur_dir = std::env::current_dir().unwrap();
    if mode == "server" {
        if command_line.len() != 3 {
            return;
        }

        let file = &command_line[2];
        let mut file = std::path::PathBuf::from_str(file).unwrap();

        if !file.is_absolute() {
            file = cur_dir.join(file);
        }

        let reader = files::FileReader::new(file.to_str().unwrap());
        let size = reader.get_size();
        let sender = streaming::Sender::new(reader).await;
        let addr = sender.get_addr();
        dbg!(addr);
        println!("File size: {}", size);
        sender.start().await;
    }
    else if mode == "client" {
        if command_line.len() < 5 {
            return;
        }
        let socket_addr = &command_line[2];
        let socket_addr = SocketAddr::from_str(&socket_addr).unwrap();

        let size = &command_line[3];
        let size = u64::from_str(&size).unwrap();

        let file = &command_line[4];

        let writer = files::FileWriter::new(cur_dir.to_str().unwrap(), file, size);
        let receiver = streaming::Receiver::new(writer, socket_addr);

        type Progress = (u64, u64, f64);
        let (send_ch, mut recv_ch) = tokio::sync::watch::channel::<Progress>((0, 0, 0.0));

        tokio::spawn(async move {
            let start_time = std::time::SystemTime::now();
            
            loop {
                let changed = recv_ch.changed().await;
                
                if changed.is_err() {
                    break;
                }
                let progress = *recv_ch.borrow_and_update();
                if progress.0 >= progress.1 {
                    break;
                }

                println!("Progress: {}/{}, Speed: {} MB/s", progress.0, progress.1, progress.2);
                sleep(Duration::from_millis(500)).await;
            }

            let end_time = std::time::SystemTime::now();
            let full_steam_duration = end_time.duration_since(start_time).unwrap();
            println!("Full stream duration: {}", full_steam_duration.as_millis());
        });
        
        receiver.receive(send_ch).await;
    }

    let finish_time = std::time::SystemTime::now();
    let duration = finish_time.duration_since(start_time).unwrap();

    println!("Done in {}", duration.as_millis());
}
