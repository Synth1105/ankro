#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    error::Error,
    env,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{atomic::{AtomicU64, Ordering}, Arc},
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
// Keep the live socket/child process count well below common per-process FD limits.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;
static QUEUE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

struct QueueState {
    queue: Mutex<RequestQueue>,
    notify: Notify,
}

type SharedQueue = Arc<QueueState>;

pub async fn serve(port: u32, target: String, ban_threshold: usize) -> Result<(), AnyError> {
    let target = Arc::new(resolve_target(target)?);
    warmup_target(target.as_str()).await?;
    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
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
                tracing::error!("connection error: {err}");
            }
        });
    }
}

pub async fn busy(target: &str) -> Result<bool, AnyError> {
    let result = Command::new(target)
        .arg("-b")
        .output()
        .await
        .map_err(|err| format!("failed to probe target with -b: {target} ({err})"))?
        .stdout;
    Ok(!result.is_empty())
}

async fn handle_connection(
    stream: TcpStream,
    target: &str,
    ip: std::net::IpAddr,
    banned: bool,
    queue: SharedQueue,
) -> Result<(), AnyError> {
    tracing::debug!("handling request");
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
            Err(_) => { 
                tracing::error!("queue comsumer dropped");
                write_half.write_all(b"queue consumer dropped\n").await?;
            },
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
            tracing::debug!("comsumer started");
            let request = match wait_for_request(&queue).await {
                Ok(request) => request,
                Err(err) => {
                    tracing::error!("queue wait failed: {err}");
                    time::sleep(Duration::from_millis(250)).await;
                    continue;
                }
            };

            while busy(target.as_str()).await.unwrap_or(true) {
                time::sleep(Duration::from_millis(100)).await;
            }
            
            tracing::debug!("consuming queue");

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
        let mut guard = queue.queue.lock().await;
        if let Some((request, banned)) = guard.pop() {
            let disk_queue = guard.to_disk();
            if let Err(err) = save_disk_queue(disk_queue).await {
                guard.push_front(request, banned);
                return Err(err);
            }
            return Ok(request);
        }

        drop(guard);
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
    let mut guard = queue.queue.lock().await;
    guard.push(
        PendingRequest {
            ip: Some(ip),
            request,
            responder,
        },
        banned,
    );
    let disk_queue = guard.to_disk();
    save_disk_queue(disk_queue).await?;
    drop(guard);
    queue.notify.notify_one();
    Ok(())
}

async fn run_target(target: &str, request: &[String]) -> Result<Vec<u8>, AnyError> {
    let response = Command::new(target)
        .arg("-r")
        .arg(request.join(","))
        .stdout(Stdio::piped())
        .output()
        .await
        .map_err(|err| format!("failed to execute target with -r: {target} ({err})"))?
        .stdout;
    Ok(response)
}

fn resolve_target(target: String) -> Result<String, AnyError> {
    let looks_like_path = target.contains('/') || target.starts_with('.') || target.starts_with('~');
    let mut candidates = Vec::new();

    if looks_like_path {
        candidates.push(PathBuf::from(&target));
    } else {
        candidates.push(PathBuf::from(&target));
        if let Ok(cwd) = env::current_dir() {
            candidates.push(cwd.join(&target));
        }

        if let Some(paths) = env::var_os("PATH") {
            candidates.extend(env::split_paths(&paths).map(|dir| dir.join(&target)));
        }
    }

    for candidate in candidates {
        match std::fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_file() => {
                return Ok(std::fs::canonicalize(candidate)?.to_string_lossy().into_owned());
            }
            Ok(metadata) if metadata.is_dir() && looks_like_path => {
                return Err(
                    format!("target points to a directory; pass the actual executable path instead: {target}")
                        .into(),
                );
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    Err(format!("target executable not found: {target}").into())
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
    let path_display = path.display().to_string();
    if let Some(parent) = path.parent() {
        tokio_fs::create_dir_all(parent).await?;
    }

    let content = serde_json::to_string(&queue)?;
    let tmp_path = queue_tmp_path(&path);
    tokio_fs::write(&tmp_path, content)
        .await
        .map_err(|err| format!("failed to write queue temp file {} ({err})", tmp_path.display()))?;
    tokio_fs::rename(tmp_path, path)
        .await
        .map_err(|err| format!("failed to persist queue file {path_display} ({err})"))?;
    Ok(())
}

fn queue_tmp_path(path: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = QUEUE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("queue.json");

    path.with_file_name(format!("{file_name}.{stamp}.{sequence}.tmp"))
}

async fn warmup_target(target: &str) -> Result<(), AnyError> {
    match busy(target).await {
        Ok(_) => Ok(()),
        Err(err) => Err(format!("target is not executable or not reachable: {target} ({err})").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{queue_tmp_path, resolve_target};
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(suffix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("ankro-{suffix}-{stamp}-{}", std::process::id()))
    }

    #[test]
    fn resolve_target_returns_canonical_file_path() {
        let path = temp_path("file");
        let mut file = fs::File::create(&path).expect("create temp file");
        writeln!(file, "demo").expect("write temp file");

        let resolved = resolve_target(path.to_string_lossy().into_owned()).expect("resolve file");
        let expected = fs::canonicalize(&path)
            .expect("canonicalize temp file")
            .to_string_lossy()
            .into_owned();

        assert_eq!(resolved, expected);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolve_target_rejects_directories() {
        let path = temp_path("dir");
        fs::create_dir_all(&path).expect("create temp dir");

        let error = resolve_target(path.to_string_lossy().into_owned()).expect_err("reject dir");
        let message = error.to_string();

        assert!(message.contains("directory"), "{message}");

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn queue_tmp_path_is_unique() {
        let path = Path::new("/tmp/ankro/queue.json");
        let first = queue_tmp_path(path);
        let second = queue_tmp_path(path);

        assert_ne!(first, second);
        assert_eq!(first.parent(), path.parent());
        assert_eq!(second.parent(), path.parent());
        assert!(first
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("queue.json.")));
        assert!(second
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("queue.json.")));
    }
}
