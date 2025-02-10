use super::CommunicationMessage;
use bincode::{Decode, Encode};
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientBody {
    ReqServerType,
    ClientContent(ClientContentBody),
    ClientCommunication(ClientCommunicationBody),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientContentBody {
    ReqFilesList,
    ReqFile(String),
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Encode, Decode)]
pub enum ClientCommunicationBody {
    ReqRegistrationToChat,
    MessageSend(CommunicationMessage),
    ReqClientList,
}
