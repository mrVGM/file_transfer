use std::net::SocketAddr;

use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use ratatui::{style::{Color, Stylize}, widgets::{Block, Borders, ListItem}, Frame};
use tokio::net::TcpStream;

use crate::pairing::{self, PairingClient, PairingServer};

pub enum InputCommand {
    None,
    Exit,
    Up,
    Down,
    Confirm
}

#[derive(Clone, Copy)]
enum AppMode {
    Server,
    Client
}

enum AppState {
    Idle,
    ChoosingNetworkInterface,
    LookingForServers(PairingClient),
    WaitingForConnection(PairingServer),
    ServerConnected(pairing::ServerConnected),
    ClientConnected(pairing::ClientConnected)
}

struct ServerEntry(String, SocketAddr);

enum UIItem {
    Label(String),
    ChangeMode(AppMode),
    RunServer,
    ChooseNetInterface(String),
    Cancel,
    NetInterface(NetworkInterface),
    FindServer(String),
    ServerEntry(String, SocketAddr),
    Quit
}

impl UIItem {
    fn to_list_item(&self) -> ListItem {
        match &self {
            Self::Label(label) => {
                let tmp: &str = &*label;
                ListItem::new(tmp)
            }
            Self::ChangeMode(app_mode) => {
                let label = match &app_mode {
                    AppMode::Client => "Client",
                    AppMode::Server => "Server"
                };
                ListItem::new(label)
            }
            Self::RunServer => {
                ListItem::new("Run")
            }
            Self::ChooseNetInterface(name) => {    
                ListItem::new(format!("Network Interface {}", name))
            }
            Self::NetInterface(i) => {
                let label = format!("    {}", &i.name);
                ListItem::new(label)
            }
            Self::FindServer(name) => {
                ListItem::new(format!("Find Server {}", name))
            }
            Self::ServerEntry(name, _) => {
                ListItem::new(format!("    {}", name))
            }
            Self::Quit => {
                ListItem::new("Quit")
            }
            Self::Cancel => {
                ListItem::new("Cancel")
            }
        }
    }
}

pub struct App {
    app_mode: AppMode,
    net_interface: Option<NetworkInterface>,
    server: Option<ServerEntry>,
    app_state: AppState,
    ui_items: Vec<UIItem>,
    selected: u8,
    closed: bool
}

impl App {
    pub fn new() -> Self {
        App {
            app_mode: AppMode::Server,
            net_interface: None,
            server: None,
            app_state: AppState::Idle,
            ui_items: vec![],
            selected: 0,
            closed: false,
        }
    }

    pub fn handle_input(&mut self, input_command: InputCommand) {
        if self.ui_items.len() > 0 {
            if self.selected >= self.ui_items.len() as u8 {
                self.selected = self.ui_items.len() as u8 - 1;
            }
        }

        match input_command {
            InputCommand::Exit => {
                self.closed = true;
            }
            InputCommand::Up => {
                if self.selected == 0 {
                    self.selected = self.ui_items.len() as u8 - 1;
                }
                else {
                    self.selected -= 1;
                }
            }
            InputCommand::Down => {
                self.selected += 1;
                self.selected %= self.ui_items.len() as u8;
            }
            InputCommand::Confirm => {
                let chosen = &self.ui_items[self.selected as usize];

                match chosen {
                    UIItem::ChangeMode(_) => {
                        self.app_mode = match &self.app_mode {
                            AppMode::Server => AppMode::Client,
                            AppMode::Client => AppMode::Server,
                        }
                    }
                    UIItem::ChooseNetInterface(_) => {
                        self.app_state = AppState::ChoosingNetworkInterface;
                    }

                    UIItem::Quit => {
                        self.closed = true;
                    }

                    UIItem::Cancel => {
                        self.app_state = AppState::Idle;
                    }

                    UIItem::NetInterface(i) => {
                        self.net_interface = Some(i.clone());
                        self.app_state = AppState::Idle;
                    }

                    UIItem::FindServer(_) => {
                        if let Some(net_interface) = &self.net_interface {
                            self.app_state = AppState::LookingForServers(PairingClient::new(net_interface.clone()));
                        }
                    }
                    UIItem::RunServer => {
                        self.app_state = AppState::WaitingForConnection(PairingServer::new());
                    }

                    UIItem::ServerEntry(_, addr) => {
                        let stream = std::net::TcpStream::connect(addr);
                        if let Ok(stream) = stream {
                            let stream = tokio::net::TcpStream::from_std(stream).unwrap();
                            self.app_state = AppState::ClientConnected(pairing::ClientConnected::new(stream));
                        }
                        else {
                            self.app_state = AppState::Idle;
                        }
                    }

                    _ => {}
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, frame: &mut Frame) {
        self.ui_items.clear();

        match &self.app_state {
            AppState::Idle => {
                let host = hostname::get().unwrap();
                let label = String::from("Computer Name: ") + &match host.into_string() {
                    Ok(s) => s,
                    _ => String::from("N/A")
                };

                self.ui_items.push(UIItem::Label(label));
                self.ui_items.push(UIItem::ChangeMode(self.app_mode));
                
                if let AppMode::Server =  &self.app_mode {
                    self.ui_items.push(UIItem::RunServer);
                }
                else {
                    let name = match &self.net_interface {
                        None => "N/A",
                        Some(i) => &i.name
                    };
                    self.ui_items.push(UIItem::ChooseNetInterface(String::from(name)));

                    let name = match &self.server {
                        None => "N/A",
                        Some(ServerEntry(name, _)) => &name
                    };
                    self.ui_items.push(UIItem::FindServer(String::from(name)));
                }

                self.ui_items.push(UIItem::Quit);
            }
            AppState::ChoosingNetworkInterface => {
                let network_interfaces = NetworkInterface::show().unwrap();
        
                let interfaces = network_interfaces.into_iter();
                let interfaces = interfaces.filter(|net| {
                    net.addr.iter().any(|x| {
                        if let network_interface::Addr::V4(_) = x {
                            true
                        } 
                        else {
                            false
                        }
                    })
                });

                for i in interfaces {
                    self.ui_items.push(UIItem::NetInterface(i))
                }
                self.ui_items.push(UIItem::Cancel);
            }
            AppState::LookingForServers(client) => {
                let servers = client.get_servers();
                let servers = &*servers.read().unwrap();
                for s in servers {
                    self.ui_items.push(UIItem::ServerEntry(String::from(&s.0), s.1));
                }
                self.ui_items.push(UIItem::Cancel);
            }
            AppState::WaitingForConnection(server) => {
                if let Some(stream) = server.try_get_stream() {
                    self.app_state = AppState::ServerConnected(pairing::ServerConnected::new(stream));
                }
                else {
                    self.ui_items.push(UIItem::Cancel);
                }
            }
            AppState::ServerConnected(_) => {
                self.ui_items.push(UIItem::Quit);
            }
            AppState::ClientConnected(_) => {
                self.ui_items.push(UIItem::Quit);
            }
        }

        let list_items = self.ui_items.iter().zip(0..).map(|(item, index)| {
            let mut li = item.to_list_item();
            if index == self.selected {
                li = li.bg(Color::Yellow);
            }
            li
        });

        let l = ratatui::widgets::List::new(list_items);
        let block = Block::default().borders(Borders::ALL);
        frame.render_widget(l.block(block), frame.size());
    }

    pub fn is_shut_down(&self) -> bool {
        self.closed
    }
}
