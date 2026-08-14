use crate::search::download_audio;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

pub struct DownloadRequest {
    pub id: String,
    pub title: String,
    pub url: String,
    pub path: String,
    pub video_cache_plan: Option<(u16, u16, u16)>,
}

pub enum DownloadEvent {
    Started {
        id: String,
    },
    Finished {
        id: String,
        title: String,
        path: String,
        error: Option<String>,
    },
}

pub struct DownloadManager {
    requests: Sender<DownloadRequest>,
    events: Receiver<DownloadEvent>,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<DownloadRequest>();
        let (event_tx, event_rx) = mpsc::channel::<DownloadEvent>();
        std::thread::spawn(move || run_worker(request_rx, event_tx));
        Self {
            requests: request_tx,
            events: event_rx,
        }
    }

    pub fn enqueue(&self, request: DownloadRequest) -> Result<(), DownloadRequest> {
        self.requests.send(request).map_err(|error| error.0)
    }

    pub fn try_recv(&self) -> Result<DownloadEvent, mpsc::TryRecvError> {
        self.events.try_recv()
    }
}

fn run_worker(requests: Receiver<DownloadRequest>, events: Sender<DownloadEvent>) {
    while let Ok(request) = requests.recv() {
        if events
            .send(DownloadEvent::Started {
                id: request.id.clone(),
            })
            .is_err()
        {
            return;
        }
        let result = retry_download(&request, 3);
        let (path, error) = match result {
            Ok(path) => (path.to_string_lossy().into_owned(), None),
            Err(error) => (request.path, Some(error)),
        };
        if events
            .send(DownloadEvent::Finished {
                id: request.id,
                title: request.title,
                path,
                error,
            })
            .is_err()
        {
            return;
        }
    }
}

fn retry_download(request: &DownloadRequest, attempts: usize) -> Result<PathBuf, String> {
    let mut errors = Vec::new();
    for attempt in 1..=attempts.max(1) {
        let result = std::panic::catch_unwind(|| {
            download_audio(
                &request.url,
                &request.title,
                Path::new(&request.path),
                request.video_cache_plan,
            )
        })
        .unwrap_or_else(|_| Err("the download process stopped unexpectedly".to_string()));
        match result {
            Ok(path) => return Ok(path),
            Err(error) => errors.push(format!("attempt {attempt}: {error}")),
        }
    }
    Err(errors.join("; "))
}
