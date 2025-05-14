use parking_lot::Mutex;
use std::sync::Arc;
use crate::cli::Config;
use llm_int::LLMContext;

#[allow(unused)]
struct SharedState {
    piped: Option<String>,
    initial_prompt: String,
    config: Config,
    llm_ctx: LLMContext,
}

#[derive(Clone)]
pub struct Context {
    shared_state: Arc<Mutex<SharedState>>,
}

impl Context {
    pub fn new(initial_prompt: String, piped: Option<String>, config: Config, llm_ctx: LLMContext) -> Self {
        Self {
            shared_state: Arc::new(Mutex::new(SharedState {
                piped,
                initial_prompt,
                config,
                llm_ctx
            })),
        }
    }

    pub fn get_piped_input(&self) -> Option<String> {
        self.shared_state.lock().piped.clone()
    }

    pub fn get_initial_prompt(&self) -> String {
        self.shared_state.lock().initial_prompt.clone()
    }

    pub fn get_llm(&self) -> LLMContext {
        self.shared_state.lock().llm_ctx.clone()
    }
}
