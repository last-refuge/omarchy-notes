//! A bounded port from instance workers to the owning UI thread.
use notes_core::launch::{ActivationToken, Intent, Reply};
use std::{sync::mpsc, time::Instant};

pub struct Request {
    pub id: String,
    pub intent: Intent,
    pub token: Option<ActivationToken>,
    pub expires: Instant,
    pub reply: mpsc::SyncSender<Reply>,
}
pub type Sender = mpsc::SyncSender<Request>;
pub type Receiver = mpsc::Receiver<Request>;
pub fn channel() -> (Sender, Receiver) {
    mpsc::sync_channel(8)
}
