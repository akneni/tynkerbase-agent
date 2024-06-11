use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};
use std::env::consts::OS;

const LINUX_TYNKERBASE_PATH: &str = "/tynkerbase-projects";

#[derive(Debug)]
pub struct FileData {
    filename: String,
    contents: Vec<u8>,
}

impl FileData {
    pub fn from(filename: String, contents: Vec<u8>) -> Self {
        FileData {
            filename: filename,
            contents: contents
        }
    }

    pub fn mem_size(&self) -> usize {
        let res = 4*64 + self.filename.len() + self.contents.len();
        return res;
    } 

    pub fn extract(self) -> (String, Vec<u8>) {
        let s = self.filename;
        let d = self.contents;
        (s, d)    
    }
}

#[derive(Debug)]
pub struct FileCollection {
    files: Vec<FileData>
}

impl FileCollection {
    pub fn new() -> Self {
        FileCollection{files: vec![]}
    }

    pub fn from_bytes(mut vec: Vec<u8>) -> Result<Self> {
        let mut res = FileCollection::new();

        while vec.len() > 0 {
            if vec.len() < 8 {
                return Err(anyhow!("Invalid bytes"));
            }

            // get length of file binary contents
            let mut d_len = [0_u8; 8];
            d_len.copy_from_slice(&vec[vec.len()-8..vec.len()]);
            let d_len = usize::from_be_bytes(d_len);
            vec.truncate(vec.len()-8);
            
            // get file binary contents
            let mut data = vec![];
            data.copy_from_slice(&vec[vec.len()-d_len..vec.len()]);
            vec.truncate(vec.len()-d_len);     

            // get length of string
            let mut s_len = [0_u8; 8];
            s_len.copy_from_slice(&vec[vec.len()-8..vec.len()]);
            let s_len = usize::from_be_bytes(s_len);
            vec.truncate(vec.len()-8);
    
            // get string (check for decoding error)
            let mut string = vec![];
            string.copy_from_slice(&vec[vec.len()-d_len..vec.len()]);
            vec.truncate(vec.len()-s_len);
            let string = match String::from_utf8(string){
                Ok(s) => s,
                Err(e) => return Err(anyhow!("Error decoding string: {}", e)),
            };

            res.push(
                FileData::from(string, data)
            );

        }

        Ok(res)
    }
    
    pub fn save(self, output_dir: &str) -> Result<()> {
        // Given a dictionary of paths and their content in bytes as well as the path for the parent 
        // directory, this function will create all the files on the disk.
        // If an error occurs, the function will stop copying the files to the disk and return the error
        for file_obj in self.files {
            let file_name= &file_obj.filename;
            let data = &file_obj.contents;
            // (file_name, data) 
            let full_path = std::path::Path::new(output_dir).join(file_name);
    
            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    let res = std::fs::create_dir_all(&parent)
                        .map_err(|e| e.to_string());
                    if res.is_err() {
                        return Err(anyhow!("{:?}", res));
                    }
                }
            }
    
