use std::{collections::HashSet, net::SocketAddr, str::FromStr, sync::{Arc, RwLock}, time::Duration};

use network_interface::NetworkInterface;
use serde_json::json;
use tokio::{net::{TcpListener, UdpSocket}, time::sleep};

use crate::ScopedRoutine;

pub struct PairingServer(ScopedRoutine);

impl PairingServer {
    pub fn new() -> Self {
        let hello_server = std::net::UdpSocket::bind("0.0.0.0:4040").unwrap();
        let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();

        let hello_server = UdpSocket::from_std(hello_server).unwrap();
        let listener = TcpListener::from_std(listener).unwrap();

        let port = listener.local_addr().unwrap().port();
        let host = hostname::get().unwrap();

        let hello = tokio::spawn(async move {
            let mut buff: [u8; 6] = [0; 6];
            loop {
                let (read, socket_addr) = hello_server.recv_from(&mut buff).await.unwrap();
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
        });

        PairingServer(ScopedRoutine(hello))
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
