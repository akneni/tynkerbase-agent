use proj_files::{load_files_from_disk, save_files_to_disk};

mod docker_wrapper;
mod proj_files;

fn main() {
    let res = load_files_from_disk("./")
        .unwrap();

    std::fs::create_dir("/home/aknen/Documents/proj");
    save_files_to_disk(&res,"/home/aknen/Documents/proj")
        .unwrap();
}
