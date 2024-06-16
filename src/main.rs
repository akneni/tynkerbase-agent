use tynkerbase_universal::{
    crypt_utils::{
        compression_utils, 
        BinaryPacket, 
    }, 
    docker_utils, 
    proj_utils::{self, FileCollection}
};
use bincode;
use rocket::{
    self,
    http::Status,
    launch,
    response::{self, Responder, Response},
    routes,
    Request, 
    request::{self, FromRequest},
    outcome::Outcome,
    State,
};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::{
    sync::{Arc, Mutex},
    fs,
    path::Path,
    io::{self, Write},
};
use anyhow::{anyhow, Result};

#[derive(Debug, Serialize, Deserialize)]
enum AgentResponse {
    Ok(String),
    OkData(Vec<u8>),
    AgentErr(String),
    ClientErr(String),
}

impl AgentResponse {
    fn ok_from(x: impl Debug) -> Self {
        Self::Ok(format!("{:?}", x))
    }

    fn ag_err_from(x: impl Debug) -> Self {
        Self::AgentErr(format!("{:?}", x))
    }

    fn cl_err_from(x: impl Debug) -> Self {
        Self::ClientErr(format!("{:?}", x))
    }

    fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(&self).unwrap()
    }

    fn from_bytes(v: &Vec<u8>) -> Self {
        bincode::deserialize(v).unwrap()
    }
}

// Implement the `rocket::response::Responder` trait for this enum so it can be returned as an object
impl<'r> Responder<'r, 'static> for AgentResponse {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        match self {
            AgentResponse::Ok(msg) => Response::build()
                .status(Status::Ok)
                .sized_body(msg.len(), std::io::Cursor::new(msg))
                .ok(),
            AgentResponse::OkData(data) => Response::build()
                .status(Status::Ok)
                .sized_body(data.len(), std::io::Cursor::new(data))
                .ok(),
            AgentResponse::AgentErr(msg) => Response::build()
                .status(Status::InternalServerError)
                .sized_body(msg.len(), std::io::Cursor::new(msg))
                .ok(),
            AgentResponse::ClientErr(msg) => Response::build()
                .status(Status::BadRequest)
                .sized_body(msg.len(), std::io::Cursor::new(msg))
                .ok(),
        }
    }
}
struct ApiKey(String);

#[rocket::async_trait]
impl<'a> FromRequest<'a> for ApiKey {
    type Error = ();

    async fn from_request(req: &'a Request<'_>) -> request::Outcome<Self, Self::Error> {
        let target = fs::read_to_string("./keys/api-key.txt")
            .unwrap();

        match req.headers().get_one("tyb-api-key") {
            Some(key) if key == &target => Outcome::Success(ApiKey(key.to_string())),
            _ => Outcome::Error((Status::Forbidden, ())),
        }
    }
}

#[rocket::post("/create-proj?<name>")]
async fn create_proj(name: &str, #[allow(unused)] apikey: ApiKey) -> Vec<u8> {
    let res = proj_utils::create_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("already exists") {
            return AgentResponse::cl_err_from(e).to_bytes();
        }
        return AgentResponse::ag_err_from(e).to_bytes();
    }

    AgentResponse::ok_from("success").to_bytes()
}

