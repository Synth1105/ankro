use std::{collections::VecDeque, net::IpAddr};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::Sender;

pub type Response = Result<Vec<u8>, String>;

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredRequest {
    #[serde(default)]
    pub ip: Option<IpAddr>,
    pub request: Vec<String>,
}

pub struct PendingRequest {
    pub ip: Option<IpAddr>,
    pub request: Vec<String>,
    pub responder: Option<Sender<Response>>,
}

#[derive(Default)]
pub struct RequestQueue {
    normal: VecDeque<PendingRequest>,
    banned: VecDeque<PendingRequest>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct DiskQueue {
    #[serde(default)]
    pub normal: VecDeque<StoredRequest>,
    #[serde(default)]
    pub banned: VecDeque<StoredRequest>,
}

impl RequestQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, request: PendingRequest, banned: bool) {
        if banned {
            self.banned.push_back(request);
        } else {
            self.normal.push_back(request);
        }
    }

    pub fn pop(&mut self) -> Option<(PendingRequest, bool)> {
        self.normal
            .pop_front()
            .map(|request| (request, false))
            .or_else(|| self.banned.pop_front().map(|request| (request, true)))
    }

    pub fn push_front(&mut self, request: PendingRequest, banned: bool) {
        if banned {
            self.banned.push_front(request);
        } else {
            self.normal.push_front(request);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.banned.is_empty()
    }

    pub fn from_disk(disk: DiskQueue) -> Self {
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

    pub fn to_disk(&self) -> DiskQueue {
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
