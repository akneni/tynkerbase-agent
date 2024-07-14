use anyhow::{anyhow, Result};
use std::env::consts::OS;
use std::process::Command;

pub fn find_package_manager() -> Result<&'static str> {
    if OS != "linux" {
        return Err(anyhow!("OS not supported"));
    }
    let package_managers = ["apt-get", "yum", "dnf", "pacman"];

    for &pm in &package_managers {
        if Command::new("which")
            .arg(pm)
            .output()
            .ok()
            .map_or(false, |output| output.status.success())
        {
            return Ok(pm);
        }
    }

    Err(anyhow!("No package manager found"))
}

pub mod docker {
    use super::find_package_manager;
    use anyhow::{anyhow, Result};
    use std::process::Command;

    pub fn check_docker() -> bool {
        let output = Command::new("docker").arg("--version").output();

        match output {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    pub fn install_docker() -> Result<()> {
        let package_manager = find_package_manager()?;
        match package_manager {
            "apt-get" => {
                Command::new("sudo")
                    .args(["apt-get", "update", "-y"])
                    .status()?;
                Command::new("sudo")
                    .args(["apt-get", "install", "-y", "docker.io"])
                    .status()?;
            }
            "yum" => {
                Command::new("sudo")
                    .args(["yum", "install", "-y", "docker"])
                    .status()?;
            }
            "dnf" => {
                Command::new("sudo")
                    .args(["dnf", "install", "-y", "docker"])
                    .status()?;
            }
            "pacman" => {
                Command::new("sudo")
                    .args(["pacman", "-Syu", "--noconfirm", "docker"])
                    .status()?;
            }
            _ => return Err(anyhow!("Unsupported package manager")),
        }

        Ok(())
    }

    pub fn get_install_command() -> Result<Vec<&'static str>> {
        let package_manager = find_package_manager()?;
        match package_manager {
            "apt-get" => Ok(vec!["apt-get", "install", "-y", "docker.io"]),
            "yum" => Ok(vec!["yum", "install", "-y", "docker"]),
            "dnf" => Ok(vec!["dnf", "install", "-y", "docker"]),
            "pacman" => Ok(vec!["pacman", "-Syu", "--noconfirm", "docker"]),
            _ => unreachable!(),
        }
    }
}

pub mod openssl {
    use super::find_package_manager;
    use anyhow::{anyhow, Result};
    use std::env::consts::OS;
    use std::process::Command;

    pub fn check_openssl() -> bool {
        // Checks if openssl is installed by running `openssl version`
        match Command::new("openssl").arg("version").output() {
            Ok(output) => {
                if output.status.success() {
                    true
                } else {
                    false
                }
            }
            Err(_e) => false,
        }
    }

    pub fn install_openssl() -> Result<()> {
        if OS != "linux" {
            return Err(anyhow!("OS not supported"));
        }

        let pm = find_package_manager()?;
        let install_command = match pm {
            "apt-get" => ("sudo", "apt-get install -y openssl"),
            "yum" => ("sudo", "yum install -y openssl"),
            "dnf" => ("sudo", "dnf install -y openssl"),
            "pacman" => ("sudo", "pacman -S --noconfirm openssl"),
            _ => unreachable!(),
        };

        println!("Installing OpenSSL...");

        let status = Command::new(install_command.0)
            .args(install_command.1.split(' '))
            .status()
            .map_err(|e| anyhow!("Failed to execute install command -> {e}"))?;

        if !status.success() {
            return Err(anyhow!("Failed to install OpenSSL"));
        }

        Ok(())
    }

    pub fn get_install_command() -> Result<Vec<&'static str>> {
        let pm = find_package_manager()?;
        match pm {
            "apt-get" => Ok("apt-get install -y openssl"
                .split_ascii_whitespace()
                .collect()),
            "yum" => Ok("yum install -y openssl".split_ascii_whitespace().collect()),
            "dnf" => Ok("dnf install -y openssl".split_ascii_whitespace().collect()),
            "pacman" => Ok("pacman -S --noconfirm openssl"
                .split_ascii_whitespace()
                .collect()),
            _ => unreachable!(),
        }
    }
}
