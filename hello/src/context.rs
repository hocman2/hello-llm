use parking_lot::Mutex;
use std::sync::Arc;

struct SharedState {
    piped: Option<String>,
    initial_prompt: String,
}

#[derive(Clone)]
pub struct Context {
    shared_state: Arc<Mutex<SharedState>>,
}

impl Context {
    pub fn new(initial_prompt: String, piped: Option<String>) -> Self {
        Self {
            shared_state: Arc::new(Mutex::new(SharedState {
                piped,
                initial_prompt,
            })),
        }
    }

    pub fn get_piped_input(&self) -> Option<String> {
        self.shared_state.lock().piped.clone()
    }

    pub fn has_piped(&self) -> bool {
        self.shared_state.lock().piped.is_some()
    }

    pub fn get_initial_prompt(&self) -> String {
        self.shared_state.lock().initial_prompt.clone()
    }
}
