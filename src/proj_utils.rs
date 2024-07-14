use tynkerbase_universal::{constants::LINUX_TYNKERBASE_PATH, file_utils::FileCollection};

use anyhow::{anyhow, Result};
use std::{
    env::consts::OS,
    fs,
    path::{Path, PathBuf},
};

pub fn create_proj(name: &str) -> Result<String> {
    if OS == "linux" {
        // Ensure project directory exists first
        let mut root_path = PathBuf::from(LINUX_TYNKERBASE_PATH);
        if !root_path.exists() {
            fs::create_dir_all(&root_path).map_err(|e| {
                anyhow!(
                    "Root project directory missing.\
                Encountered another error when creating them -> {}",
                    e
                )
            })?;
        }

        root_path.push(name);
        if root_path.exists() {
            return Err(anyhow!("Project `{}` already exists", name));
        }
        if let Err(e) = fs::create_dir_all(root_path) {
            return Err(anyhow!("Error creating dir: `{}`", e));
        }
        return Ok(format!("Created `{LINUX_TYNKERBASE_PATH}/{name}`"));
    }
    Err(anyhow!("OS `{}` is unsupported", OS))
}

pub fn add_files_to_proj(name: &str, files: FileCollection) -> Result<()> {
    if OS == "linux" {
        let proj_path = format!("{LINUX_TYNKERBASE_PATH}/{name}");
        if !Path::new(&proj_path).exists() {
            return Err(anyhow!("Project `{}` does not exist.", { name }));
        }

        if let Err(e) = files.save(&proj_path) {
            return Err(anyhow!("{}", e));
        }
        return Ok(());
    }
    Err(anyhow!("OS `{}` is unsupported", OS))
}

pub fn get_proj_names() -> Vec<String> {
    // traverses the tynkerbase-projects directory to get all the names of all the folders
    // (which should each contain a project)
    let projects = fs::read_dir(format!("{LINUX_TYNKERBASE_PATH}"));
    match projects {
        Ok(projects) => {
            let mut res = vec![];
            for path in projects {
                if let Ok(path) = path {
                    if let Ok(path) = path.file_name().into_string() {
                        res.push(path);
                    }
                }
            }
            res
        }
        _ => vec![],
    }
}

pub fn delete_proj(name: &str) -> Result<()> {
    if OS == "linux" {
        let path = format!("{LINUX_TYNKERBASE_PATH}/{name}");
        if !Path::new(&path).exists() {
            return Err(anyhow!("Project does not exist"));
        }
        if let Err(e) = fs::remove_dir_all(path) {
            return Err(anyhow!("{}", e));
        }
    }
    Ok(())
}

pub fn clear_proj(name: &str) -> Result<()> {
    let res = delete_proj(name);
    if res.is_err() {
        return res;
    }
    let res = create_proj(name);
    if let Err(e) = res {
        return Err(anyhow!("{}", e));
    }
    Ok(())
}

pub fn load_proj_files(name: &str, ignore: Option<&Vec<String>>) -> Result<FileCollection> {
    let path_str = format!("{}/{}", LINUX_TYNKERBASE_PATH, name);
    let empty_vec: Vec<String> = vec![];
    let ignore = ignore.unwrap_or(&empty_vec);

    match FileCollection::load(&path_str, &ignore) {
        Ok(fc) => Ok(fc),
        Err(e) => Err(e),
    }
}
