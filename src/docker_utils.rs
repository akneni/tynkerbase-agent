use anyhow::{anyhow, Result};
use std::env::consts::OS;
use tokio::process::Command;

pub async fn start_daemon() -> Result<()> {
    if OS == "linux" {
        // runs `sudo systemctl start docker` to start the docker engine
        let res = Command::new("systemctl").args(["start", "docker"]).status();
        if let Err(e) = res.await {
            return Err(anyhow!("{}", e));
        }
    } else {
        return Err(anyhow!("OS `{}` not supported.", OS));
    }
    Ok(())
}

pub async fn end_daemon() -> Result<()> {
    if OS == "linux" {
        // Runs `sudo service docker stop` to stop the docker daemon
        let cmd = Command::new("service").args(["docker", "stop"]).status();

        if let Err(e) = cmd.await {
            return Err(anyhow!("Error stopping docker daemon: {}", e));
        }
    } else {
        return Err(anyhow!("OS `{}` not supported.", OS));
    }

    Ok(())
}

pub async fn get_engine_status() -> Result<bool> {
    if OS == "linux" {
        // runs `sudo systemctl status docker` to get the status of the docker engine
        let cmd = Command::new("systemctl")
            .args(["status", "docker"])
            .output();

        let cmd = match cmd.await {
            Ok(c) => c,
            Err(e) => return Err(anyhow!("Error getting docker daemon status: {}", e)),
        };

        // Parse the output to check if it's active or not
        let out = match String::from_utf8(cmd.stdout) {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow!(
                    "Error extracting string from `systemctl status docker` output: {}",
                    e
                ))
            }
        };

        let out = match out.split_once("Active: ") {
            Some(r) => r,
            None => return Err(anyhow!("Error parsing `systemctl status docker` output")),
        }
        .1;

        let out = match out.split_once(" ") {
            Some(r) => r,
            None => return Err(anyhow!("Error parsing `systemctl status docker` output")),
        }
        .0;

        if out == "active" {
            return Ok(true);
        } else if out == "inactive" {
            return Ok(false);
        }

        return Err(anyhow!("Error parsing `systemctl status docker` output"));
    }
    return Err(anyhow!("OS `{}` not supported.", OS));
}

pub async fn build_image(path: &str, img_name: &str) -> Result<()> {
    let cmd = Command::new("docker")
        .args(["build", "-t", img_name, "."])
        .current_dir(path)
        .output()
        .await
        .map_err(|e| anyhow!("Error building docker image => {}", e))?;

    if !cmd.status.success() {
        let err = String::from_utf8(cmd.stderr).unwrap_or("Unable to extract stderr".to_string());
        return Err(anyhow!("docker build command failed: \n{}", err));
    }
    Ok(())
}

pub async fn delete_image(img_name: impl AsRef<str>) -> Result<()> {
    let img_name = img_name.as_ref();
    let output = Command::new("docker")
        .args(&["rmi", "-f", img_name])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to launch docker command -> {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8(output.stderr)
            .unwrap_or("Unable to extract stderr".to_string());
        return Err(anyhow!("Failed to delete image `{}`:\n{}", img_name, err));
    }

    Ok(())
}

pub async fn list_images() -> Result<String> {
    // TODO: Test this function
    let cmd = Command::new("docker")
        .arg("images")
        .output()
        .await
        .map_err(|e| anyhow!("{}", e))?;

    let s = String::from_utf8(cmd.stdout).map_err(|e| anyhow!("{e}"))?;

    Ok(s)
}

pub async fn list_containers() -> Result<String> {
    let output = Command::new("docker")
        .args(["ps", "-a", "--format", "table {{.ID}}|||{{.Image}}|||{{.Command}}|||{{.CreatedAt}}|||{{.Status}}|||{{.Ports}}|||{{.Names}}"])
        .output()
        .await
        .map_err(|e| anyhow!("Error executing list_containers docker command [fn list_containers] -> {}", e))?;


    String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("Error extracting stdout [fn list_containers] -> {}", e))
}

pub async fn list_container_stats() -> Result<String> {
    let output = Command::new("docker")
        .args(["stats", "--no-stream", "--format", "table {{.ID}}|||{{.Container}}|||{{.CPUPerc}}|||{{.MemUsage}}|||{{.MemPerc}}|||{{.NetIO}}|||{{.BlockIO}}|||{{.PIDs}}"])
        .output()
        .await
        .map_err(|e| anyhow!("Error executing `docker stats` command [fn list_container_stats] => {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8(output.stderr).unwrap_or("unable to extract stderr".to_string());
        return Err(anyhow!("`docker stats` command returned non 0 exit code [fn list_container_stats] -> {}", err));
    }

    match String::from_utf8(output.stdout) {
        Ok(output) => Ok(output),
        Err(e) => {
            Err(anyhow!("Failed to extract text from stdout [fn list_container_stats] -> {}", e))
        }
    }
}

pub async fn start_container(
    img_name: &str,
    container_name: &str,
    ports: &Vec<[u16; 2]>,
    volumes: &Vec<[String; 2]>,
) -> Result<()> {
    let mut args = vec![
        "run".to_string(),
        "-d".to_string(),
        "--name".to_string(),
        container_name.to_string(),
    ];

    for p in ports {
        args.push("-p".to_string());
        args.push(format!("{}:{}", p[0], p[1]));
    }

    for v in volumes {
        args.push("-v".to_string());
        args.push(format!("{}:{}", &v[0], &v[1]));
    }

    args.push(img_name.to_string());

    let cmd = Command::new("docker")
        .args(&args)
        .output()
        .await
        .map_err(|e| anyhow!("{e}"))?;

    if !cmd.status.success() {
        let err = String::from_utf8(cmd.stderr).unwrap_or("Unable to extract stderr".to_string());
        return Err(anyhow!(
            "Failed to spawn container `{}`: \n{}",
            container_name,
            err
        ));
    }

    Ok(())
}

pub async fn pause_container(container_name: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(&["stop", container_name])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to launch docker command -> {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8(output.stderr)
            .unwrap_or("Unable to extract stderr".to_string());
        return Err(anyhow!("Failed to stop container `{}`:\n{}", container_name, err));
    }

    Ok(())
}

pub async fn delete_container(container_name: impl AsRef<str>) -> Result<()>{
    let container_name = container_name.as_ref();
    let output = Command::new("docker")
        .args(&["rm", "-f", container_name])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to launch docker command -> {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8(output.stderr)
            .unwrap_or("Unable to extract stderr".to_string());
        let err = anyhow!("Failed to delete container `{}`:\n{}", container_name, err);
        #[cfg(debug_assertions)] println!("{:#?}", err);
        return Err(err);
    }

    Ok(())
}
