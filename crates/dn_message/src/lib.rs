#![allow(clippy::module_name_repetitions)]

pub mod assembler;
mod client;
mod server;

pub use assembler::*;
pub use client::*;
pub use server::*;

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
