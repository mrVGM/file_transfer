use std::{net::SocketAddr, str::FromStr, time::Duration};

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

        let reader = files::FileReader::new("C:\\Users\\Vas\\dev\\rust\\file_transfer\\1\\pakchunk1-Windows.pak");
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

        let progress = receiver.progress.clone();

        tokio::spawn(async move {
            loop {
                let (cur, total, stamps) = &*progress.read().await;
                let mut speed = 0.0;

                if stamps.len() > 1 {
                    let fst = stamps.front().unwrap();
                    let last = stamps.back().unwrap();
                    let duration = last.1.duration_since(fst.1).unwrap();

                    speed = (last.0 - fst.0) as f64 / (duration.as_millis() as f64 / 1000.0);
                }

                println!("Progress: {}/{}, Speed: {} B/s", *cur, *total, speed);

                if *cur >= *total {
                    break;
                }


                sleep(Duration::from_millis(100)).await;
            }
        });

        receiver.receive().await;
    }

    let finish_time = std::time::SystemTime::now();
    let duration = finish_time.duration_since(start_time).unwrap();

    println!("Done in {}", duration.as_millis());
}
