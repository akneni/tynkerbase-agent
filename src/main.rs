mod docker_wrapper;
mod proj_files;

use std::fmt::Debug;
use rocket::{
    self, 
    launch, 
    routes,
    Request,
    response::{
        self, 
        Responder,
        Response,
    },
    http::Status,
};
use serde::{Serialize, Deserialize};

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


#[rocket::get("/create-proj?<name>")]
async fn create_proj(name: &str) -> AgentResponse {
    let res = proj_files::create_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("already exists") {
            return AgentResponse::cl_err_from(e);
        }
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")

}

#[rocket::post("/add-files-to-proj?<name>", data = "<data>")]
async fn add_files_to_proj(name: &str, data: Vec<u8>) -> AgentResponse {
    let _ = proj_files::clear_proj(&name);

    let files = match proj_files::FileCollection::from_bytes(data) {
        Ok(f) => f,
        Err(e) => return AgentResponse::cl_err_from(e),
    };

    if let Err(e) = proj_files::add_files_to_proj(name, files) {
        return AgentResponse::ag_err_from(e);
    }
    
    AgentResponse::ok_from("success")
}

#[rocket::get("/delete-proj?<name>")]
async fn delete_proj(name: &str) -> AgentResponse {
    let res = proj_files::delete_proj(name);
    if let Err(e) = res {
        if e.to_string().contains("does not exist") {
            return AgentResponse::cl_err_from(e);
        }
        return AgentResponse::ag_err_from(e);
    }

    AgentResponse::ok_from("success")

}

#[rocket::get("/get-files?<name>")]
fn get_proj_files(name: &str) -> AgentResponse {
    match proj_files::load_proj_files(name, None) {
        Ok(fc) => AgentResponse::OkData(fc.to_bytes()),
        Err(e) => AgentResponse::ag_err_from(e),
    }
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}


#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", routes![root])
        .mount("/proj", routes![create_proj, add_files_to_proj, delete_proj, get_proj_files])
        .mount("/docker-daemon", routes![])
}

