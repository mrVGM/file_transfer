use std::{collections::VecDeque, mem::size_of, net::SocketAddr, str::FromStr, sync::Arc};

use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener, sync::{Mutex, RwLock}};

use crate::files::{self, FileChunk};

pub struct Sender {
    file_reader: Arc<files::FileReader>,
    socket_addr: std::net::SocketAddr,
    listener: Arc<TcpListener>
}

impl Sender {
    pub async fn new(reader: files::FileReader) -> Self {
        let ip = std::net::IpAddr::from_str("0.0.0.0").unwrap();
        let socket_addr = std::net::SocketAddr::new(ip, 0);
        let listener = tokio::net::TcpListener::bind(socket_addr).await.unwrap();

        let addr = listener.local_addr().unwrap();

        Sender {
            file_reader: Arc::new(reader),
            socket_addr: addr,
            listener: Arc::new(listener)
        }
    }

    pub fn get_addr(&self) -> SocketAddr {
        self.socket_addr
    }

    pub async fn start(&self) {

        let semaphore = Arc::new(tokio::sync::Semaphore::new(0));
        let semaphore_clone = semaphore.clone();
        let file_reader = self.file_reader.clone();
        let listener = self.listener.clone();

        let listen = tokio::spawn(async move {
            let connections = Arc::new(Mutex::new(0 as u8));
            loop {
                let connections = connections.clone();
                let (mut stream, addr) = listener.accept().await.unwrap();

                {
                    let connections = &*connections;
                    let mut connections = connections.lock().await;
                    let connections = &mut *connections;
                    *connections = *connections + 1;
                }

                let reader = file_reader.clone();
                let semaphore = semaphore.clone();
                tokio::spawn(async move {
                    loop {
                        let chunk = reader.get_chunk().await;

                        if let Some(chunk) = chunk {
                            let packed = chunk.pack();
                            stream.write(&packed).await.unwrap();
                        }
                        else {
                            break;
                        }
                    }

                    {
                        let connections = &mut *connections.lock().await;
                        *connections = *connections - 1;
                        if *connections == 0 {
                            semaphore.add_permits(1);
                        }
                    }
                });
            }
        });

        let permit = semaphore_clone.acquire().await.unwrap();
        permit.forget();
        
        listen.abort();
    }
}


pub struct Receiver {
    file_writer: Arc<files::FileWriter>,
    socket_addr: SocketAddr
}

impl Receiver {
    pub fn new(writter: files::FileWriter, addr: SocketAddr) -> Self {
        let size = writter.get_size();
        Receiver {
            file_writer: Arc::new(writter),
            socket_addr: addr,
        }
    }

    pub async fn receive(&self, sender: std::sync::mpsc::Sender<(u64, u64, f64)>) {
        let writer = self.file_writer.clone();
        let writer_clone = writer.clone();

        let semaphore = Arc::new(tokio::sync::Semaphore::new(0));
        let semaphore_clone = semaphore.clone();

        tokio::spawn(async move {
            writer_clone.write().await;
            semaphore_clone.add_permits(1);
        });

        let file_size = writer.get_size();
        
        let num_sockets: u8 = 2;

        let bytes_received = Arc::new(Mutex::new(0 as u64));
        let samples = Arc::new(Mutex::new(VecDeque::<(u64, std::time::SystemTime)>::new()));

        let sender = Arc::new(sender);
        for _ in 0..num_sockets {
            let addr = self.socket_addr;
            let writer = writer.clone();
            let semaphore = semaphore.clone();

            let sender = sender.clone();
            let bytes_received = bytes_received.clone();
            let samples = samples.clone();

            tokio::spawn(async move {
                let sock = tokio::net::TcpSocket::new_v4().unwrap();
                let mut stream = sock.connect(addr).await.unwrap();

                loop {
                    let offset : Option<u64> = {
                        let mut offset_buff = [0 as u8; size_of::<u64>()];

                        let mut read: usize = 0;
                        while read < size_of::<u64>() {
                            let bytes = stream.read(&mut offset_buff[read..]).await.unwrap();
                            if bytes == 0 {
                                break;
                            }

                            read += bytes;
                        }

                        if read < size_of::<u64>() {
                            None
                        }
                        else {
                            Some(u64::from_be_bytes(offset_buff))
                        }
                    };

                    if let None = offset {
                        break;
                    }

                    let size: Option<u32> = {
                        let mut size_buff = [0 as u8; size_of::<u32>()];

                        let mut read: usize = 0;
                        while read < size_of::<u32>() {
                            let bytes = stream.read(&mut size_buff[read..]).await.unwrap();
                            if bytes == 0 {
                                break;
                            }

                            read += bytes;
                        }

                        if read < size_of::<u32>() {
                            None
                        }
                        else {
                            Some(u32::from_be_bytes(size_buff))
                        }
                    };

                    if let None = size {
                        break;
                    }

                    let data: Option<Vec<u8>> = {
                        let size = size.unwrap() as usize;
                        let mut buff = vec![0 as u8; size];

                        let mut read: usize = 0;
                        while read < size {
                            let bytes = stream.read(&mut buff[read..]).await.unwrap();
                            if bytes == 0 {
                                break;
                            }

                            read += bytes;

                            let bytes_received = &mut *bytes_received.lock().await;
                            let samples = &mut *samples.lock().await;
                            *bytes_received += bytes as u64;
                            samples.push_back((*bytes_received, std::time::SystemTime::now()));
                            
                            while samples.len() > 10 {
                                samples.pop_front();
                            }

                            let speed = {
                                if samples.len() < 2 {
                                    0.0
                                }
                                else {
                                    let fst = samples.front().unwrap();
                                    let last = samples.front().unwrap();

                                    let mut diff = (last.0 - fst.0) as f64;
                                    diff /= (1024 * 1024) as f64;
                                    let mut time_diff = last.1.duration_since(fst.1).unwrap().as_millis() as f64;
                                    time_diff /= 1000.0;

                                    if time_diff < 0.0001 {
                                        0.0
                                    }
                                    else {
                                        diff / time_diff
                                    }
                                }
                            };

                            sender.send((*bytes_received, file_size, speed)).unwrap();
                        }

                        if read < size {
                            None
                        }
                        else {
                            Some(buff)
                        }
                    };

                    let chunk = FileChunk {
                        offset: offset.unwrap(),
                        size: size.unwrap(),
                        data: data.unwrap()
                    };

                    writer.push(chunk).await;
                }

                semaphore.add_permits(1);
            });
        }

        semaphore.acquire_many(num_sockets as u32 + 1).await.unwrap();
    }
}