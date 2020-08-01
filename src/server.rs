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
