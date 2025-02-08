use super::CommunicationMessage;
use bincode::{Decode, Encode};
use wg_2024::network::NodeId;

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerBody {
    RespServerType(ServerType),
    ErrUnsupportedRequestType,
    ServerContent(ServerContentBody),
    ServerCommunication(ServerCommunicationBody),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ServerType {
    Content,
    Communication,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerContentBody {
    RespFilesList(Vec<String>),
    RespFile(Vec<u8>, FileType, Vec<String>), //file, type file, list of linked files
    ErrFileNotFound,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum FileType {
    Image,
    Text,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerCommunicationBody {
    RespClientList(Vec<NodeId>),
    MessageReceive(CommunicationMessage),
    ErrWrongClientId,
    ErrNotRegistered,
    RegistrationSuccess,
}
