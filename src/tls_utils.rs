use crate::consts::AGENT_ROOTDIR_PATH;
use anyhow::{anyhow, Result};
use std::{fs, path::Path, process::Command};

pub fn check_tls_cert(proj_root_path: &str) -> bool {
    if !Path::new(&format!("{proj_root_path}/keys")).exists() {
        return false;
    }
    Path::new(&format!("{proj_root_path}/keys/tls-cert.pem")).exists()
        && Path::new(&format!("{proj_root_path}/keys/tls-key.pem")).exists()
}

pub fn gen_tls_cert(proj_root_path: &str) -> Result<()> {
    clear_tls_cert(proj_root_path)?;

    let cmd = vec![
        "openssl".to_string(),
        "ecparam".to_string(),
        "-name".to_string(),
        "secp256r1".to_string(),
        "-genkey".to_string(),
        "-noout".to_string(),
        "-out".to_string(),
        format!("{}/keys/tls-key.pem", proj_root_path),
    ];

    println!("Generating TLS private key...\nPlease answer the following questions to generate the certificate\n");
    let mut child = Command::new(&cmd[0]).args(&cmd[1..]).spawn().map_err(|e| {
        anyhow!("Failed to generate private key -> {e}\nMake sure you have openssl installed!")
    })?;
    child
        .wait()
        .map_err(|e| anyhow!("Failed to wait on process -> {e}"))?;

    println!("Finished generating private key.\n\nGenerating TLS certificate... ");

    // Command to generate the self-signed certificate
    let cmd = vec![
        "openssl".to_string(),
        "req".to_string(),
        "-x509".to_string(),
        "-new".to_string(),
        "-key".to_string(),
        format!("{}/keys/tls-key.pem", proj_root_path),
        "-out".to_string(),
        format!("{}/keys/tls-cert.pem", proj_root_path),
        "-days".to_string(),
        "36500".to_string(),
    ];

    let mut child = Command::new(&cmd[0]).args(&cmd[1..]).spawn().map_err(|e| {
        anyhow!("Failed to generate private key -> {e}\nMake sure you have openssl installed!")
    })?;
    child
        .wait()
        .map_err(|e| anyhow!("Failed to wait on process -> {e}"))?;

    println!("Finished generating all TLS dependencies.");

    Ok(())
}

pub fn clear_tls_cert(proj_root_path: &str) -> Result<()> {
    if !Path::new(&format!("{proj_root_path}/keys")).exists() {
        fs::create_dir(&format!("{proj_root_path}/keys")).map_err(|e| {
            anyhow!("Failed to create `keys` directory to hold the certificates -> {e}")
        })?;
    } else {
        let files = ["keys/tls-key.pem", "keys/tls-cert.pem", "keys/tls-csr.csr"];
        for file in files {
            let path = format!("{proj_root_path}/{file}");
            if Path::new(&path).exists() {
                fs::remove_file(&path)
                    .map_err(|e| anyhow!("Failed to remove existing` {path} -> {e}"))?;
            }
        }
    }
    Ok(())
}

pub fn get_cert_paths() -> [String; 2] {
    [
        format!("{}/keys/tls-cert.pem", AGENT_ROOTDIR_PATH),
        format!("{}/keys/tls-key.pem", AGENT_ROOTDIR_PATH),
    ]
}
