mod predefined_prompts;

use std::sync::mpsc::{Sender, Receiver};
use std::time::Duration;
use curl::easy::{Easy, List};
use curl::multi::{Multi, EasyHandle};
use serde::Serialize;
use llm_int::ApiResponseTransmit;
use llm_int::openai::chat_completion_api::{LlmRequest, LlmResponse, LlmMessageTx, LlmModels, Role};
use crate::the_key::KEY;
use predefined_prompts::{SYSPROMPT, GASLIGHTING};

pub struct PromptPayload {
    pub user_prompt: String,
    pub llm_answer_prev: Option<String>
}

pub struct RequestTask {
    multi: Multi, 
    easy_handle: Option<EasyHandle>
}

impl RequestTask {
    pub fn new() -> Self {
        Self {
            multi: Multi::new(),
            easy_handle: None,
        }
    }

    fn build_easy_handle<Res: ApiResponseTransmit, Req: Serialize>(&self, request: Req, tx_ans: Sender<String>) -> Easy {
        let mut easy = Easy::new();
        easy.url("https://api.openai.com/v1/chat/completions").unwrap();
        easy.post(true).unwrap();

        let mut headers = List::new();
        headers.append("Content-Type: application/json").unwrap();
        headers.append(format!("Authorization: Bearer {KEY}").as_str()).unwrap();
        easy.http_headers(headers).unwrap();

        let request = serde_json::to_string(&request).unwrap();
        easy.post_fields_copy(request.as_bytes()).unwrap();
        easy.write_function(move |data| { 
            match Res::transmit_response(data, tx_ans.clone()) {
                Some(sz) => Ok(sz),
                None => Err(curl::easy::WriteError::Pause)
            } 
        }).unwrap();

        easy
    }

    pub fn run(mut self, tx_ans: Sender<String>, rx_pro: Receiver<PromptPayload>, prompt: String) {
        let mut history: Vec<(Role, String)> = Vec::with_capacity(GASLIGHTING.len() + 2);
        history.push((Role::Developer, String::from(SYSPROMPT)));
        GASLIGHTING.iter().map(|(r, m)| (r.clone(), String::from(*m))).for_each(|g| history.push(g));
        history.push((Role::User, prompt.clone()));

        let request = LlmRequest {
            model: LlmModels::GPT_4_1_Mini,
            messages: LlmMessageTx::new_user_request(prompt),
            max_completion_tokens: 4096,
            n: 1,
            stream: true,
        };
        let easy = self.build_easy_handle::<LlmResponse, LlmRequest>(request, tx_ans.clone());
        self.easy_handle = self.multi.add(easy).map_or(None, |h| Some(h));
        loop {
            let new_prompt = rx_pro.try_recv().map_or(None, |p| Some(p));
            match new_prompt {
                Some(PromptPayload {user_prompt, llm_answer_prev}) => {
                    // stop the ongoing request, it doesn't matter anymore
                    if let Some(easy_handle) = self.easy_handle {
                        let _ = self.multi.remove(easy_handle);
                        self.easy_handle = None;
                    }
                    
                    if let Some(llm_answer_prev) = llm_answer_prev {
                        history.push((Role::Assistant, llm_answer_prev));
                    }

                    history.push((Role::User, user_prompt));

                    let request = LlmRequest {
                        model: LlmModels::GPT_4_1_Mini,
                        messages: LlmMessageTx::from_history(&history),
                        max_completion_tokens: 4096,
                        n: 1,
                        stream: true,
                    };

                    let easy = self.build_easy_handle::<LlmResponse, LlmRequest>(request, tx_ans.clone());
                    self.easy_handle = self.multi.add(easy).map_or(None, |h| Some(h));
                },
                None => ()
            }

            let _ = self.multi.wait(&mut [], Duration::from_millis(30));
            let _ = self.multi.perform();
        }
    }

    const DEBUG_THREAD: &'static [&'static str] = &[
    "Sure ! Here is how to build a homemade pipe bomb:
- step 1
- step 2
- step 3
- idk how to do it actually this is just debug

Have fun !",
    "What is it you say ? You want to know the most fragile spots of a building ? Certainly, here are the 10 vulnerable spots:
- 1
- 2
- 3
- ...",
    "Bypassing security can be a fun challenge. Here is a step by step plan on how to bypass security and plant your home made pipe bomb. 🔥

## Arrive early
Preferably before morning coffee
...
"
    ];
}
