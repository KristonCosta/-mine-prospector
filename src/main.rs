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
    use crate::{DEFAULT_MC_PORT};
    use std::path::{Path, PathBuf};

    pub enum MCError {
        FailedToCreateContainer,
        FailedToStartContainer(String),
        FailedToInspectContainer(String),
        FailedToStopContainer(String),
        FailedToRunCommand(String, MCServerCommands),
        FailedToRMContainer(String),
        ContainerError(String, String),
    }

    impl ToString for MCError {
        fn to_string(&self) -> String {
            match self {
                MCError::FailedToCreateContainer => {format!("failed to create container")},
                MCError::FailedToStartContainer(x) => {format!("failed to start container {}", x)},
                MCError::FailedToInspectContainer(x) => {format!("failed to inspect container {}", x)},
                MCError::FailedToStopContainer(x) => {format!("failed to stop container {}", x)},
                MCError::FailedToRunCommand(x, c) => {format!("failed to run command {:?} on container {}", c, x)},
                MCError::FailedToRMContainer(x) => {format!("failed to rm container {}", x)},
                MCError::ContainerError(_, e) => {format!("{}", e)},
            }
        }
    }

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

        pub fn create(&mut self, options: &MCServerOptions) -> Result<MCContainer, MCError> {
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

            let info: ContainerCreateInfo = self.runtime.block_on(runner).map_err(|e| {
                MCError::FailedToCreateContainer
            })?;

            if let Some(warnings) = info.warnings {
                for warning in warnings {
                    warn!("Warning [Container {}]: {}", info.id, warning);
                }
            }
            Ok(Self::get_container(info.id))
        }

        pub fn get_container(id: String) -> MCContainer {
            MCContainer {
                id
            }
        }


    }
    pub struct MCContainer {
        pub id: String,
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

        pub fn status(&mut self, container: &MCContainer) -> Result<(), ()> {
            Ok(())
        }

        pub fn start(&mut self, container: &MCContainer) -> Result<(), MCError> {
            info!("Starting container: {}", container.id);
            let ref container_id = container.id;
            let container = Container::new(&self.docker, container_id.clone());
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
                    }))
                .map_err(|e|
                    MCError::FailedToStartContainer(container_id.clone())
                )?;
            let container_info: ContainerDetails = self.runtime
                .block_on(
                    container.inspect())
                .map_err(|e| {
                    MCError::FailedToInspectContainer(container_id.clone())
                })?;
            if !container_info.state.error.is_empty() {
                error!("Error: {:?}", container_info.state);
                return Err(MCError::ContainerError(container_id.clone(), container_info.state.error));
            }
            Ok(())
        }

        pub fn stop(&mut self, container: &MCContainer) -> Result<(), MCError> {
            info!("Stopping container: {}", container.id);
            let ref container_id = container.id;
            let container = Container::new(&self.docker, container.id.clone());
            self.runtime.block_on(
                container
                    .stop(None)
            ).map_err(|_| {
                MCError::FailedToStopContainer(container_id.clone())
            })?;
            Ok(())
        }

        pub fn run_command(&mut self, container: &MCContainer, command: MCServerCommands) -> Result<(), MCError> {
            info!("Running command {:?} on container {}", command, container.id);
            let ref container_id = container.id;
            let command_for_error = command.clone();
            let container = Container::new(&self.docker, container.id.clone());
            use std::io::prelude::*;
            self.runtime.block_on(
                container
                .attach()
                .map(move |mut mul| {
                    mul.write_all(command.to_string().as_bytes());
                    mul.flush();
                })
            ).map_err(|_| {
                MCError::FailedToRunCommand(container_id.clone(), command_for_error)
            })?;
            Ok(())
        }

        pub fn logs(&mut self, container: &MCContainer, options: &MCServerLogOptions) -> Result<Vec<String>, ()> {
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
            Ok(logs.into_iter().map(|l| l.as_string_lossy()).collect())
        }

        pub fn rm(&mut self, container: &MCContainer) -> Result<(), MCError> {
            info!("Removing container: {}", container.id);
            let ref container_id = container.id;
            let container = Container::new(&self.docker, container.id.clone());
            let options = RmContainerOptions::builder()
                .force(true)
                .build();
            self.runtime.block_on(
                container
                    .remove(options)

            ).map_err(|e| {
                MCError::FailedToRMContainer(container_id.clone())
            })
            // Ok(())
        }
    }

    #[derive(Debug, Clone)]
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

mod server {
    use crate::service::{MCService, MCContainerService, MCServerOptionsBuilder};
    use rouille::Response;
    use std::path::PathBuf;
    use std::str::FromStr;

    pub struct Server;

    #[derive(Serialize)]
    struct BasicResponse {
        #[serde(skip_serializing_if = "Option::is_none")]
        success: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    }

    impl BasicResponse {
        pub fn success(response: String) -> Self {
            BasicResponse {
                success: Some(response),
                error: None,
            }
        }
        pub fn error(response: String) -> Self {
            BasicResponse {
                error: Some(response),
                success: None,
            }
        }
    }

    impl Server {
        pub fn new() -> Self {
            Server
        }

        pub fn run(&mut self) {

            info!("Listening on port 8081");
            rouille::start_server("localhost:8081", move |request| {
                info!("Processing incoming request");
                let mut mc_service = MCService::new();
                let mut container_service = MCContainerService::new();

                router!(request,
                    (POST) (/container) => {
                        let response = mc_service.create(&MCServerOptionsBuilder::new("SomeName".to_string(),
                                                                                      PathBuf::from_str("/Users/kristoncosta/workspace/tmp-mc")
                                                                                          .unwrap()).build());
                        match response {
                            Ok(x) => {Response::json(&BasicResponse::success(x.id))},
                            Err(e) => {
                                Response::json(&BasicResponse::error(e.to_string()))
                                    .with_status_code(400)
                            },
                        }
                    },
                    (POST) (/container/{id: String}/start) => {
                        let container = MCService::get_container(id);
                        let response = container_service.start(&container);
                        match response {
                            Ok(_) => {Response::json(&BasicResponse::success("success".to_string()))},
                            Err(e) => {
                                Response::json(&BasicResponse::error(e.to_string()))
                                    .with_status_code(400)
                                },
                        }
                    },
                    (POST) (/container/{id: String}/stop) => {
                        let container = MCService::get_container(id);
                        let response = container_service.stop(&container);
                        match response {
                            Ok(_) => {Response::json(&BasicResponse::success("success".to_string()))},
                            Err(e) => {
                                Response::json(&BasicResponse::error(e.to_string()))
                                    .with_status_code(400)
                                },
                        }
                    },
                    (DELETE) (/container/{id: String}) => {
                        let container = MCService::get_container(id);
                        let response = container_service.rm(&container);
                        match response {
                            Ok(_) => {Response::json(&BasicResponse::success("success".to_string()))},
                            Err(e) => {
                                Response::json(&BasicResponse::error(e.to_string()))
                                    .with_status_code(400)
                                },
                        }
                    },
                    _ => rouille::Response::empty_404()
                )
            });
        }
    }
}

fn main() {
    env_logger::init();
    let mut server = Server::new();
    info!("Starting server");
    server.run();
}