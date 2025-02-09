# Drone Network

This repository, **drone-network**, contains several **crates** that manage different aspects of how the drone network is initialized, 
managed, and how clients and servers interact within the system.

## Repository Structure

The `src` folder contains only the `lib.rs` file, which exposes the `dn_internal` crate.

### Main Crates

This repository is organized into the following **crates**, each serving a specific role:

- **dn_internal**: Acts as an aggregator, exposing the other crates.

- **dn_server**: Contains the code for two different types of servers that operate at the edges of the drone network, providing 
various functionalities:
  - **Content Server**.
  - **Communication Server**.

- **dn_client**: Implements the client for the drone network. Clients can utilize server functionalities.

- **dn_message**: Defines the structure of messages exchanged between clients and servers. It also contains the Assembler structure, 
which allows you to fragment a message and re-assemble a fragmented message.

- **dn_network**: Manages the initialization of the communication network.

- **dn_controller**: Contains the `SimulationController`, which acts as a back-end exposing APIs to interact with the simulation 
controller. *(Developed by Lorenzo Ferranti)*

## Content Server

*This is Daniele Di Cesare individual contribution*

## Communication Server

*This is Luca Agostinelli individual contribution*

The **Communication Server** code is contained within the `dn_server/src/communication_server` directory.

The **main file** is `communication_server_main.rs`, which defines the `CommunicationServer` struct and its two public functions:

- `pub fn new(...) -> Self`
- `pub fn run(&mut self)`

All other methods are implemented in separate files within the `handlers` directory. These methods handle different aspects of server functionality, such as:
- **Message Handling** (`handle_messages.rs`): Manages the receipt, processing, and forwarding of messages between clients.
- **Packet Handling** (`handle_packet.rs`): Handles individual network packets.
- **NACK Handling** (`handle_nack.rs`): Defines the actions to be taken when negative acknowledgments are received.
- **Command Handling** (`handle_command.rs`): Processes commands sent to the communication server by the SC.
- **Flood Handling** (`handle_flood.rs`): Implements message flooding mechanisms within the network.
- **Client Services** (`handle_server_services.rs`): Manages requests for sever type, client registration, retrieval 
of registered clients and message forwarding.

The **CommunicationServer** internally uses additional structs:
- `Assembler` (defined in the `dn_message` crate)
- `CommunicationServerNetworkTopology` - Manages the network topology, representing nodes and connections as a graph. 
It supports routing and maintains saved paths for efficient communication.
- `PendingMessagesQueue` - Stores messages that could not be sent due to the lack of a known path. Once a path is discovered, 
these messages are retrieved and forwarded.
- `SessionManager` - Manages active communication sessions, tracking message fragments, acknowledgments, and session identifiers.

The file `test_server_helper.rs` contains the `TestServerHelper` struct, which is used exclusively for unit testing. 
Most of the files in this module have associated unit tests to verify the implemented functionalities.

## Client

*This is Matteo Zendri individual contribution*

## Contributors
- **Luca Agostinelli** - Communication Server, Assembler
- **Daniele Di Cesare** - Content Server
- **Matteo Zendri** - Client
- **Lorenzo Ferranti** - Simulation Controller

## Usage

todo
