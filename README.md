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

This is Daniele Di Cesare individual contribution

## Communication Server

This is Luca Agostinelli individual contribution

## Client

This is Matteo Zendri individual contribution

## Contributors
- **Luca Agostinelli** - Communication Server, Assembler
- **Daniele Di Cesare** - Content Server
- **Matteo Zendri** - Client
- **Lorenzo Ferranti** - Simulation Controller

## Usage

todo
