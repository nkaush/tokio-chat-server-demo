use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel, error::SendError},
    select, net::{TcpStream, TcpListener}, task::JoinHandle
};
use crate::client::{ClientData, ClientStateMessage, ClientStateMessageType, handle_client};
use std::{collections::HashMap, net::SocketAddr};
use tokio_util::codec::LengthDelimitedCodec;
use chat_common::{ClientName, ChatProtocol};
use futures::stream::StreamExt;
use log::{error, trace, info};

pub struct Server {
    tcp_listener: TcpListener,
    from_clients: UnboundedReceiver<ClientStateMessage>,
    clients: HashMap<ClientName, ClientHandle>,
    server_handle: ServerHandle
}

pub struct ClientHandle {
    client_name: ClientName,
    to_client: UnboundedSender<ChatProtocol>,
    handle: JoinHandle<()>
}

impl Drop for ClientHandle {
    fn drop(&mut self) { 
        info!("Aborting client handler thread for client: {}", self.client_name);
        self.handle.abort();
    }
}

#[derive(Clone)]
pub struct ServerHandle {
    to_server: UnboundedSender<ClientStateMessage>
}

impl ServerHandle {
    pub fn send(&self, message: ClientStateMessage) -> Result<(), SendError<ClientStateMessage>> {
        self.to_server.send(message)
    }
}

impl Server {
    pub async fn new(port: u16) -> Result<Self, tokio::io::Error> {
        let (to_server, from_clients) = unbounded_channel();

        let bind_addr: SocketAddr = ([0, 0, 0, 0], port).into();
        let tcp_listener = match TcpListener::bind(bind_addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("Failed to bind to {}: {:?}", bind_addr, e);
                return Err(e);
            }
        };

        trace!("Listening on {bind_addr}");

        Ok(Self {
            tcp_listener,
            from_clients,
            clients: HashMap::new(),
            server_handle: ServerHandle { to_server }
        })
    }

    async fn admit_client(&mut self, stream: TcpStream) {
        let (to_client, from_server) = unbounded_channel();
        let mut framed = LengthDelimitedCodec::builder()
                .length_field_type::<u32>()
                .new_framed(stream);

        // Listen for the first message from a client for the client name 
        if let Some(Ok(bytes)) = framed.next().await {
            // If we are able to deserialize the first message into the client 
            // name identifier, then admit the client into the server...
            if let Ok(ChatProtocol::ClientJoined(client_name)) = bincode::deserialize(&bytes) {
                let client_data = ClientData {
                    client_name: client_name.clone(),
                    from_server,
                    framed,
                    server_handle: self.server_handle.clone(),
                };

                // Spawn the cliet handler thread and add the client to our list
                // of subscribed clients
                let client_handle = ClientHandle {
                    client_name: client_name.clone(),
                    to_client,
                    handle: tokio::spawn(handle_client(client_data))
                };

                self.clients.insert(client_name, client_handle);
            } else {
                error!("failed to deserialize first message into ChatProtocol::ClientJoined(...)")
            }
        } else {
            error!("failed to receive first message from client")
        }
    }

    pub async fn serve(&mut self) {
        loop {
            select! {
                client = self.tcp_listener.accept() => match client {
                    Ok((stream, addr)) => {
                        trace!("Client joined from {}", addr);
                        self.admit_client(stream).await;
                    },
                    Err(e) => {
                        error!("Could not accept client: {:?}", e);
                        continue
                    }
                },
                Some(state) = self.from_clients.recv() => match state.msg {
                    ClientStateMessageType::Message(m) => {
                        let mut to_remove: Vec<String> = Vec::new();

                        for (client, handle) in self.clients.iter() {
                            // Do not send message to client who sent us the message!
                            if client == &state.client { continue }

                            if let Err(_) = handle.to_client.send(m.clone()) {
                                to_remove.push(client.clone());
                            }
                        }

                        // If there were any SendErrors encountered when sending
                        // the message to client handler threads, then remove
                        // those clients and abort their threads
                        for c in to_remove.into_iter() {
                            error!("Failed to pass message to thread for client {c} ... aborting.");
                            self.clients.remove(&c);
                        }
                    },
                    ClientStateMessageType::Error => { self.clients.remove(&state.client); }
                }
            }
        } 
    }
}