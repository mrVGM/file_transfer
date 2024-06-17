use std::{collections::HashSet, net::SocketAddr, str::FromStr, sync::{Arc, RwLock}, time::Duration};

use network_interface::NetworkInterface;
use serde_json::json;
use tokio::{net::{TcpListener, TcpStream, UdpSocket}, time::sleep};

use crate::ScopedRoutine;

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


pub struct ServerConnected(pub TcpStream);
pub struct ClientConnected(pub TcpStream);