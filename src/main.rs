use std::fs;

mod docker_wrapper;
mod proj_files;

fn main() {
    let ignore = vec!["target/".to_string()];
    let ignore: Vec<String> = vec![];
    fs::remove_dir_all("/home/aknen/Documents/proj");

    let mut v: Vec<u8> = (0..200).collect();
    let mut d = [0_u8; 8];
    d.copy_from_slice(&v[v.len()-8..v.len()]);

    v.truncate(v.len()-8);
    println!("{:?}\n\n{:?}", v, d);

    println!("\n\n{}", usize::from_be_bytes(d));



}
