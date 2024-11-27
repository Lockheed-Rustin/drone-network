use dn_message::ClientBody;

pub enum ClientCommand {
    SendMessage(ClientBody),
}
