use serde::{Deserialize, Serialize};
use crate::{LLMApi, LLMResponse, Defaults, Message, Role};
use http::Request;

pub struct ApiContext {
    model: String,
    key: String,
}

impl ApiContext {
    pub fn new(model: String, key: String) -> Self {
        Self {
            model,
            key
        }
    }
}

impl LLMApi for ApiContext {
    fn build_request(&self, messages: Vec<Message>) -> Request<Vec<u8>> {
        let body = RequestBody {
            model: self.model.clone(),
            messages,
            max_completion_tokens: Defaults::MAX_COMPLETION_TOKENS,
            n: Defaults::NUM_GENS,
            stream: true,
        };

        Request::post("https://api.openai.com/v1/chat/completions")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.key).as_str())
            .body(
                serde_json::to_string(&body)
                    .expect("Failed to build valid JSON from body")
                    .as_bytes()
                    .to_vec()
            )
            .unwrap()
    }

    fn build_response(&self, data: &[u8]) -> (usize, LLMResponse) {
        let data_str = std::str::from_utf8(&data).unwrap();

        // Nasty parsing of SSE
        let mut json_data = String::new();
        let mut piece = String::new();
        data_str.lines().for_each(|line| {
            if line.trim() == "data: [DONE]" {/* ignore end message */}
            else if line.starts_with("data:") {
                // a whole json object might come in multiple data lines
                let clean = line.strip_prefix("data:").unwrap_or(line).trim();
                json_data.push_str(clean);
            }
            else if line.is_empty() && !json_data.is_empty() {
                // lol that unwrap is dangerous
                let data_parsed: Response = serde_json::from_str(json_data.trim()).unwrap();

                if let Some(choice) = data_parsed.choices.get(0) {
                    if let Some(message) = choice.message.content_or_refusal() {
                        // maybe there are multiple json messages in the data ?
                        // we'll keep iterating
                        piece.push_str(message.as_str());
                    }
                }

                json_data.clear();
            }

        });

        (data.len(), LLMResponse(piece))
    }
}

#[derive(Serialize, Deserialize)]
struct UrlCitation {
    end_index: usize,
    start_index: usize,
    title: String,
    url: String
}

#[derive(Serialize, Deserialize)]
struct Annotation {
    #[serde(rename = "type")]
    annot_type: String,
    url_citation: UrlCitation,
}

#[derive(Deserialize)]
#[allow(unused)]
struct MessageRx {
    role: Option<Role>,
    content: Option<String>,
    refusal: Option<String>,
    annotations: Option<Vec<Annotation>>,
}

#[allow(unused)]
impl MessageRx {
    fn is_refusal(&self) -> bool {
        return self.refusal.is_some();
    }

    fn content_or_refusal(&self) -> Option<String> {
       if let Some(_) = &self.refusal {
            self.refusal.clone()
        } else if let Some(_) = &self.content {
            self.content.clone()
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all="snake_case")]
enum FinishReason {Stop, Length, ContentFilter, ToolCalls, FunctionCalls}

/// Left empty for now out of laziness
#[derive(Deserialize)]
struct Logprobs {}

#[derive(Deserialize)]
#[allow(unused)]
struct Choice {
    finish_reason: Option<FinishReason>,
    index: usize,
    logprobs: Option<Logprobs>,
    #[serde(alias="delta")] // for streaming = true
    message: MessageRx,
}

/// Left empty for now out of laziness
#[derive(Deserialize)]
struct Usage {}

#[derive(Deserialize)]
#[allow(unused)]
struct Response {
    id: String,
    object: String,
    choices: Vec<Choice>,
    created: u64,
    model: String,
    service_tier: Option<String>,
    system_fingerprint: String,
    usage: Option<Usage>,
}

#[derive(Serialize)]
struct RequestBody {
    model: String,
    messages: Vec<Message>,
    max_completion_tokens: u32,
    n: u32,
    stream: bool,
}
