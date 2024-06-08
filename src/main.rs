mod docker_wrapper;
mod proj_files;

fn main() {
    docker_wrapper::end_daemon()
        .unwrap();

    let status = docker_wrapper::get_engine_status();
    
    println!("{:?}", status);

}
