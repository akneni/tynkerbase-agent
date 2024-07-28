mod consts;
mod dep_utils;
mod diagnostics;
mod docker_utils;
mod global_state;
mod ngrok_utils;
mod proj_utils;
mod tls_utils;

use anyhow::anyhow;
use bincode;
use consts::{AGENT_ROOTDIR_PATH, SERVER_ENDPOINT, CONTAINER_MOD, IMAGE_MOD};
use global_state::{GlobalState, TsGlobalState};
use rand::{thread_rng, Rng};
use rocket::{
    self, 
    catchers, 
    config::{Config, TlsConfig}, 
    data::{Limits, ToByteUnit}, 
    figment::Figment, 
    http::Status, 
    launch, 
    outcome::Outcome, 
    request::{self, FromRequest}, 
    response::status::Custom, 
    routes, 
    Request,
};

use tynkerbase_universal::{
    constants::{LINUX_TYNKERBASE_PATH, TYB_APIKEY_HTTP_HEADER},
    crypt_utils::{self, compression_utils, hash_utils, BinaryPacket},
    file_utils::FileCollection,
    netwk_utils::ProjConfig,

};

use std::{
    env,
    process,
    path::PathBuf,
    sync::OnceLock,
};

// Suppress warning being thrown since OS is only used in release mode
#[allow(unused_imports)]
use std::{env::consts::OS, fs, path::Path};

async fn load_apikey(res: reqwest::Response, pass_sha384: &str) -> String {
    if !res.status().is_success() {
        println!("Error: Failed to login to server: {:#?}", res);
        process::exit(1);
    }

    let salt = res
        .text()
        .await
        .expect("unable to extract text from response");

    crypt_utils::gen_apikey(pass_sha384, &salt)
}

async fn prompt_node_name(email: &str, pass_sha256: &str) -> String {
    loop {
        let name = crypt_utils::prompt("Enter a name for this node: ");

        let endpoint = format!(
            "{SERVER_ENDPOINT}/ngrok/check-node-exists/name?\
            email={email}&\
            pass_sha256={pass_sha256}&\
            name={}",
            &name
        );

        let res = reqwest::get(endpoint).await.unwrap();

        if !res.status().is_success() {
            println!("Error communicating with database. This is an error with tynkerbase.");
            #[cfg(debug_assertions)]
            {
                println!("Error Res -> \n{:?}\n\n", res);
            }
            process::exit(1);
        }

        let text = res.text().await.unwrap();
        if text == "false" {
            return name;
        }

        println!("\n\nError: node name `{}` already exists: ", name);
    }
}

async fn load_node_info(email: &str, pass_sha256: &str) -> (String, String) {
    // Generate/read Server ID & name
    let id_len = 32;
    let mut path_str = format!("{}/data", AGENT_ROOTDIR_PATH);
    let path = Path::new(&path_str);
    if !path.exists() {
        fs::create_dir_all(&path).unwrap();
    }
    path_str.push_str("/node-info.bin");
    let path = Path::new(&path_str);

    if path.exists() {
        let res = fs::read(&path_str).unwrap();
        match bincode::deserialize(&res) {
            Ok(r) => return r,
            _ => {
                fs::remove_file(&path_str).unwrap();
            }
        }
    }

    let id = (0..id_len)
        .map(|_| thread_rng().gen_range(97..97 + 26) as u8)
        .collect();
    let id = String::from_utf8(id).unwrap();
    fs::write(&path_str, &id).unwrap();
    let name = prompt_node_name(email, pass_sha256);
    let name = name.await;

    let result = (id, name);
    let binary = bincode::serialize(&result).unwrap();
    fs::write(path, binary).unwrap();
    result
}

