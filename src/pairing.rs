use core::str;
use std::{collections::HashSet, net::SocketAddr, path::Path, str::FromStr, sync::{Arc, RwLock}, time::Duration};

use network_interface::NetworkInterface;
use serde_json::json;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream, UdpSocket}, time::sleep};

use crate::{utils::{self, FileData}, ScopedRoutine};

struct PairingServerUDP(std::sync::mpsc::Receiver<bool>, tokio::sync::watch::Sender<bool>);

impl Drop for PairingServerUDP {
    fn drop(&mut self) {
        self.1.send(true).unwrap();

        tokio::spawn(async move {
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.send_to("Hello!".as_bytes(), SocketAddr::from_str("127.0.0.1:4040").unwrap()).unwrap();
        });

        self.0.recv().unwrap();
    }
}

impl PairingServerUDP {
    pub fn new(port: u16) -> Self {
        let (stop_channel_send, mut stop_channel_recv) = tokio::sync::watch::channel(false);
        let (finish_channel_send, finish_channel_recv) = std::sync::mpsc::channel::<bool>();
        
        tokio::spawn(async move {
            let hello_server = tokio::net::UdpSocket::bind("0.0.0.0:4040").await.unwrap();
            let host = hostname::get().unwrap();

            let mut buff: [u8; 6] = [0; 6];
            loop {
                let (read, socket_addr) = hello_server.recv_from(&mut buff).await.unwrap();
                let stop = *stop_channel_recv.borrow_and_update();
                if stop {
                    break;
                }

                if read != 6 {
                    continue;
                }
                
                let message = &*String::from_utf8_lossy(&mut buff);
                if message != "Hello!" {
                    continue;
                }

                let json = json!({
                    "name": host.to_str().unwrap(),
                    "port": port
                });

                let message = json.to_string();
                hello_server.send_to(message.as_bytes(), socket_addr).await.unwrap();
            }

            finish_channel_send.send(true).unwrap();
        });

        PairingServerUDP(finish_channel_recv, stop_channel_send)
    }
}


struct ConnectionListener(u16, std::sync::mpsc::Receiver<TcpStream>);
impl ConnectionListener {
    fn new() -> ConnectionListener {
        let (send, recv) = std::sync::mpsc::channel::<TcpStream>();
        let (send_port, recv_port) = std::sync::mpsc::channel::<u16>();
        
        tokio::spawn(async move {
            let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            send_port.send(port);

            let (stream, _) = listener.accept().await.unwrap();
            send.send(stream);
        });

        let port = recv_port.recv().unwrap();

        ConnectionListener(port, recv)
    }
}

impl Drop for ConnectionListener {
    fn drop(&mut self) {
        let addr = format!("127.0.0.1:{}", self.0);
        let socket = std::net::TcpStream::connect(addr);
    }
}

pub struct PairingServer(ConnectionListener, PairingServerUDP);
impl PairingServer {
    pub fn new() -> Self {
        let connection_listener = ConnectionListener::new();
        let pairing_server_udp = PairingServerUDP::new(connection_listener.0);

        PairingServer(connection_listener, pairing_server_udp)
    }

    pub fn try_get_stream(&self) -> Option<TcpStream> {
        match self.0.1.try_recv() {
            Ok(stream) => Some(stream),
            _ => None
        }
    }
}

type PCPayload = Vec<(String, SocketAddr)>;
pub struct PairingClient(NetworkInterface, ScopedRoutine, Arc<RwLock<PCPayload>>);

impl PairingClient {
    pub fn get_servers(&self) -> Arc<RwLock<PCPayload>> {
        self.2.clone()
    } 

