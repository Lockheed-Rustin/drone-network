use super::CommunicationMessage;
use bincode::{Decode, Encode};

#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientBody {
    ReqServerType,
    ClientContent(ClientContentBody),
    ClientCommunication(ClientCommunicationBody),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientContentBody {
    ReqFilesList,
    ReqFile(String),
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientCommunicationBody {
    ReqRegistrationToChat,
    MessageSend(CommunicationMessage),
    ReqClientList,
}
