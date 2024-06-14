mod files;
mod streaming;

use std::{net::{Ipv4Addr, SocketAddr}, str::FromStr, sync::Arc, u8};

use tokio::{io::AsyncWriteExt, sync::{Mutex, Semaphore}};

#[tokio::main]
async fn main() {
    let reader = files::FileReader::new("C:\\Users\\Vas\\dev\\rust\\file_transfer\\1\\pakchunk1-Windows.pak");
    let writer = files::FileWriter::new("C:\\Users\\Vas\\dev\\rust\\file_transfer\\2", "pakchunk1-Windows.pak", reader.get_size());

    let sender = streaming::Sender::new(reader).await;

    let addr = sender.get_addr();
    let ip = std::net::IpAddr::from_str("127.0.0.1").unwrap();
    let addr = SocketAddr::new(ip, addr.port());

    tokio::spawn(async move {
        sender.start().await;
    });

    let receiver = streaming::Receiver::new(writer, addr);
    receiver.receive().await;
/*    
    let ip = std::net::IpAddr::from_str("0.0.0.0").unwrap();
    let socket_addr = std::net::SocketAddr::new(ip, 4040);
    let socket = tokio::net::TcpListener::bind(socket_addr).await.unwrap();


    let num_connections = Arc::new(Mutex::new(0));
    let num_chunks = Arc::new(Mutex::new(100 as u8));
    let semaphore = Arc::new(Semaphore::new(0));
    let semaphore_1 = semaphore.clone(); 
    let server = tokio::spawn(async move {
        let connections = num_connections.clone();
        loop {
            let connections = connections.clone();
            let (mut stream, addr) = socket.accept().await.unwrap();

            {
                let connections = &*connections;
                let mut connections = connections.lock().await;
                let connections = &mut *connections;
                *connections = *connections + 1;
            }

            let connections = connections.clone();
            let num_chunks = num_chunks.clone();

            let semaphore = semaphore.clone();
            tokio::spawn(async move {
                loop {
                    {
                        let num = &*num_chunks;
                        let num = &mut *num.lock().await;
                        if *num == 0 {
                            break;
                        }
                        *num = *num - 1;
                    }

                    let mut buff: Box<Vec<u8>> = Box::new(vec![]);
                    buff.resize(8_388_608, 0);

                    let arr = &mut buff[..];
                    stream.write(arr).await;
                }


                {
                    let connections = &*connections;
                    let mut connections = connections.lock().await;
                    let connections = &mut *connections;
                    *connections = *connections - 1;

                    if *connections == 0 {
                        let semaphore = &*semaphore;
                        semaphore.add_permits(1);
                    }

                }
            });

        }
    });

    let semaphore = &*semaphore_1;
    let semaphore = &*semaphore;
    semaphore.acquire().await;

*/

    println!("Done");
}
