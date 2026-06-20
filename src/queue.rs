

use std::{collections::VecDeque, sync::mpsc::Sender};

use serde::{Deserialize, Serialize};

pub type Response = Result<Vec<u8>, String>;

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredRequest {
    pub request: Vec<String>,
}

pub struct PendingRequest {
    pub request: Vec<String>,
    pub responder: Option<Sender<Response>>,
}

#[derive(Default)]
pub struct RequestQueue {
    pub pending: VecDeque<PendingRequest>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct DiskQueue {
    pub pending: VecDeque<StoredRequest>,
}

impl RequestQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, request: PendingRequest) {
        self.pending.push_back(request);
    }

    pub fn pop(&mut self) -> Option<PendingRequest> {
        self.pending.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn from_disk(disk: DiskQueue) -> Self {
        let pending = disk
            .pending
            .into_iter()
            .map(|request| PendingRequest {
                request: request.request,
                responder: None,
            })
            .collect();

        Self { pending }
    }

    pub fn to_disk(&self) -> DiskQueue {
        let pending = self
            .pending
            .iter()
            .map(|request| StoredRequest {
                request: request.request.clone(),
            })
            .collect();

        DiskQueue { pending }
    }
}
