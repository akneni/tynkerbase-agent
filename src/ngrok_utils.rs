use crate::consts::SERVER_ENDPOINT;
use tynkerbase_universal::crypt_utils::aes_utils;
use anyhow::{anyhow, Result};
use bincode;
use tokio::process::Command as TkCommand;
use reqwest;
use std::{
    process::{Command, Stdio},
    time::Duration,
};

pub async fn store_token<T: AsRef<str>>(email: T , pass_sha256: T, tyb_apikey: T, ng_token: String) -> Result<()> {
    let email = email.as_ref();
    let pass_sha256 = pass_sha256.as_ref();
    let tyb_apikey = tyb_apikey.as_ref();
    let ng_token = ng_token.as_ref();

    // encrypt ng_token
    let aes = aes_utils::AesEncryption::from_tyb_apikey(tyb_apikey);
    let mut aes_ng = aes_utils::AesMsg::from_str(ng_token);
    aes.encrypt(&mut aes_ng)
        .map_err(|e| anyhow!("Failed to encrypt -> {}", e))?;
    assert!(aes_ng.is_encrypted);
    let aes_ng = bincode::serialize(&aes_ng)
        .map_err(|e| anyhow!("Failed to serialize token -> {}", e))?;


    // Send request
    let endpoint = format!("{SERVER_ENDPOINT}/ngrok/save-ng-auth?email={email}&pass_sha256={pass_sha256}");

    let res = reqwest::Client::new()
        .post(&endpoint)
        .body(aes_ng)
        .send()
        .await
        .map_err(|e| anyhow!("Error sending req -> {}", e))?;
    
    if !res.status().is_success() {
        return Err(anyhow!("{:?}", res.status()));
    }

    Ok(())
}

pub async fn get_token(email: impl AsRef<str>, pass_sha256: impl AsRef<str>, tyb_apikey: impl AsRef<str>) -> Option<String> {
    let email = email.as_ref();
    let pass_sha256 = pass_sha256.as_ref();
    let tyb_apikey = tyb_apikey.as_ref();

    // Send request
    let endpoint = format!("{SERVER_ENDPOINT}/ngrok/get-ng-auth?email={email}&pass_sha256={pass_sha256}");

    let res = reqwest::Client::new()
        .get(&endpoint)
        .send()
        .await;

    let res = match res {
        Ok(r) => r,
        _ => return None,
    };
    
    let body = match res.bytes().await {
        Ok(r) => r.to_vec(),
        _ => return None,
    };

    // Decrypt token
    let mut aes_ng: aes_utils::AesMsg = bincode::deserialize(&body).unwrap();
    let aes = aes_utils::AesEncryption::from_tyb_apikey(tyb_apikey);
    aes.decrypt(&mut aes_ng)
        .unwrap();

    Some(aes_ng.extract_str().unwrap())
}

/*
NOTE: The ngrok rust driver for rust seems to be buggy. I will use the CLI tool 
through std::process::Command and will switch to the ngrok crate once it becomes 
stable. 
*/

pub async fn attach_token(ng_token: impl AsRef<str>) -> Result<()> {
    let ng_token = ng_token.as_ref();
    let child = TkCommand::new("ngrok")
        .args(["config", "add-authtoken", ng_token])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to start process -> {}", e))?;
    
    if !child.status.success() {
        return Err(anyhow!("failed to attach token -> {:?}", child.status));
    }

    Ok(())
}

pub async fn token_is_installed() -> Result<bool> {
    let child = TkCommand::new("ngrok")
        .args(["http", "45824"])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to start process -> {}", e))?;

    if child.status.success() {
        return Ok(true);
    }

    let err = String::from_utf8(child.stderr).unwrap();
    if !err.contains("ERR_NGROK_4018") {
        return Err(anyhow!("unknown error -> {}", err));
    }
    Ok(false)
}

// Uses ngrok to make service public and inserts the public address into mongo
pub async fn make_public<T: AsRef<str>>(email: T , pass_sha256: T, node_id: T, name: T) -> Result<String>{
    let email = email.as_ref();
    let pass_sha256 = pass_sha256.as_ref();
    let node_id = node_id.as_ref();
    let name = name.as_ref();

    let public_addr = spawn_ngrok(10.).await?;

    let endpoint = format!("{SERVER_ENDPOINT}/ngrok/add-addr?\
        email={email}\
        &pass_sha256={pass_sha256}\
        &node_id={node_id}\
        &name={name}\
        &addr={public_addr}");

    let _ = reqwest::Client::new()
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| anyhow!("Error sending req -> {}", e))?;

    Ok(public_addr)
}

async fn spawn_ngrok (timeout: f64) -> Result<String> {
    /*
    Unfortunately, the ngrok rust driver doesn't seem to work. 
    Additionally, the ngrok CLI tool displays output in a terminal UI, not in stdio, so
    I can spawn ngrok CLI using std::process::Command, but I can't actually see the public url. 
    As as a shoddy workaround, this function uses Command to start ngrok using it's cli tool, and then makes
    an API request to a local endpoint where ngrok hosts it's data (including the new
    public url)
    */

    let _ = Command::new("ngrok")
        .args(["http", "7462"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs_f64(timeout / 10.)).await;
    
        let res = reqwest::get("http://localhost:4040/api/tunnels/")
            .await;
        let res = match res {
            Ok(r) => r,
            _ => continue,
        };

        let url = res.text().await;
        let url = match url {
            Ok(u) => u,
            _ => continue,
        };

        if !url.contains("\"public_url\":\"") {
            continue;
        }
    
        let url = url
            .split_once("\"public_url\":\"")
            .unwrap().1
            .to_string();
    
        let url = url.split_once("\",\"").unwrap().0.to_string();
        return Ok(url);
    }

    Err(anyhow!("timeout error, could not get public ip from ngrok"))
}