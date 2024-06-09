use std::process::Command;
use std::env::consts::OS;
use anyhow::{anyhow, Result};

pub fn start_daemon() -> Result<()> {
    if OS == "linux" {
        // runs `sudo systemctl start docker` to start the docker engine
        let res = Command::new("systemctl")
            .args(["start", "docker"])
            .status();
        if let Err(e) = res {
            return Err(anyhow!("{}", e));
        }        
    }
    else {
        return Err(anyhow!("OS `{}` not supported.", OS));
    }
    Ok(())
}

pub fn end_daemon() -> Result<()> {
    if OS == "linux" {
        // Runs `sudo service docker stop` to stop the docker daemon
        let cmd = Command::new("service")
            .args(["docker", "stop"])
            .status();
    
        if let Err(e) = cmd {
            return Err(anyhow!("Error stopping docker daemon: {}", e));
        }
    }
    else {
        return Err(anyhow!("OS `{}` not supported.", OS));
    }

    Ok(())
}

pub fn get_engine_status() -> Result<bool> {
    if OS == "linux" {
        // runs `sudo systemctl status docker` to get the status of the docker engine
        let cmd = Command::new("systemctl")
            .args(["status", "docker"])
            .output();
    
        let cmd = match cmd {
            Ok(c) => c,
            Err(e) => return Err(anyhow!("Error getting docker daemon status: {}", e)),
        };
    
        // Parse the output to check if it's active or not
        let out = match String::from_utf8(cmd.stdout) {
            Ok(s) => s,
            Err(e) => return Err(anyhow!("Error extracting string from `systemctl status docker` output: {}", e)),
        };
    
        let out = match out.split_once("Active: ") {
            Some(r) => r,
            None => return Err(anyhow!("Error parsing `systemctl status docker` output")),
        }.1;
    
        let out = match out.split_once(" ") {
            Some(r) => r,
            None => return Err(anyhow!("Error parsing `systemctl status docker` output")),
        }.0;
    
        if out == "active" {
            return Ok(true);
        }
        else if out == "inactive" {
            return Ok(false);
        }
        
        return Err(anyhow!("Error parsing `systemctl status docker` output"));
    }
    return Err(anyhow!("OS `{}` not supported.", OS));
}

fn install_docker() -> Result<()> {
    // TODO: implement this function for the apt and dnf package managers (or in an agnostic manner).
    Ok(())
}