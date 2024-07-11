mod consts;
mod dep_utils;
mod tls_utils;
mod ngrok_utils;
mod global_state;

use consts::{AGENT_ROOTDIR_PATH, SERVER_ENDPOINT};
use global_state::{GlobalState, TsGlobalState};
use tynkerbase_universal::{
    crypt_utils::{
        self, compression_utils, hash_utils, BinaryPacket 
    }, 
    docker_utils, 
    proj_utils::{self, FileCollection},
    constants as univ_consts,
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
    config::{Config, TlsConfig},
    figment::Figment,
};
use rand::{thread_rng, Rng};

use std::{
    io::{BufReader, BufRead}, 
    process::{self, Command, Stdio},
    env,
    sync::OnceLock,
};

// Suppress warning being thrown since OS is only used in release mode
#[allow(unused_imports)]
use std::{
    fs,
    path::Path,
    env::consts::OS,
};

async fn load_apikey(email: &str, pass_sha256: &str, pass_sha384: &str) -> String {
    const ENDPOINT: &str = "https://tynkerbase-server.shuttleapp.rs";
    let endpoint = format!("{}/auth/login?email={}&pass_sha256={}", ENDPOINT, &email, &pass_sha256);

    let res = reqwest::get(&endpoint)
        .await
        .unwrap();

    let salt = res.text().await
        .expect("unable to extract text from response");

    crypt_utils::gen_apikey(pass_sha384, &salt)
}

async fn prompt_node_name(email: &str, pass_sha256: &str) -> String {
    loop {
        let name = crypt_utils::prompt("Enter a name for this node: ");

        let endpoint = format!("{SERVER_ENDPOINT}/ngrok/check-node-exists/name?\
            email={email}&\
            pass_sha256={pass_sha256}&\
            name={}", &name);

        let res = reqwest::get(endpoint)
            .await
            .unwrap();

        if !res.status().is_success() {
            println!("Error communicating with database. This is an error with tynkerbase.");
            #[cfg(debug_assertions)] {
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
        fs::create_dir(&path)
            .unwrap();
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
        .map(|_| thread_rng().gen_range(97..97+26) as u8)
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


struct ApiKey(
    #[allow(unused)]
    String
);

#[rocket::async_trait]
impl<'a> FromRequest<'a> for ApiKey {
    type Error = ();

    async fn from_request(req: &'a Request<'_>) -> request::Outcome<Self, Self::Error> {
        match req.headers().get_one("tyb-api-key") {
            Some(key) => {
                let gstate = get_global();
                let lock = gstate.read().await;
                if key == (lock.tyb_apikey.as_ref().unwrap()) {
                    return Outcome::Success(ApiKey(key.to_string()));
                }
                Outcome::Error((Status::Forbidden, ()))
            },
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

#[rocket::get("/list_projects")]
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

#[rocket::get("/pull-files?<name>")]
fn pull_proj_files(name: &str, #[allow(unused)] apikey: ApiKey) -> Custom<Vec<u8>> {
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
    let path = format!("{}/{}", univ_consts::LINUX_TYNKERBASE_PATH, name);
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
async fn install_docker(#[allow(unused)] apikey: ApiKey) -> TextStream![String] {
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
async fn install_openssl(#[allow(unused)] apikey: ApiKey) -> TextStream![String] {
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

#[rocket::get("/get-id")]
async fn identify(#[allow(unused)] apikey: ApiKey) -> String {
    let gstate = get_global();
    gstate.read().await.node_id.clone().unwrap()
}

#[rocket::get("/")]
async fn root() -> &'static str {
    "alive"
}


fn get_global() -> &'static TsGlobalState {
    GSTATE.get_or_init(|| {
        GlobalState::new()
    })
}

static GSTATE: OnceLock<TsGlobalState> = OnceLock::new();


#[launch]
async fn rocket() -> _ {
    // Ensure we're running on linux
    #[cfg(not(debug_assertions))] {
        if OS != "linux" {
            println!("Unfortunately, we only support linux at the current time.");
            process::exit(0);
        }
    }
    // Create TynkerBase Directory
    #[cfg(not(debug_assertions))] {
        let path_str = format!("/{}", univ_consts::LINUX_TYNKERBASE_PATH);
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

    // load global state
    let gstate = get_global();

    // Get login info
    let email = crypt_utils::prompt("Enter your email: ");
    let password = crypt_utils::prompt_secret("Enter your password: ");
    let pass_sha256 = hash_utils::sha256(&password);
    let pass_sha384 = hash_utils::sha384(&password);

    // TODO: Authorize login info here

    let mut lock = gstate.write().await;
    lock.tyb_apikey = Some(load_apikey(&email, &pass_sha256, &pass_sha384).await);
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
        let f_query = ngrok_utils::get_token(email.clone(), pass_sha256.clone(), tyb_apikey.clone());
        let f_query = tokio::spawn(f_query);

        // check if authtoken is already in ngrok config
        let f_installed = ngrok_utils::token_is_installed();
        let f_installed = tokio::spawn(f_installed);

        let query = f_query.await;
        let installed = f_installed.await;

        let mut attach_tok = true;
        if let Ok(Ok(b)) = installed {
            attach_tok = !b;
        }


        if attach_tok {
            if let Ok(Some(tok)) = query {
                // if the token is not attached, but we got it from mongo, attach it
                #[cfg(debug_assertions)] println!("Token: {}", tok);
                let f = ngrok_utils::attach_token(&tok);
                f.await.unwrap();
            }
            else {
                // if it's not attached and not in mongo, prompt for it. 
                let url = "https://dashboard.ngrok.com/get-started/your-authtoken";
                let mut prompt = format!("Please sign up for an ngrok account and get your api token at {}", url);
                prompt.push_str("\nPlease enter that auth token here: ");

                let tok = crypt_utils::prompt_secret(&prompt);
                let f = tokio::spawn(ngrok_utils::attach_token(tok.clone()));
                let f_mong = tokio::spawn(
                    ngrok_utils::store_token(email.clone(), pass_sha256.clone(), tyb_apikey.clone(), tok.clone())
                );

                f.await.unwrap().unwrap();
                let _ = f_mong.await;
            }
        }

        let _public_addr = ngrok_utils::make_public(&email, &pass_sha256, &node_id, &name)
            .await
            .unwrap();
        println!("TynkerBase Agent running publicly!");

    }

    // Specify configuration
    let tls_paths = tls_utils::get_cert_paths();
    let config = Config {
        address: "0.0.0.0".parse().expect("Invalid address"),
        port: 7462,
        tls: Some(TlsConfig::from_paths(&tls_paths[0], &tls_paths[1])),
        ..Config::default()
    };
    let figment = Figment::from(config);

    // Ensure all fields are filled before hosting
    let lock = gstate.read().await;
    assert!(lock.check_status());
    drop(lock);

    rocket::custom(figment)
        .mount("/", routes![root, identify])
        .mount("/files/proj", routes![
                create_proj, 
                add_files_to_proj, 
                delete_proj, 
                pull_proj_files, 
                list_projects,
        ])
        .mount("/docker/daemon", routes![
            start_docker_daemon, 
            end_docker_daemon, 
            get_daemon_status,
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

