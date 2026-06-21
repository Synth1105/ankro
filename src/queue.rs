//! In-memory and persisted queue types used by `ankro`.

use std::{collections::VecDeque, fmt::Display, net::IpAddr};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::Sender;

/// A queued response returned to the waiting client.
pub type Response = Result<Vec<u8>, String>;

/// A request record that can be written to disk.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredRequest {
    /// Source IP associated with the request, if known.
    #[serde(default)]
    pub ip: Option<IpAddr>,
    /// The request payload lines.
    pub request: Vec<String>,
}

/// A request waiting in memory for the queue consumer.
#[derive(Debug)]
pub struct PendingRequest {
    /// Source IP associated with the request, if known.
    pub ip: Option<IpAddr>,
    /// The request payload lines.
    pub request: Vec<String>,
    /// One-shot channel used to return the response to the waiting caller.
    pub responder: Option<Sender<Response>>,
}

impl Display for PendingRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ip_str = match self.ip {
            Some(ip) => ip.to_string(),
            None => "Unknown".to_string(),
        };

        let responder_str = match self.responder {
            Some(_) => "Available",
            None => "None",
        };

        write!(
            f,
            "PendingRequest {{ IP: {}, Requests: {:?}, Responder: {} }}",
            ip_str, self.request, responder_str
        )
    }
}

/// FIFO request queues split into normal and banned lanes.
#[derive(Default, Debug)]
pub struct RequestQueue {
    normal: VecDeque<PendingRequest>,
    banned: VecDeque<PendingRequest>,
}

/// Serializable snapshot of the queue state.
#[derive(Default, Serialize, Deserialize)]
pub struct DiskQueue {
    /// Requests from non-banned clients.
    #[serde(default)]
    pub normal: VecDeque<StoredRequest>,
    /// Requests from banned clients.
    #[serde(default)]
    pub banned: VecDeque<StoredRequest>,
}

impl RequestQueue {
    /// Create an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a request to the normal or banned lane.
    pub fn push(&mut self, request: PendingRequest, banned: bool) {
        tracing::debug!("pushing queue with request {request}");
        if banned {
            self.banned.push_back(request);
        } else {
            self.normal.push_back(request);
        }
    }

    /// Remove the next request, preferring normal traffic over banned traffic.
    pub fn pop(&mut self) -> Option<(PendingRequest, bool)> {
        tracing::debug!("poping queue");
        self.normal
            .pop_front()
            .map(|request| (request, false))
            .or_else(|| self.banned.pop_front().map(|request| (request, true)))
    }

    /// Reinsert a request at the front of the selected lane.
    pub fn push_front(&mut self, request: PendingRequest, banned: bool) {
        if banned {
            self.banned.push_front(request);
        } else {
            self.normal.push_front(request);
        }
    }

    /// Return `true` when both lanes are empty.
    pub fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.banned.is_empty()
    }

    /// Convert a persisted snapshot into an in-memory queue.
    pub fn from_disk(disk: DiskQueue) -> Self {
        tracing::debug!("loading queue from disk");
        let normal = disk
            .normal
            .into_iter()
            .map(|request| PendingRequest {
                ip: request.ip,
                request: request.request,
                responder: None,
            })
            .collect();

        let banned = disk
            .banned
            .into_iter()
            .map(|request| PendingRequest {
                ip: request.ip,
                request: request.request,
                responder: None,
            })
            .collect();

        Self { normal, banned }
    }

    /// Convert the in-memory queue into a persisted snapshot.
    pub fn to_disk(&self) -> DiskQueue {
        tracing::info!("saving queue to disk");
        let normal = self
            .normal
            .iter()
            .map(|request| StoredRequest {
                ip: request.ip,
                request: request.request.clone(),
            })
            .collect();

        let banned = self
            .banned
            .iter()
            .map(|request| StoredRequest {
                ip: request.ip,
                request: request.request.clone(),
            })
            .collect();

        DiskQueue { normal, banned }
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingRequest, RequestQueue};

    #[test]
    fn normal_requests_are_drained_before_banned_requests() {
        let mut queue = RequestQueue::new();
        queue.push(
            PendingRequest {
                ip: None,
                request: vec!["banned".into()],
                responder: None,
            },
            true,
        );
        queue.push(
            PendingRequest {
                ip: None,
                request: vec!["normal".into()],
                responder: None,
            },
            false,
        );

        let (first, first_banned) = queue.pop().unwrap();
        let (second, second_banned) = queue.pop().unwrap();

        assert_eq!(first.request, vec!["normal"]);
        assert_eq!(second.request, vec!["banned"]);
        assert!(!first_banned);
        assert!(second_banned);
        assert!(queue.is_empty());
    }
}
