use super::CommunicationMessage;
use bincode::{Decode, Encode};
use wg_2024::network::NodeId;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerBody {
    RespServerType(ServerType),
    ErrUnsupportedRequestType,
    ServerContent(ServerContentBody),
    ServerCommunication(ServerCommunicationBody),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ServerType {
    Content,
    Communication,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerContentBody {
    RespFilesList(Vec<String>),
    RespFile(Vec<u8>),
    ErrFileNotFound,
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum FileType {
    Image,
    Text,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ServerCommunicationBody {
    RespClientList(Vec<NodeId>),
    MessageReceive(CommunicationMessage),
    ErrWrongClientId,
    ErrNotRegistered,
    RegistrationSuccess,
}
