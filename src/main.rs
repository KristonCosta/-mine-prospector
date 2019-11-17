extern crate termion;
extern crate shiplift;
extern crate rusqlite;
#[macro_use] extern crate log;
extern crate env_logger;

use crate::service::{MCServerLogOptions, MCServerCommands};


#[derive(Debug)]
pub struct MCWorker {
    id: u32,
    name: String,
    container: String,
    volume: String,
    status: String,
    port: u32
}

pub mod repository {
    use rusqlite::Connection;
    pub struct MCRepository {
        conn: Connection,
    }

    impl MCRepository {
        pub fn new() -> Self {
            let conn = Connection::open("default.db").expect("Couldn't open db connection");
            MCRepository {
                conn
            }
        }
    }
}

mod service {
    use crate::repository::MCRepository;

    use shiplift::{Docker, Container, LogsOptions, Error};
    use shiplift::builder::{ContainerOptions};
    use shiplift::rep::{ContainerCreateInfo, ContainerDetails};

    use tokio::prelude::{Future, Stream};
    use tokio::runtime::Runtime;

    use log::{info, warn};

    pub struct MCService {
        repo: MCRepository,
        image: String,
        docker: Docker,
        runtime: Runtime,
    }
    impl MCService {
        pub fn new() -> Self {
            MCService {
                docker: Docker::host("http://localhost:2375".parse().unwrap()),
                repo: MCRepository::new(),
                image: "itzg/minecraft-server".to_string(),
                runtime: Runtime::new().expect("failed to make tokio runtime"),
            }
        }

        pub fn create(&mut self, options: &MCServerOptions) -> Result<String, ()> {
            let options = ContainerOptions::builder(self.image.as_ref())
                .env(vec!["EULA=TRUE"])
                .volumes(vec![&format!("{}:/data/world", options.volume)])
                .attach_stdin(true)
                .expose(options.port, "tcp", 25565)
                .build();

            let runner = self.docker
                .containers()
                .create(&options)
                .map(move |info| return info)
                .map_err(|e| eprintln!("Error: {}", e));
            let info: ContainerCreateInfo = self.runtime.block_on(runner).expect("failed to create container");
            if let Some(warnings) = info.warnings {
                for warning in warnings {
                    warn!("Warning [Container {}]: {}", info.id, warning);
                }
            }

            Ok(info.id)
        }

        pub fn get(service: &MCService, id: String) -> Result<MCContainer, ()> {
            info!("entering get");
            Ok(MCContainer{
                container: Container::new(&service.docker, id),
                runtime: Runtime::new().expect("failed to make tokio runtime"),

            })
        }
    }
    pub struct MCContainer<'a, 'b> {
        container: Container<'a, 'b>,
        runtime: Runtime,
    }

    impl <'a, 'b> MCContainer<'a, 'b> {

        pub fn start(&mut self) -> Result<(), ()> {
            self.runtime.spawn(
                self.container.start()
                    .map_err(|e| {
                        match e {
                            Error::Fault {
                                code,
                                ..
                            } => {
                                if code.is_client_error() || code.is_server_error() {
                                    error!("{}", e);
                                }
                            },
                            _ => {error!("{}", e)},
                        }
                    }));
            let container_info: ContainerDetails = self.runtime.block_on(self.container.inspect()).expect("");
            if !container_info.state.error.is_empty() {
                error!("Error: {:?}", container_info.state);
                return Err(());
            }
            return Ok(())
        }

        pub fn run_command(&mut self, command: MCServerCommands) {
            use std::io::prelude::*;
            let x = self.container
                .attach()
                .map(move |mut mul| {
                    mul.write_all(command.to_string().as_bytes());
                    mul.flush();
                })
                .map_err(|e| error!("Error: {:?}", e));
            self.runtime.spawn(x);

        }

        pub fn logs(&mut self, options: &MCServerLogOptions) {
            let log_runner = self.container.logs(&LogsOptions::builder()
                .stderr(true)
                .stdout(true)
                .tail(&options.limit)
                .build());
            let logs: Vec<_> = self.runtime.block_on(
                log_runner.collect()
                .map(|res| return res)
                .map_err(|e| error!("Error: {:?}", e))
            ).expect("");

            for log in logs {
                info!("{}", log.as_string_lossy());
            }
        }
    }

    pub enum MCServerCommands {
        OP(String),
    }

    impl ToString for MCServerCommands {
        fn to_string(&self) -> String {
            match self {
                MCServerCommands::OP(name) => { format!("/op {}\n", name) },
            }
        }
    }

    pub struct MCServerLogOptions {
        limit: String,
    }

    impl Default for MCServerLogOptions {
        fn default() -> Self {
            MCServerLogOptions { limit: "20".to_string() }
        }
    }


    pub struct MCServerOptions {
        volume: String,
        port: u32,
        name: String,
    }

    pub struct MCServerOptionsBuilder {
        name: String,
        port: u32,
    }
}

fn main() {
    use crate::service::MCService;
    env_logger::init();
    use std::{thread, time};

    let mut serv = MCService::new();
    let mut container = MCService::get(
        &serv, "a59d11a008629358463dcd1094488d626863df8831f31b6f73bfb8b60c426e65".to_string())
        .expect("Couldn't get container");
    container.start().expect("Failed to start");
    container.logs(&Default::default());
    container.run_command(MCServerCommands::OP("Sylent_Buck".to_string()));
    thread::sleep(time::Duration::from_millis(100));
    container.logs(&Default::default());

}