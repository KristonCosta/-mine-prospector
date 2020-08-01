extern crate termion;
extern crate shiplift;
extern crate rusqlite;
#[macro_use]
extern crate log;
extern crate env_logger;
#[macro_use]
extern crate rouille;
extern crate serde;
#[macro_use] extern crate serde_derive;


use crate::service::{MCServerLogOptions, MCServerCommands, MCServerOptionsBuilder, MCContainerService};

use crate::server::Server;

const DEFAULT_MC_PORT: u32 = 25565;

#[derive(Debug)]
pub struct MCWorker {
    id: u32,
    name: String,
    container: String,
    volume: String,
    status: String,
    port: u32
}

pub mod repository;

mod service;

mod server;

fn main() {
    env_logger::init();
    let mut server = Server::new();
    info!("Starting server");
    server.run();
}