struct ApiKey(#[allow(unused)] String);

#[rocket::async_trait]
impl<'a> FromRequest<'a> for ApiKey {
    type Error = ();

    async fn from_request(req: &'a Request<'_>) -> request::Outcome<Self, Self::Error> {
        match req.headers().get_one(TYB_APIKEY_HTTP_HEADER) {
            Some(key) => {
                let gstate = get_global();
                let lock = gstate.read().await;
                let actual_key = lock.tyb_apikey.as_ref().unwrap();
                if key == actual_key {
                    return Outcome::Success(ApiKey(key.to_string()));
                }
                #[cfg(debug_assertions)] {
                    println!("\n\nAUTH ERROR: key `{}...` is invalid.\n", &key[..10]);
                }
                Outcome::Error((Status::Forbidden, ()))
            }
            _ => {
                #[cfg(debug_assertions)] {
                    println!("\n\nAUTH ERROR: No API key provided.\n\n")
                }
                Outcome::Error((Status::Forbidden, ()))
            },
        }
    }
}

#[rocket::get("/create-proj?<name>&<confirm>")]
async fn create_proj(name: &str, confirm: Option<bool>, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let confirm = confirm.unwrap_or(true);
    let res = proj_utils::create_proj(name);
    if let Err(e) = res {
        let e = e.to_string();
        if e.contains("already exists") {
            if !confirm {
                return Custom(Status::Ok, "success".to_string());
            }
            return Custom(Status::Ok, format!("Project Already Exists -> {e}"));
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
        return Custom(
            Status::InternalServerError,
            format!("Error adding files to project -> {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/delete-proj?<name>&<confirm>")]
async fn delete_proj(name: &str, confirm: Option<bool>, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let confirm = confirm.unwrap_or(true);
    let res = proj_utils::delete_proj(name);
    if let Err(e) = res {
        let e = e.to_string();
        if e.contains("does not exist") {
            if !confirm {
                return Custom(Status::Ok, "success".to_string());
            }
            return Custom(Status::Conflict, format!("Project does not exist -> {e}"));
        }
        return Custom(Status::InternalServerError, e);
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/list-projects")]
async fn list_projects(#[allow(unused)] apikey: ApiKey) -> Custom<Vec<u8>> {
    let res = proj_utils::get_proj_names();
    let res = match bincode::serialize(&res) {
        Ok(r) => r,
        Err(e) => {
            let e = e.to_string();
            let e: Vec<u8> = bincode::serialize(&e).unwrap_or(vec![]);
            return Custom(Status::InternalServerError, e);
        }
    };

    Custom(Status::Ok, res)
}

#[rocket::get("/purge-project?<name>&<retries>")]
async fn purge_projects(name: &str, retries:Option<u32>, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let retries = retries.unwrap_or(2);

    let container_name = format!("{}{CONTAINER_MOD}", name);
    let image_name = format!("{}{IMAGE_MOD}", name);

    let mut success: [anyhow::Result<()>; 2] = [
        Err(anyhow!("unknown error deleting container")),
        Err(anyhow!("unknown error deleting image")),
    ];

    for _ in 0..retries {
        let mut handles = vec![];
        if success[0].is_err() {
            let f = tokio::spawn(docker_utils::delete_container(container_name.clone()));
            handles.push((f, 0));
        }

        if success[1].is_err() {
            let f = tokio::spawn(docker_utils::delete_image(image_name.clone()));
            handles.push((f, 1));
        }

        for (h, i) in handles {
            let res = h.await;
            if let Ok(Ok(_)) = res {
                success[i] = Ok(());
            }
            else if let Err(e) = res {
                let e = e.to_string();
                if e.contains("No such image") || e.contains("No such container") {
                    success[i] = Ok(());
                }
                else {
                    success[i] = Err(anyhow!("{:?}", e));
                }
            }
        }
        if success.iter().all(|b| b.is_ok()) {
            break;
        }
    }

    if !success.iter().all(|b| b.is_ok()) {
        let err_msg = success
            .iter()
            .filter(|e| e.is_err())
            .map(|e| format!("Error -> {:?}", e))
            .collect::<Vec<String>>()
            .join("\n");
        return Custom(
            Status::InternalServerError, 
            format!("Failed to delete images and/or containers -> {}", err_msg)
        );
    }

    if let Err(e) = proj_utils::delete_proj(name) {
        if !e.to_string().contains("does not exist") {
            return Custom(
                Status::InternalServerError, 
                format!("Failed to delete project files -> {}", e)
            );
        }
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/pull-files?<name>")]
fn pull_proj_files(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<Vec<u8>> {
    let fc = match proj_utils::load_proj_files(name, None) {
        Ok(fc) => fc,
        Err(e) => {
            return Custom(
                Status::InternalServerError,
                format!("Error starting docker daemon -> {e}")
                    .as_bytes()
                    .to_vec(),
            );
        }
    };

    let mut out_packet = BinaryPacket::from(&fc).unwrap();
    if out_packet.data.len() > 5_000_000 {
        compression_utils::compress_brotli(&mut out_packet).unwrap();
    }

    let payload = bincode::serialize(&out_packet).unwrap();

    Custom(Status::Ok, payload)
}

#[rocket::post("/start-docker-daemon")]
async fn start_docker_daemon(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    if let Err(e) = docker_utils::start_daemon().await {
        return Custom(
            Status::InternalServerError,
            format!("Error starting docker daemon -> {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/end-docker-daemon")]
async fn end_docker_daemon(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    if let Err(e) = docker_utils::end_daemon().await {
        return Custom(
            Status::InternalServerError,
            format!("Failed to end daemon: {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/get-daemon-status")]
async fn get_daemon_status(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let status = match docker_utils::get_engine_status().await {
        Ok(b) => b,
        Err(e) => return Custom(Status::Ok, format!("Error getting daemon status: {}", e)),
    };

    Custom(Status::Ok, status.to_string())
}

#[rocket::get("/build-img?<name>")]
async fn build_image(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let mut path = PathBuf::from(LINUX_TYNKERBASE_PATH);
    path.push(name);

    let img_name = format!("{}{IMAGE_MOD}", name);
    let path_str = match path.to_str() {
        Some(p) => p,
        None => return Custom(Status::InternalServerError, "Failed to parse path".to_string()),
    };
    let res = docker_utils::build_image(path_str, &img_name);
    if let Err(e) = res.await {
        return Custom(Status::InternalServerError, format!("Failed to delete image -> {}", e));
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/delete-img?<name>")]
async fn delete_image(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let mut path = PathBuf::from(LINUX_TYNKERBASE_PATH);
    path.push(name);

    let img_name = format!("{}{IMAGE_MOD}", name);
    let res = docker_utils::delete_image(&img_name);
    if let Err(e) = res.await {
        return Custom(Status::InternalServerError, format!("Failed to delete image -> {}", e));
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/list-imgs")]
async fn list_images(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let lst = docker_utils::list_images().await;
    match lst {
        Ok(l) => Custom(Status::Ok, l),
        Err(e) => Custom(
            Status::InternalServerError, 
            format!("Error getting images -> {}", e)
        ),
    }
}

#[rocket::post("/spawn-container", data="<data>")]
async fn spawn_container(data: Vec<u8>, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let data: ProjConfig = bincode::deserialize(&data).unwrap();

    let img_name = format!("{}{IMAGE_MOD}", &data.proj_name);
    let container_name = format!("{}{CONTAINER_MOD}", &data.proj_name);

    let f = docker_utils::start_container(
        &img_name, 
        &container_name, 
        &data.port_mapping, 
        &data.volume_mapping
    );

    if let Err(e) = f.await {
        return Custom(
            Status::InternalServerError,
            format!("Failed to start container -> {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/pause-container?<name>")]
async fn pause_container(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let container_name = format!("{}{CONTAINER_MOD}", name);
    if let Err(e) = docker_utils::pause_container(&container_name).await {
        return Custom(
            Status::InternalServerError,
            format!("Failed to pause container -> {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/get-diags")]
async fn get_diags(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let gstate = get_global();
    let lock = gstate.read().await;

    let diags = diagnostics::measure(lock.node_id.as_ref().unwrap(), lock.name.as_ref().unwrap());

    match serde_json::to_string(&diags.await) {
        Ok(json) => {
            Custom(Status::Ok, json)
        }
        Err(e) => {
            Custom(Status::InternalServerError, format!("Error serializing diagnostics: {:?}", e))
        }
    }
}

#[rocket::get("/delete-container?<name>")]
async fn delete_container(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let container_name = format!("{}{CONTAINER_MOD}", name);
    if let Err(e) = docker_utils::delete_container(&container_name).await {
        return Custom(
            Status::InternalServerError,
            format!("Failed to delete container -> {e}"),
        );
    }

    Custom(Status::Ok, "success".to_string())
}

#[rocket::get("/list-containers")]
async fn list_containers(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let lst = docker_utils::list_containers().await;
    match lst {
        Ok(l) => Custom(Status::Ok, l),
        Err(e) => Custom(
            Status::InternalServerError, 
            format!("Error getting containers -> {}", e)
        ),
    }
}

#[rocket::get("/list-container-stats")]
async fn list_container_stats(#[allow(unused)] apikey: ApiKey) -> Custom<String> {
    let lst = docker_utils::list_container_stats().await;
    match lst {
        Ok(l) => Custom(Status::Ok, l),
        Err(e) => Custom(
            Status::InternalServerError, 
            format!("Error getting containers -> {}", e)
        ),
    }
}

#[rocket::get("/get-id")]
async fn identify(#[allow(unused)] apikey: ApiKey) -> String {
    let gstate = get_global();
    gstate.read().await.node_id.clone().unwrap()
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}

#[rocket::catch(404)]
fn handle_404(req: &Request) -> Custom<String> {
    let body = format!("404: `{}` is not a valid path.", req.uri());
    Custom(Status::NotFound, body)
}

fn get_global() -> &'static TsGlobalState {
    GSTATE.get_or_init(|| GlobalState::new())
}

static GSTATE: OnceLock<TsGlobalState> = OnceLock::new();

#[launch]
async fn rocket() -> _ {
    // Ensure we're running on linux
    #[cfg(not(debug_assertions))]
    {
        if OS != "linux" {
            println!("Unfortunately, we only support linux at the current time.");
            process::exit(0);
        }
    }

    // Create TynkerBase Directory
    #[cfg(not(debug_assertions))] {
        let path = Path::new(LINUX_TYNKERBASE_PATH);
        if !path.exists() {
            if let Err(e) = fs::create_dir_all(path) {
                if e.to_string().contains("Permission denied") {
                    println!("TynkerBase Agent needs root privileges. Please re-run with `sudo`");
                    std::process::exit(0);
                }
            }
        }
    }

    // load global state
    let gstate = get_global();

    // Get login info
    let email = crypt_utils::prompt("Enter your email: ");
    let password = crypt_utils::prompt_secret("Enter your password: ");
    let pass_sha256 = hash_utils::sha256(&password);
    let pass_sha384 = hash_utils::sha384(&password);

    // Authorize login info
    let endpoint = format!(
        "{}/auth/login?email={}&pass_sha256={}",
        SERVER_ENDPOINT, email, pass_sha256
    );
    let res = reqwest::get(&endpoint).await.unwrap();
    if res.status().as_u16() == 403 {
        println!("Incorrect authorization.");
        process::exit(0);
    }

    let mut lock = gstate.write().await;
    lock.tyb_apikey = Some(load_apikey(res, &pass_sha384).await);
    let (node_id, name) = load_node_info(&email, &pass_sha256).await;
    lock.node_id = Some(node_id);
    lock.name = Some(name);

    lock.email = Some(email);
    lock.pass_sha256 = Some(pass_sha256);
    lock.pass_sha384 = Some(pass_sha384);

    drop(lock);
    drop(password);

    // Ensure TLS keys and certificates are ready
    let root_dir = consts::AGENT_ROOTDIR_PATH;
    if !tls_utils::check_tls_cert(root_dir) {
        if !dep_utils::openssl::check_openssl() {
            println!("In order to enable TLS encryption, you need to install open ssl. (If it's already installed, try restarting the terminal)");
            let res = crypt_utils::prompt("Would you like to do that now? (y/n): ");
            if res.to_ascii_lowercase() == "y" {
                if let Err(e) = dep_utils::openssl::install_openssl() {
                    println!(
                        "Failed to install OpenSSL, install manually.\nError -> {}",
                        e
                    );
                    process::exit(1);
                } else {
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
        println!(
            "Docker is not installed. (If it's already installed, try restarting the terminal)"
        );
        let res = crypt_utils::prompt("Install docker now? (y/n): ");
        if res.to_ascii_lowercase() == "y" {
            if let Err(e) = dep_utils::docker::install_docker() {
                println!(
                    "Failed to install Docker, install manually.\nError -> {}",
                    e
                );
                process::exit(1);
            } else {
                println!("Successfully installed Docker!");
            }
        }
        process::exit(0);
    }

    // Ensure ngrok auth token is ready
    let envs = env::args().collect::<Vec<String>>();
    if !(envs.len() >= 2 && envs[1] == "--priv") {
        let lock = gstate.read().await;

        let email = lock.email.clone().unwrap();
        let pass_sha256 = lock.pass_sha256.clone().unwrap();
        let tyb_apikey = lock.tyb_apikey.clone().unwrap();
        let node_id = lock.node_id.clone().unwrap();
        let name = lock.name.clone().unwrap();

        drop(lock);

        // check if authtoken exists in mongo
        let f_query =
            ngrok_utils::get_token(email.clone(), pass_sha256.clone(), tyb_apikey.clone());
        let f_query = tokio::spawn(f_query);

        // check if authtoken is already in ngrok config
        // let f_installed = ngrok_utils::token_is_installed();
        // let f_installed = tokio::spawn(f_installed);
        /*
        TODO:
            ngrok_utils::token_is_installed() fails in root mode due to
            oddities in the ngrok cli tool. Find a work around for this.
        */

        let query = f_query.await;
        // let installed = f_installed.await;

        let attach_tok = true;
        // if let Ok(Ok(b)) = installed {
        //     attach_tok = !b;
        // }

        if attach_tok {
            if let Ok(Some(tok)) = query {
                // if the token is not attached, but we got it from mongo, attach it
                #[cfg(debug_assertions)]
                println!("Token: {}", tok);
                let f = ngrok_utils::attach_token(&tok);
                f.await.unwrap();
            } else {
                // if it's not attached and not in mongo, prompt for it.
                let url = "https://dashboard.ngrok.com/get-started/your-authtoken";
                let mut prompt = format!(
                    "Please sign up for an ngrok account and get your api token at {}",
                    url
                );
                prompt.push_str("\nPlease enter that auth token here: ");

                let tok = crypt_utils::prompt_secret(&prompt);
                let f = tokio::spawn(ngrok_utils::attach_token(tok.clone()));
                let f_mong = tokio::spawn(ngrok_utils::store_token(
                    email.clone(),
                    pass_sha256.clone(),
                    tyb_apikey.clone(),
                    tok.clone(),
                ));

                f.await.unwrap().unwrap();
                let _ = f_mong.await;
            }
        }

        let public_addr = ngrok_utils::make_public(&email, &pass_sha256, &node_id, &name)
            .await
            .unwrap();
        println!("TynkerBase Agent running publicly on: {}", &public_addr);
    }

    // Specify configuration
    let tls_paths = tls_utils::get_cert_paths();
    let config = Config {
        address: "0.0.0.0".parse().expect("Invalid address"),
        port: 7462,
        tls: Some(TlsConfig::from_paths(&tls_paths[0], &tls_paths[1])),
        limits: Limits::default().limit("bytes", 20.megabytes()),
        ..Config::default()
    };
    let figment = Figment::from(config);

    // Ensure all fields are filled before hosting
    let lock = gstate.read().await;
    assert!(lock.check_status());
    drop(lock);

    rocket::custom(figment)
        .register("/", catchers![handle_404])
        .mount("/", routes![root, identify])
        .mount("/diags", routes![get_diags])
        .mount(
            "/files/proj",
            routes![
                create_proj,
                add_files_to_proj,
                delete_proj,
                pull_proj_files,
                list_projects,
                purge_projects,
            ],
        )
        .mount(
            "/docker/daemon",
            routes![start_docker_daemon, end_docker_daemon, get_daemon_status,],
        )
        .mount(
            "/docker/proj",
            routes![
                build_image, 
                delete_image, 
                list_images, 
                spawn_container, 
                pause_container, 
                delete_container, 
                list_containers,
                list_container_stats,
            ],
        )
}
