use std::{
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
};

pub struct ThreadPool {
    _threads: Vec<Worker>,
    sender: mpsc::Sender<Job>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (tx, rx) = mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));

        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&rx)));
        }

        ThreadPool {
            _threads: workers,
            sender: tx,
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        let _ = self.sender.send(job);
    }
}

pub struct Worker {
    _id: usize,
    _thread: JoinHandle<()>,
}

impl Worker {
    pub fn new(id: usize, rx: Arc<Mutex<mpsc::Receiver<Job>>>) -> Self {
        let thread = thread::spawn(move || {
            loop {
                let job = match rx.lock().unwrap().recv() {
                    Ok(job) => job,
                    Err(_) => break,
                };

                job();
            }
        });

        Self {
            _id: id,
            _thread: thread,
        }
    }
}
