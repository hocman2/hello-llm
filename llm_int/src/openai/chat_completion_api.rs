use serde::{Deserialize, Serialize};
use crate::ApiResponseTransmit;
use std::sync::mpsc::Sender;

pub struct LlmModels {}
#[allow(non_upper_case_globals)]
impl LlmModels {
    pub const GPT_4_1_Mini: &'static str = "gpt-4.1-mini-2025-04-14";
    pub const GPT_O_4_Mini: &'static str = "o4-mini-2025-04-16";
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all="snake_case")]
pub enum Role {Assistant, User, Developer}

#[derive(Serialize, Deserialize)]
pub struct UrlCitation {
    end_index: usize,
    start_index: usize,
    title: String,
    url: String
}

#[derive(Serialize, Deserialize)]
pub struct Annotation {
    #[serde(rename = "type")]
    annot_type: String,
    url_citation: UrlCitation,
}

#[derive(Deserialize)]
pub struct LlmMessageRx {
    pub role: Option<Role>,
    pub content: Option<String>,
    pub refusal: Option<String>,
    pub annotations: Option<Vec<Annotation>>,
}

impl LlmMessageRx {
    pub fn is_refusal(&self) -> bool {
        return self.refusal.is_some();
    }

    pub fn content_or_refusal(&self) -> Option<String> {
       if let Some(_) = &self.refusal {
            self.refusal.clone() 
        } else if let Some(_) = &self.content {
            self.content.clone()
        } else {
            None
        }
    }
}

#[derive(Serialize)]
pub struct LlmMessageTx {
    pub role: Role,
    pub content: String,
}

impl LlmMessageTx {
    // Returns a vec of a single message intended for
    // an initial request. 
    pub fn new_user_request(content: String) -> Vec<Self> {
        vec![LlmMessageTx {
            role: Role::User,
            content,
        }]
    } 

    pub fn from_history(content: &Vec<(Role, String)>) -> Vec<Self> {
        content.iter()
            .map(|(r, s)| {
                LlmMessageTx {
                    role: r.clone(),
                    content: s.clone(),
                }
            })
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(rename_all="snake_case")]
pub enum FinishReason {Stop, Length, ContentFilter, ToolCalls, FunctionCalls}

/// Left empty for now out of laziness
#[derive(Deserialize)]
pub struct Logprobs {}

#[derive(Deserialize)]
pub struct Choice {
    pub finish_reason: Option<FinishReason>,
    pub index: usize,
    pub logprobs: Option<Logprobs>,
    #[serde(alias="delta")] // for streaming = true
    pub message: LlmMessageRx,
}

/// Left empty for now out of laziness
#[derive(Deserialize)]
pub struct Usage {}

#[derive(Deserialize)]
pub struct LlmResponse {
    pub id: String,
    pub object: String,
    pub choices: Vec<Choice>,
    pub created: u64,
    pub model: String,
    pub service_tier: Option<String>,
    pub system_fingerprint: String,
    pub usage: Option<Usage>,
}

impl ApiResponseTransmit for LlmResponse {
    fn transmit_response(data: &[u8], tx_ans: Sender<String>) -> Option<usize> {
        let data_str = std::str::from_utf8(&data).unwrap();
        let mut event_data = String::new();
        data_str.lines().for_each(|line| {
            if line.trim() == "data: [DONE]" {/* ignore end message */}
            else if line.starts_with("data:") {
                // a whole json object might come in multiple data lines
                let clean = line.strip_prefix("data:").unwrap_or(line).trim();
                event_data.push_str(clean);
            } 
            else if line.is_empty() && !event_data.is_empty() {
                let json = event_data.trim();
                let data_parsed: LlmResponse = serde_json::from_str(json).unwrap();

                if let Some(choice) = data_parsed.choices.get(0) {
                    if let Some(message) = choice.message.content_or_refusal() {
                        let _ = tx_ans.send(message);
                    }
                }

                event_data.clear();
            }

        });

        Some(data.len())
    }
}
#[derive(Serialize)]
pub struct LlmRequest {
    pub model: &'static str,
    pub messages: Vec<LlmMessageTx>,
    pub max_completion_tokens: u32,
    pub n: u32,
    pub stream: bool,
}

impl Default for LlmRequest {
    fn default() -> Self {
        Self {
            model: LlmModels::GPT_4_1_Mini,
            messages: Vec::new(),
            max_completion_tokens: 4096,
            n: 1,
            stream: false,
        }
    }
}
