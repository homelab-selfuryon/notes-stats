use std::sync::{Arc, atomic::AtomicBool};

use crate::{config::Config, notes_api::NotesApi};

/// Shared by all handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// `/readyz` answers 200 while this is true.
    pub ready: Arc<AtomicBool>,
    pub notes_api: Arc<NotesApi>,
}

impl AppState {
    pub fn new(config: Config, notes_api: NotesApi) -> Self {
        Self {
            config: Arc::new(config),
            ready: Arc::new(AtomicBool::new(true)),
            notes_api: Arc::new(notes_api),
        }
    }
}
