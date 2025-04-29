pub mod openai;

use serde::{Serialize, Deserialize};
use http::Request;
use std::sync::Arc;

pub struct Defaults;
impl Defaults {
    const MAX_COMPLETION_TOKENS: u32 = 4096;
    const NUM_GENS: u32 = 1;
}

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum Provider {
    OpenAi,
}

pub struct LLMResponse(pub String);

// A thin wrapper over provider specific API contexts
// I find it more convenient to have a vtable over passing generic arguments and pollute other
// parts with provider specific stuff
#[derive(Clone)]
pub struct LLMContext {
    api: Arc<dyn LLMApi + Send + Sync>,
}

impl LLMContext {
    pub fn new(provider: Provider, model: String, key: String) -> Self {
        match provider {
            Provider::OpenAi => {
                let api = Arc::new(openai::chat_completion_api::ApiContext::new(model, key));
                Self { api }
            }        
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all="snake_case")]
pub enum Role {Assistant, User, Developer}

#[derive(Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    // Returns a vec of a single message intended for
    // an initial request. 
    pub fn new_user_request(content: String) -> Vec<Self> {
        vec![Message {
            role: Role::User,
            content,
        }]
    } 

    pub fn from_history(content: &Vec<(Role, String)>) -> Vec<Self> {
        content.iter()
            .map(|(r, s)| {
                Message {
                    role: r.clone(),
                    content: s.clone(),
                }
            })
            .collect()
    }
}

pub trait LLMApi {
    fn build_request(&self, messages: Vec<Message>) -> Request<Vec<u8>>; 
    fn build_response(&self, data: &[u8]) -> (usize, LLMResponse);
}

impl LLMApi for LLMContext {
   fn build_request(&self, messages: Vec<Message>) -> Request<Vec<u8>> {
        self.api.build_request(messages)
    } 
    fn build_response(&self, data: &[u8]) -> (usize, LLMResponse) {
        self.api.build_response(data)
    }
}
