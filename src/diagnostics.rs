use tynkerbase_universal::netwk_utils::NodeDiags;
use std::{fs, ops::Deref, sync::{Arc, Mutex}};
use tokio::process::Command;



pub async fn measure(node_id: &str, name: &str) -> NodeDiags {
    let nd: Arc<Mutex<NodeDiags>> = Arc::new(Mutex::new(NodeDiags::new(node_id, name)));

    tokio::join!(
        get_cpu_data(nd.clone()),
        get_manufacturer(nd.clone()),
    );
    get_mem_data(nd.clone());
    
    let lock = nd.lock().unwrap();

    lock.deref().clone()
}


async fn get_manufacturer(diags: Arc<Mutex<NodeDiags>>) {
    let output = Command::new("sh")
        .arg("-c")
        .arg("dmidecode -s system-manufacturer")
        .output();

    match output.await {
        Ok(o) => {
            let s = String::from_utf8(o.stdout);
            match s {
                Ok(s) => {
                    let mut lock = diags.lock().unwrap();
                    lock.manufacturer = Some(s);
                },
                _ => {},
            }
        }
        _ => {},
    }
}

async fn get_cpu_data(diags: Arc<Mutex<NodeDiags>>) {
    fn extract_from_line(mut line: &str) -> String {
        line = line.trim();
        line
            .split_once(":")
            .unwrap().1
            .trim()
            .to_string()
    }
    let output = Command::new("lscpu")
        .output()
        .await;
    let output = match output {
        Ok(o) => o,
        _ => return,
    };
    let output = match String::from_utf8(output.stdout) {
        Ok(o) => o,
        _ => return,
    };
    let mut lock = diags.lock().unwrap();
    for mut line in output.split("\n") {
        line = line.trim();
        if line.starts_with("Architecture") {
            lock.cpu_arc = Some(extract_from_line(line));
        }
        else if line.starts_with("CPU(s)") {
            let num = extract_from_line(line).parse::<usize>();
            let num = match num {
                Ok(n) => n,
                _ => continue,
            };
            lock.hardware_threads = Some(num);
        }
        else if line.starts_with("Model name") {
            lock.cpu = Some(extract_from_line(line));
        }
        else if line.starts_with("L1d") {
            lock.l1_cache_d = Some(extract_from_line(line));
        }
        else if line.starts_with("L1i") {
            lock.l1_cache_i = Some(extract_from_line(line));
        }
        else if line.starts_with("L2") {
            lock.l2_cache = Some(extract_from_line(line));
        }
        else if line.starts_with("L3") {
            lock.l3_cache = Some(extract_from_line(line));
        }
    }
}

fn get_mem_data(diags: Arc<Mutex<NodeDiags>>) {
    let text = fs::read_to_string("/proc/meminfo");
    if let Ok(text) = text {
        let mut lock = diags.lock().unwrap();
        
        let mut num_added = 0;
        for line in text.split("\n") {
            if line.starts_with("MemTotal") {
                let num = line
                    .split_once(":")
                    .unwrap().1
                    .trim()
                    .split_once(" ")
                    .unwrap().0
                    .to_string()
                    .parse::<usize>()
                    .unwrap();
                lock.mem_total = Some(num as f64 / 1_000_000.);
                num_added += 1;
            }
            else if line.starts_with("MemFree") {
                let num = line
                    .split_once(":")
                    .unwrap().1
                    .trim()
                    .split_once(" ")
                    .unwrap().0
                    .to_string()
                    .parse::<usize>()
                    .unwrap();
                lock.mem_free = Some(num as f64 / 1_000_000.);
                num_added += 1;
            }
            if num_added > 2 {
                break;
            }
        }


    }
}