    pub fn new(net_interface: NetworkInterface) -> Self {
        let servers_found: Arc<RwLock<PCPayload>> = Arc::new(RwLock::new(vec![]));
        let servers_found_clone = servers_found.clone();
        let addr = net_interface.addr.iter().find(|addr| {
            addr.ip().is_ipv4()
        }).unwrap();
        let socket_addr = SocketAddr::new(addr.ip(), 0);
        let broadcast_addr = SocketAddr::new(addr.broadcast().unwrap(), 4040);
        
        let lookup = tokio::spawn(async move {
            let socket = UdpSocket::bind(socket_addr).await.unwrap();
            let socket = Arc::new(socket);
            let socket_clone = socket.clone();
            
            let recv_routine = tokio::spawn(async move {
                let mut buff: [u8; 1024] = [0; 1024];
                let mut found = HashSet::<String>::new();

                loop {
                    let (read, socket_addr) = socket_clone.recv_from(&mut buff).await.unwrap();
                    let addr_str = socket_addr.to_string();
                    if found.contains(&addr_str) {
                        continue;
                    }
                    found.insert(addr_str);

                    let message = String::from_utf8_lossy(&buff[..read]);

                    let json = serde_json::Value::from_str(&*message);
                    if let Ok(value) = json {
                        let port = value["port"].as_u64().unwrap();
                        let socket_addr = SocketAddr::new(socket_addr.ip(), port as u16);
                        let name = value["name"].as_str().unwrap();
                        let servers = &mut *servers_found_clone.write().unwrap();
                        servers.push((String::from(name), socket_addr));
                    }
                }
            });

            let _recv_from = ScopedRoutine::new(recv_routine);

            let message = "Hello!";
            loop {
                socket.send_to(message.as_bytes(), broadcast_addr).await.unwrap();
                sleep(Duration::from_millis(1000)).await;
            }
        });

        PairingClient(net_interface, ScopedRoutine::new(lookup), servers_found)
    }
}


pub struct ServerConnected;
pub struct ClientConnected;

type ServerPayload = (TcpListener, Vec<FileData>);

impl ServerConnected {
    pub fn new(stream: TcpStream) -> Self {
        tokio::spawn(async move {
            let mut stream = stream;
            let mut buff: [u8; 1024] = [0; 1024];
            let mut cnt: u32 = 0;
            let mut str_buff = String::new();

            let data_listener = TcpListener::bind("0.0.0.0:0").await.unwrap();

            let file_list = utils::get_files();
            let mut payload = (data_listener, file_list);

            loop {
                let read = stream.read(&mut buff).await.unwrap();
                if read == 0 {
                    break;
                }

                for c in &buff[..read] {
                    str_buff.push(*c as char);

                    if *c == b'{' {
                        cnt += 1;
                    }
                    if *c == b'}' {
                        cnt -= 1;
                    }

                    if cnt == 0 {
                        ServerConnected::handle_req(&mut stream, &str_buff, &mut payload).await;
                        str_buff.clear();
                    }
                }
            }
        });

        ServerConnected
    }

    async fn handle_req(stream: &mut TcpStream, req: &str, payload: &mut ServerPayload) {
        let mut json = serde_json::Value::from_str(req).unwrap();

        let subject = String::from(json["subject"].as_str().unwrap());

        if subject == "introduction" {
            let host = hostname::get().unwrap();
            let host = host.to_str().unwrap();

            json = json!({
                "subject": "introduction",
                "name": host
            });
        }

        if subject == "file_list" {
            let file_list: Vec<serde_json::Value> = payload.1.iter().map(|x| {
                let path = String::from(x.relative_path.to_str().unwrap());
                let size = x.size;
                json!({
                    "path": path,
                    "size": size
                })
            }).collect();

            let port = payload.0.local_addr().unwrap().port();

            json = json!({
                "data_port": port,
                "file_list": file_list
            });
        }

        stream.write(json.to_string().as_bytes()).await.unwrap();
    }

}

impl ClientConnected {
    pub fn new(stream: TcpStream) -> Self {
        tokio::spawn(async move {
            let mut stream = stream;

            let host = hostname::get().unwrap();
            let host = host.to_str().unwrap();

            let id_req = json!({
                "subject": "introduction",
                "name": host
            });

            let resp = ClientConnected::make_request(id_req, &mut stream).await;
            println!("resp: {}", resp.to_string());
            
            let file_list = json!({
                "subject": "file_list"
            });
            
            let resp = ClientConnected::make_request(file_list, &mut stream).await;
            
            println!("File List: {}", resp.to_string());
        });

        ClientConnected
    }

    async fn make_request(req: serde_json::Value, stream: &mut TcpStream) -> serde_json::Value {
        let str = req.to_string();
        stream.write(str.as_bytes()).await.unwrap();

        let mut resp = String::new();
        let mut buff: [u8; 1024] = [0; 1024];
        let mut cnt: u32 = 0;

        loop {
            let read = stream.read(&mut buff).await.unwrap();
            if read == 0 {
                break;
            }

            for c in &buff[..read] {
                resp.push(*c as char);
                if *c == b'{' {
                    cnt += 1;
                }
                if *c == b'}' {
                    cnt -= 1;
                }

                if cnt == 0 {
                    break;
                }
            }

            if cnt == 0 {
                break;
            }
        }

        let resp = serde_json::Value::from_str(&resp).unwrap();
        resp
    }
}