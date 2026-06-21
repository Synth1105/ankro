#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    error::Error,
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use tokio::{
    fs as tokio_fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::Command,
    sync::{oneshot, Mutex, Notify},
    time,
};

use crate::{
    ban::BanList,
    queue::{DiskQueue, PendingRequest, RequestQueue, Response},
};

pub mod args;
pub mod ban;
pub mod queue;

type AnyError = Box<dyn Error + Send + Sync>;

const QUEUE_PATH: &str = "/tmp/ankro/queue.json";
const MAX_CONCURRENT_CONNECTIONS: usize = 256;

struct QueueState {
    queue: Mutex<RequestQueue>,
    notify: Notify,
}

type SharedQueue = Arc<QueueState>;

pub async fn serve(port: u32, target: String, ban_threshold: usize) -> Result<(), AnyError> {
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    let target = Arc::new(target);
    let queue = Arc::new(QueueState {
        queue: Mutex::new(load_queue().await?),
        notify: Notify::new(),
    });
    let bans = Arc::new(Mutex::new(BanList::new(ban_threshold)));
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    spawn_queue_consumer(Arc::clone(&target), Arc::clone(&queue));

    loop {
        let (stream, addr) = listener.accept().await?;
        let ip = addr.ip();
        let permit = Arc::clone(&permits).acquire_owned().await?;
        let banned = {
            let mut ban_list = bans.lock().await;
            ban_list.record(ip)
        };
        let target = Arc::clone(&target);
        let queue = Arc::clone(&queue);

        tokio::spawn(async move {
            let _permit = permit;
            if let Err(err) = handle_connection(stream, target.as_str(), ip, banned, queue).await
            {
                eprintln!("connection error: {err}");
            }
        });
    }
}

pub async fn busy(target: &str) -> Result<bool, AnyError> {
    let result = Command::new(target).arg("-b").output().await?.stdout;
    Ok(!result.is_empty())
}

async fn handle_connection(
    stream: TcpStream,
    target: &str,
    ip: std::net::IpAddr,
    banned: bool,
    queue: SharedQueue,
) -> Result<(), AnyError> {
    let (read_half, mut write_half) = stream.into_split();
    let reader = BufReader::new(read_half);
    let mut lines = reader.lines();

    let mut request = Vec::new();
    while let Some(line) = lines.next_line().await? {
        if line.is_empty() {
            break;
        }

        request.push(line);
    }

    if should_queue(target, &queue, banned).await? {
        let (tx, rx) = oneshot::channel();
        enqueue_request(&queue, request, Some(tx), ip, banned).await?;

        match rx.await {
            Ok(Ok(response)) => write_half.write_all(&response).await?,
            Ok(Err(err)) => write_half.write_all(err.as_bytes()).await?,
            Err(_) => write_half.write_all(b"queue consumer dropped\n").await?,
        }

        return Ok(());
    }

    let response = run_target(target, &request).await?;
    write_half.write_all(&response).await?;
    Ok(())
}

fn spawn_queue_consumer(target: Arc<String>, queue: SharedQueue) {
    tokio::spawn(async move {
        loop {
            let request = match wait_for_request(&queue).await {
                Ok(request) => request,
                Err(err) => {
                    eprintln!("queue wait failed: {err}");
                    time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
            };

            while busy(target.as_str()).await.unwrap_or(true) {
                time::sleep(Duration::from_millis(100)).await;
            }

            let response = match run_target(target.as_str(), &request.request).await {
                Ok(response) => Ok(response),
                Err(err) => Err(err.to_string()),
            };

            if let Some(responder) = request.responder {
                let _ = responder.send(response);
            }
        }
    });
}

async fn wait_for_request(queue: &SharedQueue) -> Result<PendingRequest, AnyError> {
    loop {
        let maybe_request = {
            let mut guard = queue.queue.lock().await;
            let request = guard.pop();
            if request.is_some() {
                let disk_queue = guard.to_disk();
                Some((request, disk_queue))
            } else {
                None
            }
        };

        if let Some((Some(request), disk_queue)) = maybe_request {
            save_disk_queue(disk_queue).await?;
            return Ok(request);
        }

        queue.notify.notified().await;
    }
}

async fn should_queue(target: &str, queue: &SharedQueue, banned: bool) -> Result<bool, AnyError> {
    if banned {
        return Ok(true);
    }

    let queue_has_items = {
        let guard = queue.queue.lock().await;
        !guard.is_empty()
    };
    if queue_has_items {
        return Ok(true);
    }

    busy(target).await
}

async fn enqueue_request(
    queue: &SharedQueue,
    request: Vec<String>,
    responder: Option<oneshot::Sender<Response>>,
    ip: std::net::IpAddr,
    banned: bool,
) -> Result<(), AnyError> {
    let disk_queue = {
        let mut guard = queue.queue.lock().await;
        guard.push(
            PendingRequest {
                ip: Some(ip),
                request,
                responder,
            },
            banned,
        );
        guard.to_disk()
    };

    save_disk_queue(disk_queue).await?;
    queue.notify.notify_one();
    Ok(())
}

async fn run_target(target: &str, request: &[String]) -> Result<Vec<u8>, AnyError> {
    let response = Command::new(target)
        .arg("-r")
        .arg(request.join(","))
        .stdout(Stdio::piped())
        .output()
        .await?
        .stdout;
    Ok(response)
}

async fn load_queue() -> Result<RequestQueue, AnyError> {
    let path = PathBuf::from(QUEUE_PATH);
    if !tokio_fs::try_exists(&path).await? {
        return Ok(RequestQueue::new());
    }

    let file = tokio_fs::read_to_string(&path).await?;
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

async fn save_disk_queue(queue: DiskQueue) -> Result<(), AnyError> {
    let path = PathBuf::from(QUEUE_PATH);
    if let Some(parent) = path.parent() {
        tokio_fs::create_dir_all(parent).await?;
    }

    let content = serde_json::to_string(&queue)?;
    let tmp_path = path.with_extension("json.tmp");
    tokio_fs::write(&tmp_path, content).await?;
    tokio_fs::rename(tmp_path, path).await?;
    Ok(())
}
