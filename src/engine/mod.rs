//! The engine plays music and keeps the [`Store`] current; the UI reads the store and sends
//! [`Command`]s. [`Engine::demo`] is a stand-in with canned data, for headless renders and
//! UI work without an account.

pub mod analysis;
mod demo;
mod demo_art;
mod images;
#[cfg(feature = "spotify")]
mod spotify;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::model::{Command, Store};

pub use images::ImageBytes;

type Listener = Box<dyn Fn() + Send + Sync>;

/// A cheap handle; clones share one engine.
#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
    backend: Arc<dyn Backend>,
}

/// What a backend does with the UI's requests. Both run on the caller's thread and must
/// return at once.
pub(crate) trait Backend: Send + Sync {
    fn command(&self, command: Command);
    /// Fetch `url` and hand its bytes to [`Shared::image_arrived`].
    fn fetch_image(&self, url: String);
}

pub(crate) struct Shared {
    store: Mutex<Store>,
    listeners: Mutex<Vec<Listener>>,
    images: images::Images,
    analysis: analysis::Analysis,
}

impl Shared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(Store::default()),
            listeners: Mutex::new(Vec::new()),
            images: images::Images::default(),
            analysis: analysis::Analysis::new(),
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Change the store, then tell every listener.
    pub(crate) fn update<R>(&self, change: impl FnOnce(&mut Store) -> R) -> R {
        let result = change(&mut self.store());
        self.notify();
        result
    }

    pub(crate) fn read<R>(&self, read: impl FnOnce(&Store) -> R) -> R {
        read(&self.store())
    }

    pub(crate) fn notify(&self) {
        let listeners = self.listeners.lock().unwrap_or_else(PoisonError::into_inner);
        for listener in listeners.iter() {
            listener();
        }
    }

    /// `None` records a failed fetch, so the UI stops asking for it.
    pub(crate) fn image_arrived(&self, url: &str, bytes: Option<Vec<u8>>) {
        self.images.arrived(url, bytes);
        self.notify();
    }
}

impl Engine {
    /// Canned library and a pretend player that answers every command.
    pub fn demo() -> Self {
        let shared = Shared::new();
        let backend = demo::start(shared.clone());
        Self { shared, backend }
    }

    /// The real thing: a Spotify Connect device with the account's library. It starts by
    /// reading saved credentials; [`SessionState`](crate::model::SessionState) says how
    /// that went.
    #[cfg(feature = "spotify")]
    pub fn spotify() -> std::io::Result<Self> {
        let shared = Shared::new();
        let backend = spotify::start(shared.clone())?;
        Ok(Self { shared, backend })
    }

    /// `--check`: every Spotify service the app uses, called once and reported on stdout.
    /// Returns the exit code.
    #[cfg(feature = "spotify")]
    pub fn check(json: bool) -> i32 {
        spotify::check::run(json)
    }

    /// Read the store. Keep the closure short: the engine's threads wait on the same lock.
    pub fn read<R>(&self, read: impl FnOnce(&Store) -> R) -> R {
        self.shared.read(read)
    }

    /// The visualizers' audio analysis. It runs only when stepped; see [`analysis`].
    pub fn analysis(&self) -> analysis::Analysis {
        self.shared.analysis.clone()
    }

    pub fn send(&self, command: Command) {
        self.backend.command(command);
    }

    /// Run `listener` after every store change and every image arrival. It runs on an
    /// engine thread, so it should only wake the UI (`AppHandle::update`).
    pub fn subscribe(&self, listener: impl Fn() + Send + Sync + 'static) {
        self.shared.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(Box::new(listener));
    }

    /// The encoded image (JPEG or PNG) at `url`, once fetched. The first call for a URL
    /// starts the fetch and returns `None`; listeners run when it lands. A URL that failed
    /// stays `None` and is fetched again only after a pause.
    pub fn image(&self, url: &str) -> Option<ImageBytes> {
        match self.shared.images.get(url) {
            images::Lookup::Ready(bytes) => Some(bytes),
            images::Lookup::Pending => None,
            images::Lookup::Missing => {
                self.backend.fetch_image(url.to_owned());
                None
            }
        }
    }
}
