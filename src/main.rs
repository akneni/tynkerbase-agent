mod consts;
mod dep_utils;
mod tls_utils;

use tynkerbase_universal::{
    crypt_utils::{
        self, compression_utils, hash_utils, BinaryPacket 
    }, 
    docker_utils, 
    proj_utils::{self, FileCollection}
};
use bincode;
use rocket::{
    self,
    launch,
    routes,
    Request, 
    request::{self, FromRequest},
    outcome::Outcome,
    response::{
        status::Custom,
        stream::TextStream,
    },
    http::Status,
    config::Config,
    figment::{Figment, providers::{Format, Toml, Env}},
};

use std::{
    io::{BufReader, BufRead}, 
    process::{self, Command, Stdio},
};

// Suppress warning being thrown since OS is only used in release mode
#[allow(unused_imports)]
use std::{
    fs,
    path::Path,
    env::consts::OS,
};

use once_cell::sync::Lazy;

static API_KEY: Lazy<String> = Lazy::new(|| {
    const ENDPOINT: &str = "https://tynkerbase-server.shuttleapp.rs";
    let rt = tokio::runtime::Runtime::new()
        .expect("Unable to generate new tokio runtime.");

    let email = crypt_utils::prompt("Enter your email: ");
    let password = crypt_utils::prompt_secret("Enter your password: ");

    let pass_sha256 = hash_utils::sha256(&password);
    let pass_sha384 = hash_utils::sha384(&password);

    let endpoint = format!("{}/auth/login?email={}&pass_sha256={}", ENDPOINT, &email, &pass_sha256);

    let f = reqwest::get(&endpoint);
    let res = rt.block_on(f)
        .expect("Error sending response");

    let salt = rt.block_on(res.text())
        .expect("unable to extract text from response");

    crypt_utils::gen_apikey(&pass_sha384, &salt)
});

struct ApiKey(
    #[allow(unused)]
    String
);

#[rocket::async_trait]
impl<'a> FromRequest<'a> for ApiKey {
    type Error = ();

    async fn from_request(req: &'a Request<'_>) -> request::Outcome<Self, Self::Error> {
        match req.headers().get_one("tyb-api-key") {
            Some(key) if key == (&*API_KEY) => Outcome::Success(ApiKey(key.to_string())),
            _ => Outcome::Error((Status::Forbidden, ())),
        }
    }
}

