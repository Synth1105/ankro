#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    error::Error,
    fs,
    io::{BufReader, Write, prelude::*},
    net::{IpAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::Duration,
};

use crate::{
    ban::BanList,
    queue::{DiskQueue, PendingRequest, RequestQueue, Response},
};

pub mod args;
pub mod ban;
pub mod pool;
pub mod queue;

type SharedQueue = Arc<(Mutex<RequestQueue>, Condvar)>;

const QUEUE_PATH: &str = "/tmp/ankro/queue.json";
pub fn serve(port: u32, target: String, ban_threshold: usize) -> Result<(), Box<dyn Error>> {
    let url = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&url)?;
    let pool = pool::ThreadPool::new(4);
    let target = Arc::new(target);
    let queue = Arc::new((Mutex::new(load_queue()?), Condvar::new()));
    let bans = Arc::new(Mutex::new(BanList::new(ban_threshold)));

    spawn_queue_consumer(Arc::clone(&target), Arc::clone(&queue));

    for stream in listener.incoming() {
        let stream = stream?;
        let ip = stream.peer_addr()?.ip();
        let banned = {
            let mut ban_list = bans.lock().unwrap();
            ban_list.record(ip)
        };
        let target = Arc::clone(&target);
        let queue = Arc::clone(&queue);

        pool.execute(move || {
            if let Err(err) =
                handle_connection(stream, target.as_str(), ip, banned, Arc::clone(&queue))
            {
                eprintln!("connection error: {err}");
            }
        });
    }

    Ok(())
}

pub fn busy(target: &str) -> Result<bool, Box<dyn Error>> {
    let result = Command::new(target).arg("-b").output()?.stdout;
    Ok(!result.is_empty())
}

pub fn handle_connection(
    mut stream: TcpStream,
    target: &str,
    ip: IpAddr,
    banned: bool,
    queue: SharedQueue,
) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request = Vec::new();
    for line in reader.by_ref().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }

        request.push(line);
    }

    if should_queue(target, &queue, banned)? {
        let (tx, rx) = mpsc::channel();
        enqueue_request(&queue, request, Some(tx), ip, banned)?;

        match rx.recv()? {
            Ok(response) => stream.write_all(&response)?,
            Err(err) => stream.write_all(err.as_bytes())?,
        }

        return Ok(());
    }

    let response = run_target(target, &request)?;
    stream.write_all(&response)?;
    Ok(())
}

fn spawn_queue_consumer(target: Arc<String>, queue: SharedQueue) {
    thread::spawn(move || {
        loop {
            let request = match wait_for_request(&queue) {
                Ok(request) => request,
                Err(err) => {
                    eprintln!("queue wait failed: {err}");
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };

            while let Ok(true) = busy(target.as_str()) {
                thread::sleep(Duration::from_millis(100));
            }

            let response = match run_target(target.as_str(), &request.request) {
                Ok(response) => Ok(response),
                Err(err) => Err(err.to_string()),
            };

            if let Some(responder) = request.responder {
                if let Err(err) = responder.send(response) {
                    eprintln!("queue response send failed: {err}");
                }
            }
        }
    });
}

fn wait_for_request(queue: &SharedQueue) -> Result<PendingRequest, Box<dyn Error>> {
    let (lock, cvar) = &**queue;
    let mut guard = lock.lock().unwrap();

    loop {
        if let Some(request) = guard.pop() {
            save_queue_locked(&guard)?;
            return Ok(request);
        }

        guard = cvar.wait(guard).unwrap();
    }
}

fn should_queue(target: &str, queue: &SharedQueue, banned: bool) -> Result<bool, Box<dyn Error>> {
    if banned {
        return Ok(true);
    }

    let (lock, _) = &**queue;
    let guard = lock.lock().unwrap();
    if !guard.is_empty() {
        return Ok(true);
    }

    drop(guard);
    busy(target)
}

fn enqueue_request(
    queue: &SharedQueue,
    request: Vec<String>,
    responder: Option<mpsc::Sender<Response>>,
    ip: IpAddr,
    banned: bool,
) -> Result<(), Box<dyn Error>> {
    let (lock, cvar) = &**queue;
    let mut guard = lock.lock().unwrap();
    guard.push(
        PendingRequest {
            ip: Some(ip),
            request,
            responder,
        },
        banned,
    );
    save_queue_locked(&guard)?;
    cvar.notify_one();
    Ok(())
}

fn run_target(target: &str, request: &[String]) -> Result<Vec<u8>, Box<dyn Error>> {
    let response = Command::new(target)
        .arg("-r")
        .arg(request.join(","))
        .output()?
        .stdout;
    Ok(response)
}

fn load_queue() -> Result<RequestQueue, Box<dyn Error>> {
    let path = PathBuf::from(QUEUE_PATH);
    if !path.try_exists()? {
        return Ok(RequestQueue::new());
    }

    let file = fs::read_to_string(path)?;
    match serde_json::from_str::<DiskQueue>(&file) {
        Ok(disk_queue) => Ok(RequestQueue::from_disk(disk_queue)),
        Err(_) => {
            #[derive(serde::Deserialize)]
            struct LegacyDiskQueue {
                pending: Vec<Vec<String>>,
            }

            let legacy_queue: LegacyDiskQueue = serde_json::from_str(&file)?;
            let disk_queue = DiskQueue {
                normal: legacy_queue
                    .pending
                    .into_iter()
                    .map(|request| crate::queue::StoredRequest { ip: None, request })
                    .collect(),
                banned: Default::default(),
            };

            Ok(RequestQueue::from_disk(disk_queue))
        }
    }
}

fn save_queue_locked(queue: &RequestQueue) -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(QUEUE_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let disk_queue = queue.to_disk();
    let content = serde_json::to_string(&disk_queue)?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}
