use wg_2024::network::{NodeId, SourceRoutingHeader};

#[derive(Debug, Clone)]
pub struct Message {
    pub routing_header: SourceRoutingHeader,
    pub session_id: u64,
    pub body: MessageBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerType {
    Content,
    Communication,
}

#[derive(Debug, Clone)]
pub struct CommunicationMessage {
    pub from: NodeId,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum MessageBody {
    Client(ClientBody),
    Server(ServerBody),
}

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
    ReqClientList,
}

#[derive(Debug, Clone)]
pub enum ClientCommunicationBody {
    ReqRegistrationToChat,
    MessageSend(CommunicationMessage),
}

#[derive(Debug, Clone)]
pub enum ServerBody {
    RespServerType(ServerType),
    ErrUnsupportedRequestType,
    ServerContent(ServerContentBody),
    ServerCommunication(ServerCommunicationBody),
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