#[rocket::post("/add-files-to-proj?<name>", data = "<data>")]
async fn add_files_to_proj(
    name: &str,
    data: Vec<u8>,
    #[allow(unused)] apikey: ApiKey,
) -> AgentResponse {   
    let _ = proj_utils::clear_proj(&name);
    
    let packet: BinaryPacket = bincode::deserialize(&data).unwrap();
    let files: FileCollection = bincode::deserialize(&packet.data).unwrap();

    if let Err(e) = proj_utils::add_files_to_proj(name, files) {
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")
}

#[rocket::get("/delete-proj?<name>")]
async fn delete_proj(name: &str, #[allow(unused)] apikey: ApiKey) -> AgentResponse {
    let res = proj_utils::delete_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("does not exist") {
            return AgentResponse::cl_err_from(e);
        }
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")
}

#[rocket::get("/get-files?<name>")]
fn get_proj_files(name: &str, #[allow(unused)] apikey: ApiKey) -> AgentResponse {
    let fc = match proj_utils::load_proj_files(name, None) {
        Ok(fc) => fc,
        Err(e) => return AgentResponse::ag_err_from(e),
    };

    let mut out_packet = BinaryPacket::from(&fc).unwrap();
    if out_packet.data.len() > 5_000_000 {
        compression_utils::compress_brotli(&mut out_packet).unwrap();
    }

    let payload = bincode::serialize(&out_packet).unwrap();
    
    AgentResponse::OkData(payload)
}

#[rocket::post("/start-docker-daemon")]
fn start_docker_daemon(#[allow(unused)] apikey: ApiKey) -> AgentResponse {
    if let Err(e) = docker_utils::start_daemon() {
        return AgentResponse::ag_err_from(e);
    }
    
    AgentResponse::Ok("success".to_string())
}

#[rocket::get("/end-docker-daemon")]
fn end_docker_daemon(#[allow(unused)] apikey: ApiKey) -> AgentResponse {
    if let Err(e) = docker_utils::end_daemon() {
        return AgentResponse::ag_err_from(e);
    }
    
    AgentResponse::Ok("success".to_string())
}

#[rocket::get("/get-daemon-status")]
fn get_daemon_status(#[allow(unused)] apikey: ApiKey) -> AgentResponse {
    let status = match docker_utils::get_engine_status() {
        Ok(b) => b,
        Err(e) => return AgentResponse::ag_err_from(e),
    };

    AgentResponse::OkData(vec![status as u8])
}

#[rocket::get("/build-img?<name>")]
fn build_image(name: &str, #[allow(unused)] apikey: ApiKey) -> AgentResponse {
    let path = format!("{}/{}", proj_utils::LINUX_TYNKERBASE_PATH, name);
    let img_name = format!("{}_image", name);

    docker_utils::build_image(&path, &img_name)
        .unwrap();

    AgentResponse::Ok("success".to_string())
}

#[rocket::get("/spawn-container?<name>")]
fn spawn_container(name: &str, #[allow(unused)] apikey: ApiKey) -> AgentResponse {
    let img_name = format!("{}_image", name);
    let container_name = format!("{}_container", name);
    docker_utils::start_container(&img_name, &container_name, vec![], vec![])
        .unwrap();


    AgentResponse::Ok("success".to_string())
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}

#[derive(Debug)]
struct GlobalState {
    api_key: String,
}

type GlobalStateMx = Arc<Mutex<GlobalState>>;

#[launch]
fn rocket() -> _ {
    // Create TynkerBase Directory
    let path_str = format!("/{}", proj_utils::LINUX_TYNKERBASE_PATH);
    let path =  Path::new(&path_str);
    if !path.exists() {
        fs::create_dir(path_str)
            .unwrap();
    }


    // handle api keys
    let path = Path::new("./keys/api-key.txt");
    let mut api_key = String::new();
    if !path.exists() {
        print!("Enter your API key: ");    // Prompt
        io::stdout().flush().unwrap();  // Flushes output buffer

        io::stdin().read_line(&mut api_key).expect("Failed to read line.");

        if !api_key.starts_with("tyb_key_") || api_key.len() < 64 {
            panic!("Incorrect format");
        }

        fs::write("./keys/api-key.txt", &api_key)
            .unwrap();
    }
    else {
        api_key = fs::read_to_string("./keys/api-key.txt")
            .unwrap();
    }

    let state = Arc::new(Mutex::new(GlobalState { 
        api_key: api_key,
    }));

    rocket::build()
        .mount("/", routes![root])
        .mount(
            "/proj",
            routes![create_proj, add_files_to_proj, delete_proj, get_proj_files],
        )
        .mount("/docker/daemon", routes![
            start_docker_daemon, 
            end_docker_daemon, 
            get_daemon_status
        ])
        .mount("/docker/imgs", routes![
            build_image,
            spawn_container,
        ])
        .manage(state)
}
