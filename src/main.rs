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
/*
        tokio::spawn(async move {
            loop {
                println!("Progress: {}/{}, Speed: {} B/s", 0, size, 0);
                sleep(Duration::from_millis(100)).await;
            }
        });
*/
        
        receiver.receive(send_ch).await;
    }

    let finish_time = std::time::SystemTime::now();
    let duration = finish_time.duration_since(start_time).unwrap();

    println!("Done in {}", duration.as_millis());
}
