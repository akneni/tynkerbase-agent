use std::fs;
use std::io::Write;
use std::path::Path;
use std::env::consts::OS;

const PROJ_PATH_LINUX: &str = "/tynkerbase-projects";

pub fn create_proj(name: &str) -> String {
    if OS == "linux" {
        return format!("/{PROJ_PATH_LINUX}/{name}");
    }
    "".to_string()
}

pub fn get_proj_names() -> Vec<String> {
    // traverses the tynkerbase-projects directory to get all the names of all the folders
    // (which should each contain a project)
    let projects = fs::read_dir(format!("{PROJ_PATH_LINUX}"));
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
        _ => vec![]
    }
}

pub fn delete_proj(name: &str) -> Result<(), String> {
    if OS == "linux" {
        let path = format!("/{PROJ_PATH_LINUX}/{name}");
        if !Path::new(&path).exists() {
            return Err("Project does not exist".to_string());
        }
        if let Err(e) = fs::remove_dir_all(path) {
            return Err(e.to_string());
        }
    }
    Ok(())
}

pub fn clear_proj(name: &str) -> Result<(), String> {
    let res = delete_proj(name);
    if res.is_err() {
        return res;
    }
    create_proj(name);
    Ok(())
}

pub fn save_files_to_disk(files: &Vec<(String, Vec<u8>)>, output_dir: &str) -> Result<(), String> {
    // Given a dictionary of paths and their content in bytes as well as the path for the parent 
    // directory, this function will create all the files on the disk.
    // If an error occurs, the function will stop copying the files to the disk and return the error
    for (file_name, data) in files {
        let full_path = std::path::Path::new(output_dir).join(file_name);

        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                let res = std::fs::create_dir_all(&parent)
                    .map_err(|e| e.to_string());
                if res.is_err() {
                    return res;
                }
            }
        }

        let mut outfile = match fs::File::create(&full_path) {
            Ok(f) => f,
            Err(e) => return Err(e.to_string()),
        };
        let res = outfile.write_all(&data);
        if res.is_err() {
            return res.map_err(|e| e.to_string());
        }
    }
    Ok(())
}


// Loads all files in the specified parent directory to memory
pub fn load_files_from_disk(parent_dir: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut res = vec![];

    let parent_dir_path = Path::new(parent_dir);
    if parent_dir_path.is_file() {
        return Err("Argument must be a directory, not a file.".to_string());
    }
    let it = fs::read_dir(parent_dir_path)
        .map_err(|s| s.to_string())?;

    for new_file in it {
        if let Ok(new_file) = new_file {
            let new_file_str = new_file.file_name().into_string().unwrap();
            let full_path = parent_dir_path.join(&new_file_str)
                .to_str()
                .unwrap()
                .to_string();

            if new_file.path().is_dir() {
                let mut rec_res = match load_files_from_disk(&full_path) {
                    Ok(r) => r,
                    Err(e) => return Err(format!("recursive call failed: {}", e)),
                };
                res.append(&mut rec_res);
                
            }
            else {
                let bytes = fs::read(&full_path).map_err(|e| e.to_string())?;
                res.push((full_path, bytes));
            }
        }
    }

    Ok(res)
}