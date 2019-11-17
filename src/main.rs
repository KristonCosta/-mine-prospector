extern crate termion;
extern crate shiplift;
extern crate rusqlite;
#[macro_use] extern crate log;
extern crate env_logger;

use crate::service::{MCServerLogOptions, MCServerCommands, MCServerOptionsBuilder, MCContainerService};
use std::path::PathBuf;
use std::str::FromStr;

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

    use shiplift::{Docker, Container, LogsOptions, Error, RmContainerOptions};
    use shiplift::builder::{ContainerOptions, RmContainerOptionsBuilder};
    use shiplift::rep::{ContainerCreateInfo, ContainerDetails};

    use tokio::prelude::{Future, Stream};
    use tokio::runtime::Runtime;

    use log::{info, warn};
    use crate::DEFAULT_MC_PORT;
    use std::path::{Path, PathBuf};

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
            let volume_path = options.volume.as_path().to_str().expect("unable to load path");
            let options = ContainerOptions::builder(self.image.as_ref())
                .env(vec!["EULA=TRUE"])
                .volumes(vec![&format!("{}:/data", volume_path)])
                .attach_stdin(true)
                .expose(options.port, "tcp", DEFAULT_MC_PORT)
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

        pub fn get_container(id: String) -> MCContainer {
            MCContainer {
                id
            }
        }


    }
    pub struct MCContainer {
        id: String,
    }

    pub struct MCContainerService {
        repo: MCRepository,
        docker: Docker,
        runtime: Runtime,
    }
    impl MCContainerService {
        pub fn new() -> Self {
            MCContainerService {
                docker: Docker::host("http://localhost:2375".parse().unwrap()),
                repo: MCRepository::new(),
                runtime: Runtime::new().expect("failed to make tokio runtime"),
            }
        }

        pub fn start(&mut self, container: &MCContainer) -> Result<(), ()> {
            info!("Starting container: {}", container.id);
            let container = Container::new(&self.docker, container.id.clone());
            self.runtime.block_on(
                container.start()
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
                    })).expect("couldn't start container");
            let container_info: ContainerDetails = self.runtime.block_on(container.inspect()).expect("");
            if !container_info.state.error.is_empty() {
                error!("Error: {:?}", container_info.state);
                return Err(());
            }
            Ok(())
        }

        pub fn stop(&mut self, container: &MCContainer) -> Result<(), ()> {
            info!("Stopping container: {}", container.id);
            let container = Container::new(&self.docker, container.id.clone());
            self.runtime.block_on(
                container
                    .stop(None)
                    .map_err(|e| error!("{}", e))
            )
        }

        pub fn run_command(&mut self, container: &MCContainer, command: MCServerCommands) -> Result<(), ()> {
            info!("Running command {:?} on container {}", command, container.id);
            let container = Container::new(&self.docker, container.id.clone());
            use std::io::prelude::*;
            self.runtime.block_on(
                container
                .attach()
                .map(move |mut mul| {
                    mul.write_all(command.to_string().as_bytes());
                    mul.flush();
                })
                .map_err(|e| error!("Error: {:?}", e))
            )
        }

        pub fn logs(&mut self, container: &MCContainer, options: &MCServerLogOptions) {
            info!("Logging container: {}", container.id);
            let container = Container::new(&self.docker, container.id.clone());
            let log_runner = container.logs(&LogsOptions::builder()
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
        pub fn rm(&mut self, container: &MCContainer) -> Result<(), ()> {
            info!("Removing container: {}", container.id);
            let container = Container::new(&self.docker, container.id.clone());
            let options = RmContainerOptions::builder()
                .force(true)
                .build();
            self.runtime.block_on(
                container
                    .remove(options)
                    .map_err(|e| error!("{}", e))
            )
            // Ok(())
        }
    }

    #[derive(Debug)]
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
        volume: PathBuf,
        port: u32,
        name: String,
    }

    pub struct MCServerOptionsBuilder {
        name: String,
        port: u32,
        volume: PathBuf
    }

    impl MCServerOptionsBuilder {

        pub fn new(name: String, volume: PathBuf) -> Self {
            MCServerOptionsBuilder {
                name,
                volume,
                port: DEFAULT_MC_PORT,
            }
        }

        pub fn port(mut self, port: u32) -> Self {
            self.port = port;
            self
        }

        pub fn build(self) -> MCServerOptions {
            MCServerOptions {
                volume: self.volume,
                port: self.port,
                name: self.name,
            }
        }
    }
}

fn main() {
    use crate::service::MCService;
    env_logger::init();
    use std::{thread, time};
    let path = PathBuf::from_str("/Users/kristoncosta/Workspace/PalaceRetreat").expect("couldn't make pathbuf");
    let server_options = MCServerOptionsBuilder::new("test_name".to_string(), path).build();
    let mut serv = MCService::new();
    let id = serv.create(&server_options).expect("couldn't make container");
    let mut container = MCService::get_container(id);

    let mut container_service = MCContainerService::new();
    container_service.start(&container).expect("Failed to start");
    container_service.logs(&container, &Default::default());
    container_service.run_command(&container, MCServerCommands::OP("Sylent_Buck".to_string()));
    thread::sleep(time::Duration::from_millis(5000));
    container_service.logs(&container, &Default::default());
    container_service.stop(&container).expect("couldn't stop container");
    container_service.rm(&container).expect("couldn't rm");
}