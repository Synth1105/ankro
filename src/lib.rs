#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc; // allocator change for memory performance

use std::{
    error::Error,
    fs,
    io::{BufReader, Write, prelude::*},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crate::queue::{DiskQueue, PendingRequest, RequestQueue, Response};

pub mod args;
pub mod pool;
pub mod queue;

type SharedQueue = Arc<(Mutex<RequestQueue>, Condvar)>;

const QUEUE_PATH: &str = "/tmp/ankro/queue.json";

pub fn serve(port: u32, target: String) -> Result<(), Box<dyn Error>> {
    let url = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&url)?;
    let pool = pool::ThreadPool::new(4);
    let target = Arc::new(target);
    let queue = Arc::new((Mutex::new(load_queue()?), Condvar::new()));
    let queue_mode = Arc::new(AtomicBool::new(false));

    spawn_queue_consumer(
        Arc::clone(&target),
        Arc::clone(&queue),
        Arc::clone(&queue_mode),
    );

    for stream in listener.incoming() {
        let stream = stream?;
        let target = Arc::clone(&target);
        let queue = Arc::clone(&queue);
        let queue_mode = Arc::clone(&queue_mode);

        pool.execute(move || {
            if let Err(err) = handle_connection(
                stream,
                target.as_str(),
                Arc::clone(&queue),
                Arc::clone(&queue_mode),
            ) {
                eprintln!("connection error: {err}");
            }
        });
    }

    Ok(())
}

pub fn busy(target: &str) -> Result<bool, Box<dyn Error>> {
    let result = Command::new(target).arg("-b").output()?.stdout;
    if result.is_empty() {
        Ok(false)
    } else {
        Ok(true)
    }
}

pub fn handle_connection(
    mut stream: TcpStream,
    target: &str,
    queue: SharedQueue,
    queue_mode: Arc<AtomicBool>,
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

    if should_queue(target, &queue, &queue_mode)? {
        let (tx, rx) = mpsc::channel();
        enqueue_request(&queue, &queue_mode, request, Some(tx))?;

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

fn spawn_queue_consumer(target: Arc<String>, queue: SharedQueue, queue_mode: Arc<AtomicBool>) {
    thread::spawn(move || {
        loop {
            let request = match wait_for_request(&queue, &queue_mode) {
                Ok(request) => request,
                Err(err) => {
                    eprintln!("queue wait failed: {err}");
                    thread::sleep(Duration::from_millis(250));
                    continue;
                }
            };

            let PendingRequest {
                request: request_payload,
                responder,
            } = request;
            let fallback_responder = responder.clone();

            while let Ok(true) = busy(target.as_str()) {
                thread::sleep(Duration::from_millis(100));
            }

            let response = match run_target(target.as_str(), &request_payload) {
                Ok(response) => Ok(response),
                Err(err) => Err(err.to_string()),
            };

            match response {
                Ok(response) => {
                    if let Some(responder) = responder
                        && let Err(err) = responder.send(Ok(response))
                    {
                        eprintln!("queue response send failed: {err}");
                    }
                }
                Err(err) => {
                    eprintln!("queue request failed: {err}");

                    let pending_request = PendingRequest {
                        request: request_payload,
                        responder,
                    };

                    if let Err(requeue_err) =
                        requeue_failed_request(&queue, &queue_mode, pending_request)
                    {
                        eprintln!("queue requeue failed: {requeue_err}");

                        if let Some(responder) = fallback_responder
                            && let Err(send_err) = responder.send(Err(err))
                        {
                            eprintln!("queue response send failed: {send_err}");
                        }
                    }
                }
            }
        }
    });
}

fn wait_for_request(
    queue: &SharedQueue,
    queue_mode: &Arc<AtomicBool>,
) -> Result<PendingRequest, Box<dyn Error>> {
    let (lock, cvar) = &**queue;
    let mut guard = lock.lock().unwrap();

    loop {
        if let Some(request) = guard.pop() {
            save_queue_locked(&guard)?;
            queue_mode.store(true, Ordering::SeqCst);
            return Ok(request);
        }

        queue_mode.store(false, Ordering::SeqCst);
        guard = cvar.wait(guard).unwrap();
    }
}

fn should_queue(
    target: &str,
    queue: &SharedQueue,
    queue_mode: &Arc<AtomicBool>,
) -> Result<bool, Box<dyn Error>> {
    if queue_mode.load(Ordering::SeqCst) {
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
    queue_mode: &Arc<AtomicBool>,
    request: Vec<String>,
    responder: Option<mpsc::Sender<Response>>,
) -> Result<(), Box<dyn Error>> {
    let (lock, cvar) = &**queue;
    let mut guard = lock.lock().unwrap();
    guard.push(PendingRequest { request, responder });
    save_queue_locked(&guard)?;
    queue_mode.store(true, Ordering::SeqCst);
    cvar.notify_one();
    Ok(())
}

fn requeue_failed_request(
    queue: &SharedQueue,
    queue_mode: &Arc<AtomicBool>,
    request: PendingRequest,
) -> Result<(), Box<dyn Error>> {
    let (lock, cvar) = &**queue;
    let mut guard = lock.lock().unwrap();
    guard.pending.push_front(request);
    save_queue_locked(&guard)?;
    queue_mode.store(true, Ordering::SeqCst);
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
                pending: legacy_queue
                    .pending
                    .into_iter()
                    .map(|request| crate::queue::StoredRequest { request })
                    .collect(),
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