            let mut outfile = match fs::File::create(&full_path) {
                Ok(f) => f,
                Err(e) => return Err(anyhow!("{}", e)),
            };
            let res = outfile.write_all(&data);
            if res.is_err() {
                return res.map_err(|e| anyhow!("{}", e));
            }
        }

        Ok(())
    }    

    pub fn load(parent_dir: &str, ignore: &Vec<String>) -> Result<Self> {
        let mut res = FileCollection{files:vec![]};
    
        let parent_dir_path = Path::new(parent_dir);
        if parent_dir_path.is_file() {
            return Err(anyhow!("Argument must be a directory, not a file."));
        }
        let it = fs::read_dir(parent_dir_path)
            .map_err(|s| anyhow!("{}", s))?;
    
        for new_file in it {
            if let Ok(new_file) = new_file {
                let new_file_str = new_file.file_name().into_string().unwrap();
                let full_path = parent_dir_path.join(&new_file_str);
                let full_path_str = full_path
                    .to_str()
                    .unwrap()
                    .to_string();
    
                if !Self::filter_ignored_files(ignore, &full_path) {
                    continue;
                }
    
                if new_file.path().is_dir() {
                    let mut rec_res = match Self::load(&full_path_str, ignore) {
                        Ok(r) => r,
                        Err(e) => return Err(anyhow!("recursive call failed: {}", e)),
                    };
                    res.append(&mut rec_res);
                    
                }
                else {
                    let bytes = fs::read(&full_path_str).map_err(|e| anyhow!("{}", e))?;
                    res.push(FileData::from(full_path_str, bytes));
                }
            }
        }
    
        Ok(res)
    }

    fn filter_ignored_files(ignore: &Vec<String>, path: &PathBuf) -> bool {
        let is_dir = path.is_dir();
        let full_path_str = path
            .to_str()
            .unwrap()
            .to_string();
        let path_str = full_path_str.split("/").last().unwrap().to_string();
    
        for ign in ignore.iter() {
            if ign.starts_with("!") {
                if &full_path_str == &ign[1..] {
                    return true;
                }
            }
        }
    
        for ign in ignore.iter() {
            if &full_path_str == ign {
                return false;
            }
            if is_dir {
                if ign.ends_with("/") && path_str.starts_with(&ign[..ign.len()-1]) {
                    return false;
                }
    
            }
            else {
                if ign.starts_with("*.") && path_str.ends_with(&ign[1..]) {
                    return false;
                }
                if ign.ends_with("*") && path_str.starts_with(&ign[..ign.len()-1]){
                    return false; 
                }
                if ign.contains("/*") {
                    let (path, file_ext) = ign.split_once("/*").unwrap();
                    if full_path_str.starts_with(path) && path.ends_with(file_ext) {
                        return false;
                    }
                }
            }
        }
    
        true
    }

    fn append(&mut self, other: &mut FileCollection) {
        self.files.append(&mut other.files);
    }

    fn push(&mut self, elem: FileData) {
        self.files.push(elem);
    }

    pub fn to_bytes(self) -> Vec<u8> {
        let mut res = vec![];

        for file in self.files {
            let (s, mut d) = file.extract();

            // Add string as Vec<u8> to res and add the length of the string after
            let mut s = s.as_bytes().to_vec();
            let s_len = s.len();
            res.append(&mut s);
            let mut s_len = s_len.to_be_bytes().to_vec();
            res.append(&mut s_len);

            // Add file binary contents as Vec<u8> to res and add the length of the binary after
            let d_len = d.len();
            res.append(&mut d);
            let mut d_len = d_len.to_be_bytes().to_vec();
            res.append(&mut d_len);
        }

        res.shrink_to_fit();
        res
    }
}


pub fn create_proj(name: &str) -> Result<String> {
    if OS == "linux" {
        let path_str = format!("/{LINUX_TYNKERBASE_PATH}/{name}");
        let path = Path::new(&path_str);
        if !Path::exists(&path) {
            return Err(anyhow!("Project `{}` already exists", name));
        }
        if let Err(e) = fs::create_dir(&path) {
            return Err(anyhow!("Error creating dir: `{}`", e));
        }
        return Ok(format!("Created `/{LINUX_TYNKERBASE_PATH}/{name}`"));
    }
    Err(anyhow!("OS `{}` is unsupported", OS))
}

pub fn add_files_to_proj(name: &str, files: FileCollection) -> Result<()> {
    if !get_proj_names().contains(&name.to_string()) {
        return Err(anyhow!("Project `{}` does not exist.", {name}));
    }

    if OS == "linux" {
        let proj_path = format!("/{LINUX_TYNKERBASE_PATH}/{name}");
        if let Err(e) = files.save(&proj_path) {
            return Err(anyhow!("{}", e));
        }
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
        _ => vec![]
    }
}

pub fn delete_proj(name: &str) -> Result<()> {
    if OS == "linux" {
        let path = format!("/{LINUX_TYNKERBASE_PATH}/{name}");
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
    let path_str = format!("/{}/{}", LINUX_TYNKERBASE_PATH, name);
    let empty_vec: Vec<String> = vec![];
    let ignore = ignore.unwrap_or(&empty_vec);

    match FileCollection::load(&path_str, &ignore) {
        Ok(fc) => Ok(fc),
        Err(e) => Err(e),
    }
}