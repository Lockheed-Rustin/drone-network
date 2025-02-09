pub mod assembler;
mod client;
mod server;

pub use client::*;
pub use server::*;
pub use assembler::*;

use bincode::{Decode, Encode};
use wg_2024::network::NodeId;

#[derive(Debug, Clone, Encode, Decode)]
pub enum Message {
    Client(ClientBody), // comes from Client
    Server(ServerBody), // comes from Server
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CommunicationMessage {
    pub from: NodeId, // source Client
    pub to: NodeId,   // destination Client
    pub message: String,
}
