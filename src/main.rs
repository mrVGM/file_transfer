use std::{net::SocketAddr, os::windows::process, str::FromStr, sync::Arc, time::Duration};

use tokio::time::sleep;

mod files;
mod streaming;

#[tokio::main]
async fn main() {
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
        let (send_ch, recv_ch) = std::sync::mpsc::channel::<Progress>();

        tokio::spawn(async move {
            let mut last_log = std::time::SystemTime::now();
            let start_time = last_log;
            
            loop {
                let progress = recv_ch.recv();
                
                if let Ok(progress) = progress {
                    let now = std::time::SystemTime::now();
                    let duration = now.duration_since(last_log).unwrap().as_millis();

                    if duration >= 100 {
                        println!("Progress: {}/{}, Speed: {} B/s", progress.0, progress.1, progress.2);
                        last_log = now;
                    }
                }
                else {
                    break;
                }
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
