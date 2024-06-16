use tynkerbase_universal::{
    crypt_utils::{
        compression_utils, 
        rsa_utils, 
        BinaryPacket, 
        CompressionType, 
        RsaKeys
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
};
use rsa::{
    pkcs1::{DecodeRsaPublicKey, EncodeRsaPublicKey}, 
    RsaPrivateKey, 
    RsaPublicKey,
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

#[rocket::post("/create-proj?<name>", data="<data>")]
async fn create_proj(name: &str, data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let _packet = match packet {
        Ok(p) => p,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    let res = proj_utils::create_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("already exists") {
            return AgentResponse::cl_err_from(e);
        }
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")
}

#[rocket::post("/add-files-to-proj?<name>", data = "<data>")]
async fn add_files_to_proj(
    name: &str,
    data: Vec<u8>,
    state: &State<GlobalStateMx>,
) -> AgentResponse {

    let lock = state.lock().unwrap();
    let packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let packet = match packet {
        Ok(p) => p,
        Err(e) => return AgentResponse::cl_err_from(e),
    };
    
    let _ = proj_utils::clear_proj(&name);

    let files: FileCollection = bincode::deserialize(&packet.data).unwrap();

    if let Err(e) = proj_utils::add_files_to_proj(name, files) {
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")
}

#[rocket::post("/delete-proj?<name>", data="<data>")]
async fn delete_proj(name: &str, data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    if let Err(e) = in_packet {
        return AgentResponse::cl_err_from(e);
    }

    let res = proj_utils::delete_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("does not exist") {
            return AgentResponse::cl_err_from(e);
        }
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")
}

#[rocket::post("/get-files?<name>", data="<data>")]
fn get_proj_files(name: &str, data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);

    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    let fc = match proj_utils::load_proj_files(name, None) {
        Ok(fc) => fc,
        Err(e) => return AgentResponse::ag_err_from(e),
    };

    let mut out_packet = BinaryPacket::from(&fc).unwrap();
    if out_packet.data.len() > 5_000_000 {
        compression_utils::compress_brotli(&mut out_packet).unwrap();
    }

    let pub_key = RsaPublicKey::from_pkcs1_der(&in_packet.data).unwrap();
    rsa_utils::encrypt(&mut out_packet, &pub_key).unwrap();

    let payload = bincode::serialize(&out_packet).unwrap();
    
    AgentResponse::OkData(payload)
}

#[rocket::get("/get-pub-key")]
async fn emit_pubkey(state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = match state.lock() {
        Ok(l) => l,
        Err(e) => return AgentResponse::ag_err_from(format!("Error getting lock on key: {}", e)),
    };

    let pubkey = lock.rsa_keys.pub_key.to_pkcs1_der();
    let pubkey = match pubkey {
        Ok(p) => p.to_vec(),
        Err(e) => return AgentResponse::ag_err_from(format!("Error serializing key: {}", e)),
    };

    AgentResponse::OkData(pubkey)
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}

fn parse_req(v: &Vec<u8>, rsa_keys: &RsaKeys, target_key: Option<&str>) -> Result<BinaryPacket> {
    let mut packet: BinaryPacket = bincode::deserialize(v)
        .unwrap();

    if packet.is_encrypted {
        rsa_utils::decrypt(&mut packet, &rsa_keys.priv_key)
            .unwrap();
    }
    if let Some(target_key) = target_key {
        let auth_err = anyhow!("AUTHORIZATION ERROR");
        match packet.get_apikey() {
            Ok(k) => {
                if k != target_key {
                    return Err(auth_err)
                }
            }
            Err(_) => return Err(auth_err),
        }
    }

    match packet.compression_type {
        CompressionType::Brotli => compression_utils::decompress_brotli(&mut packet).unwrap(),
        _ => {},
    }

    Ok(packet)
}

#[rocket::post("/start-docker-daemon", data="<data>")]
fn start_docker_daemon(data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    if let Err(e) = docker_utils::start_daemon() {
        return AgentResponse::ag_err_from(e);
    }
    
    AgentResponse::Ok("success".to_string())
}

#[rocket::post("/end-docker-daemon", data="<data>")]
fn end_docker_daemon(data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    if let Err(e) = docker_utils::end_daemon() {
        return AgentResponse::ag_err_from(e);
    }
    
    AgentResponse::Ok("success".to_string())
}

#[rocket::post("/get-daemon-status", data="<data>")]
fn get_daemon_status(data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    let status = match docker_utils::get_engine_status() {
        Ok(b) => b,
        Err(e) => return AgentResponse::ag_err_from(e),
    };

    AgentResponse::OkData(vec![status as u8])
}

#[rocket::post("/build-img?<name>", data="<data>")]
fn build_image(name: &str, data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    let path = format!("{}/{}", proj_utils::LINUX_TYNKERBASE_PATH, name);
    let img_name = format!("{}_image", name);

    docker_utils::build_image(&path, &img_name)
        .unwrap();

    AgentResponse::Ok("success".to_string())
}

#[rocket::post("/spawn-container?<name>", data="<data>")]
fn spawn_container(name: &str, data: Vec<u8>, state: &State<GlobalStateMx>) -> AgentResponse {
    let lock = state.lock().unwrap();
    let in_packet = parse_req(&data, &lock.rsa_keys, Some(&lock.api_key));
    drop(lock);
    let in_packet = match in_packet {
        Ok(d) => d,
        Err(e) => return AgentResponse::cl_err_from(e),
    };
    
    let img_name = format!("{}_image", name);
    let container_name = format!("{}_container", name);
    docker_utils::start_container(&img_name, &container_name, vec![], vec![])
        .unwrap();


    AgentResponse::Ok("success".to_string())
}

#[derive(Debug)]
struct GlobalState {
    rsa_keys: RsaKeys,
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

    let rsa_keys = RsaKeys::new();

    let state = Arc::new(Mutex::new(GlobalState { 
        rsa_keys: rsa_keys,
        api_key: "placeholder".to_string(),
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
            spawn_container
        ])
        .mount("/security", routes![emit_pubkey])
        .manage(state)
}
