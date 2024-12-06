use wg_2024::network::NodeId;

#[derive(Debug, Clone)]
pub enum Message {
    Client(ClientBody), // comes from Client
    Server(ServerBody), // comes from Server
}

// --- Server ---

#[derive(Debug, Clone)]
pub enum ServerBody {
    RespServerType(ServerType),
    ErrUnsupportedRequestType,
    ServerContent(ServerContentBody),
    ServerCommunication(ServerCommunicationBody),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerType {
    Content,
    Communication,
}

#[derive(Debug, Clone)]
pub enum ServerContentBody {
    RespFilesList(Vec<String>),
    RespFile(Vec<u8>),
    ErrFileNotFound,
}

#[derive(Debug, Clone)]
pub enum ServerCommunicationBody {
    RespClientList(Vec<NodeId>),
    MessageReceive(CommunicationMessage),
    ErrWrongClientId,
}

// --- Client ---
#[derive(Debug, Clone)]
pub enum ClientBody {
    ReqServerType,
    ClientContent(ClientContentBody),
    ClientCommunication(ClientCommunicationBody),
}

#[derive(Debug, Clone)]
pub enum ClientContentBody {
    ReqFilesList,
    ReqFile(String),
}

#[derive(Debug, Clone)]
pub enum ClientCommunicationBody {
    ReqRegistrationToChat,
    MessageSend(CommunicationMessage),
    ReqClientList,
}

#[derive(Debug, Clone)]
pub struct CommunicationMessage {
    pub from: NodeId, // source Client
    pub to: NodeId,   // destination Client
    pub message: String,
}
