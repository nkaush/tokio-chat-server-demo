use serde::{Deserialize, Serialize};

pub type ClientName = String;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ChatProtocol {
    Message(ClientName, String),
    ClientJoined(ClientName),
    ClientDisconnected(ClientName)
}