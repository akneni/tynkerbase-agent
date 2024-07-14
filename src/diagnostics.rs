use std::process::Command;

#[allow(unused)]
struct NodeDiags {
    id: String,
    name: String,
    manufacturer: Option<String>,
    cpu: Option<String>,
}

#[allow(unused)]
fn get_manufacturer() -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("dmidecode -s system-manufacturer")
        .output();

    match output {
        Ok(o) => {
            let s = String::from_utf8(o.stdout);
            match s {
                Ok(s) => Some(s),
                _ => None,
            }
        }
        _ => None,
    }
}

#[allow(unused)]
fn get_cpu_data() {
    let output = Command::new("lscpu").output();
}
