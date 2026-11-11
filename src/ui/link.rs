//! The windows' handle on the engine. Tests swap in a recorder to see what the UI asked for.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::engine::{Engine, ImageBytes};
use crate::model::{Command, Store};

#[derive(Clone)]
pub struct Link {
    engine: Engine,
    sent: Option<Arc<Mutex<Vec<Command>>>>,
}

impl Link {
    pub fn new(engine: Engine) -> Self {
        Self { engine, sent: None }
    }

    /// A link that also keeps every command it sends, for tests.
    #[cfg(test)]
    pub fn recording(engine: Engine) -> (Self, Arc<Mutex<Vec<Command>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        (Self { engine, sent: Some(sent.clone()) }, sent)
    }

    pub fn send(&self, command: Command) {
        if let Some(sent) = &self.sent {
            sent.lock().unwrap_or_else(PoisonError::into_inner).push(command.clone());
        }
        self.engine.send(command);
    }

    /// Never call [`Link::image`] or [`Link::send`] inside `read`: the engine's listeners run
    /// on the calling thread and may read the store themselves.
    pub fn read<R>(&self, read: impl FnOnce(&Store) -> R) -> R {
        self.engine.read(read)
    }

    pub fn image(&self, url: &str) -> Option<ImageBytes> {
        self.engine.image(url)
    }

    pub fn subscribe(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.engine.subscribe(listener);
    }

    /// Sends `command` and waits up to a second for `done`. Only headless scenes use this, so a
    /// snapshot shows the answer rather than a race.
    pub fn send_and_wait(&self, command: Command, done: impl Fn(&Store) -> bool) {
        self.send(command);
        self.wait_for(done);
    }

    /// Waits up to a second for `done`; for headless scenes only.
    pub fn wait_for(&self, done: impl Fn(&Store) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !self.read(&done) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