#[rocket::post("/create-proj?<name>")]
async fn create_proj(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let res = proj_utils::create_proj(name);
    if let Err(e) = res {
        let e = e.to_string();
        if e.contains("already exists") {
            return Custom(Status::Conflict, format!("User Already Exists -> {e}"));
        }
        return Custom(Status::InternalServerError, e);
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::post("/add-files-to-proj?<name>", data = "<data>")]
async fn add_files_to_proj(
    name: &str,
    data: Vec<u8>,
    #[allow(unused)] apikey: ApiKey,
) -> Custom<String> {   
    let _ = proj_utils::clear_proj(&name);
    
    let packet: BinaryPacket = bincode::deserialize(&data).unwrap();
    let files: FileCollection = bincode::deserialize(&packet.data).unwrap();

    if let Err(e) = proj_utils::add_files_to_proj(name, files) {
        return Custom(Status::InternalServerError, format!("Error adding files to project -> {e}"));
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/delete-proj?<name>")]
async fn delete_proj(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let res = proj_utils::delete_proj(name);
    if let Err(e) = res {
        let e = e.to_string();
        if e.contains("does not exist") {
            return Custom(Status::Conflict, format!("Project does not exist -> {e}"));
        }
        return Custom(Status::InternalServerError, e);
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/get-files?<name>")]
fn get_proj_files(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<Vec<u8>> {
    let fc = match proj_utils::load_proj_files(name, None) {
        Ok(fc) => fc,
        Err(e) => {
            return Custom(
                Status::InternalServerError, 
                format!("Error starting docker daemon -> {e}").as_bytes().to_vec()
            );
        },
    };

    let mut out_packet = BinaryPacket::from(&fc).unwrap();
    if out_packet.data.len() > 5_000_000 {
        compression_utils::compress_brotli(&mut out_packet).unwrap();
    }

    let payload = bincode::serialize(&out_packet).unwrap();
    
    Custom(Status::Ok, payload)
}

#[rocket::post("/start-docker-daemon")]
fn start_docker_daemon(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    if let Err(e) = docker_utils::start_daemon() {
        return Custom(Status::InternalServerError, format!("Error starting docker daemon -> {e}"));
    }
    
    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/end-docker-daemon")]
fn end_docker_daemon(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    if let Err(e) = docker_utils::end_daemon() {
        return Custom(Status::InternalServerError, format!("Failed to end daemon: {e}"));
    }
    
    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/get-daemon-status")]
fn get_daemon_status(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let status = match docker_utils::get_engine_status() {
        Ok(b) => b,
        Err(e) => return Custom(Status::Ok, format!("Error getting daemon status: {}", e)),
    };

    Custom(Status::Ok, status.to_string())
}

#[rocket::get("/build-img?<name>")]
fn build_image(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let path = format!("{}/{}", proj_utils::LINUX_TYNKERBASE_PATH, name);
    let img_name = format!("{}_image", name);

    docker_utils::build_image(&path, &img_name)
        .unwrap();

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/spawn-container?<name>")]
fn spawn_container(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let img_name = format!("{}_image", name);
    let container_name = format!("{}_container", name);
    if let Err(e) = docker_utils::start_container(&img_name, &container_name, vec![], vec![]) {
        return Custom(Status::InternalServerError, format!("Failed to start container -> {e}"));
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/halt-container?<name>")]
async fn halt_container(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let img_name = format!("{}_image", name);
    let container_name = format!("{}_container", name);

    if let Err(e) = docker_utils::start_container(&img_name, &container_name, vec![], vec![]) {
        return Custom(Status::InternalServerError, format!("Failed to start container -> {e}"));
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/install-docker")]
async fn install_docker() -> TextStream![String] {
    TextStream! {
        yield "Installing Docker...\n\n".to_string();

        let cmd = dep_utils::docker::get_install_command().unwrap();

        let mut child = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start command");

        let stdout = child.stdout.take();
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
    
            while let Some(Ok(l)) = reader.next() {
                yield l;
            }
            yield "\nFinished docker installation".to_string();        
        }
        else {
            yield "Failed to extract stdout from docker install process".to_string();
        }
    }
}

#[rocket::get("/install-openssl")]
async fn install_openssl() -> TextStream![String] {
    TextStream! {
        yield "Installing OpenSSL...\n\n".to_string();

        let cmd = dep_utils::openssl::get_install_command().unwrap();

        let mut child = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start command");

        let stdout = child.stdout.take();
        if let Some(stdout) = stdout {
            let mut reader = BufReader::new(stdout).lines();
    
            while let Some(Ok(l)) = reader.next() {
                yield l;
            }
            yield "\nFinished OpenSSL installation".to_string();        
        }
        else {
            yield "Failed to extract stdout from OpenSSL install process".to_string();
        }
    }
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}


#[launch]
fn rocket() -> _ {
    // Ensure we're running on linux
    #[cfg(not(debug_assertions))] {
        if OS != "linux" {
            println!("Unfortunately, we only support linux at the current time.");
            process::exit(0);
        }
    }
    // Create TynkerBase Directory
    #[cfg(not(debug_assertions))] {
        let path_str = format!("/{}", proj_utils::LINUX_TYNKERBASE_PATH);
        let path =  Path::new(&path_str);
        if !path.exists() {
            if let Err(e) = fs::create_dir(path_str) {
                if e.to_string().contains("Permission denied") {
                    println!("TynkerBase Agent needs root privileges. Please re-run with `sudo`");
                    std::process::exit(0);
                }
            }
        }
    }

    // Load API key
    Lazy::force(&API_KEY);

    // Ensure TLS keys and certificates are ready
    let root_dir = consts::AGENT_ROOTDIR_PATH;
    if !tls_utils::check_tls_cert(root_dir) {
        if !dep_utils::openssl::check_openssl() {
            println!("In order to enable TLS encryption, you need to install open ssl. (If it's already installed, try restarting the terminal)");
            let res = crypt_utils::prompt("Would you like to do that now? (y/n): ");
            if res.to_ascii_lowercase() == "y" {
                if let Err(e) = dep_utils::openssl::install_openssl() {
                    println!("Failed to install OpenSSL, install manually.\nError -> {}", e);
                    process::exit(1);
                }
                else {
                    println!("Successfully installed OpenSSL!");
                }
            }
            process::exit(0);
        }
        if let Err(e) = tls_utils::gen_tls_cert(root_dir) {
            println!("Error:\n{}", e);
            std::process::exit(1);
        }
    }

    // Ensure docker is installed
    if !dep_utils::docker::check_docker() {
        println!("Docker is not installed. (If it's already installed, try restarting the terminal)");
        let res = crypt_utils::prompt("Install docker now? (y/n): ");
        if res.to_ascii_lowercase() == "y" {
            if let Err(e) = dep_utils::docker::install_docker() {
                println!("Failed to install Docker, install manually.\nError -> {}", e);
                process::exit(1);
            }
            else {
                println!("Successfully installed Docker!");
            }
        }
        process::exit(0);
    }

    // Specify the path to Rocket.toml
    let r_toml_path = format!("{}/Rocket.toml", consts::AGENT_ROOTDIR_PATH);
    let r_toml_path = Path::new(&r_toml_path);
    let figment = Figment::from(Config::default())
        .merge(Toml::file(r_toml_path))
        .merge(Env::prefixed("ROCKET_"));

    rocket::custom(figment)
        .mount("/", routes![root])
        .mount(
            "/files/proj",
            routes![create_proj, add_files_to_proj, delete_proj, get_proj_files],
        )
        .mount("/docker/daemon", routes![
            start_docker_daemon, 
            end_docker_daemon, 
            get_daemon_status
        ])
        .mount("/docker/proj", routes![
            build_image,
            spawn_container,
            halt_container,
        ])
        .mount("/dependencies", routes![
            install_docker,
            install_openssl,
        ])
}
