use std::process::Command;

pub mod dockwrap {
    use std::process::Command;

    struct DockerBuild {
        
    }

    pub fn start_engine() -> Result<(), String> {
        // runs `sudo systemctl start docker` to start the docker engine
        let res = Command::new("systemctl")
            .args(["start", "docker"])
            .status();
        if let Err(e) = res {
            return Err(e.to_string());
        }        
        Ok(())
    }

    pub fn get_engine_status() {
        // runs `sudo systemctl status docker` to get the status of the docker engine
    }
}