use tokio::{sync::mpsc::{error::SendError, UnboundedReceiver}, net::TcpStream, select};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use chat_common::{ChatProtocol, ClientName};
use futures::{stream::StreamExt, SinkExt};
use crate::server::ServerHandle;
use log::{trace, error};

#[derive(Debug)]
pub enum ClientStateMessageType {
    Error,
    Message(ChatProtocol)
}

#[derive(Debug)]
pub struct ClientStateMessage {
    pub(crate) client: ClientName,
    pub(crate) msg: ClientStateMessageType
}

pub struct ClientData {
    pub(crate) client_name: ClientName,
    pub(crate) server_handle: ServerHandle,
    pub(crate) framed: Framed<TcpStream, LengthDelimitedCodec>,
    pub(crate) from_server: UnboundedReceiver<ChatProtocol>
}

impl ClientData {
    fn generate_state_msg(&self, msg: ClientStateMessageType) -> ClientStateMessage {
        ClientStateMessage { client: self.client_name.clone(), msg }
    }
    
    fn notify_client_message(&mut self, msg: ChatProtocol) -> Result<(), SendError<ClientStateMessage>> {
        self.server_handle.send(self.generate_state_msg(ClientStateMessageType::Message(msg)))
    }

    fn notify_network_error(&mut self) -> Result<(), SendError<ClientStateMessage>> {
        self.server_handle.send(self.generate_state_msg(ClientStateMessageType::Error))
    }
}

pub async fn handle_client(mut client_data: ClientData) {
    loop {
        select! {
            Some(to_send) = client_data.from_server.recv() => {
                trace!("Sending message to {} : {:?}", client_data.client_name, to_send);
                let bytes = bincode::serialize(&to_send).unwrap();
                if client_data.framed.send(bytes.into()).await.is_err() {
                    error!("TCP send error on handler thread for client {}", client_data.client_name);
                    client_data.notify_network_error().unwrap();
                    return;
                }
            },
            received = client_data.framed.next() => match received {
                Some(Ok(bytes)) => {
                    let msg = match bincode::deserialize(&bytes) {
                        Ok(m) => m,
                        Err(e) => {
                            error!("Deserialize error on client handler {}: {:?}", client_data.client_name, e);
                            continue
                        }
                    };

                    trace!("Got message from {} : {:?}", client_data.client_name, msg);
                    client_data.notify_client_message(msg).unwrap();
                },
                _ => {
                    error!("TCP receive error on handler thread for client {}", client_data.client_name);
                    client_data.notify_network_error().unwrap();
                    return;
                }
            }
        }
    }
}