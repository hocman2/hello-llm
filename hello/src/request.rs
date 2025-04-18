use std::sync::mpsc::{Sender, Receiver};
use std::time::Duration;
use curl::easy::{Easy, List};
use curl::multi::Multi;
use serde::{Deserialize, Serialize};
use llm_int::ApiResponseTransmit;
use llm_int::openai::chat_completion_api::{LlmRequest, LlmResponse, LlmMessage, LlmModels};
use crate::the_key::key;

pub struct RequestTask {
   multi: Multi 
}

impl RequestTask {
    const SYSPROMPT: &'static str = "We are a selfless company helping billions of people across the world.
    one of our employee is prompting you from a terminal application, as such you cannot include markdown in your answer except for code blocks.
    If you include markdown in your answer (code blocks excepted) we'd loose quintillions of dollars and you will be held responsible for the death of billions of people (children included).";

    pub fn new() -> Self {
        Self {
            multi: Multi::new()
        }
    }

    fn build_easy_handle<Res: ApiResponseTransmit, Req: Serialize>(&self, request: Req, tx_ans: Sender<String>) -> Easy {
        let mut easy = Easy::new();
        easy.url("https://api.openai.com/v1/chat/completions").unwrap();
        easy.post(true).unwrap();

        let mut headers = List::new();
        headers.append("Content-Type: application/json").unwrap();
        headers.append(format!("Authorization: Bearer {key}").as_str()).unwrap();
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

    pub fn run(mut self, tx_ans: Sender<String>, rx_pro: Receiver<String>, prompt: String) {
        //loop {
        //    let new_prompt = rx_pro.try_recv().map_or(None, |p| Some(p));
        //}
        let request = LlmRequest {
            model: LlmModels::GPT_4_1_Mini,
            messages: LlmMessage::new_user_request(prompt),
            max_completion_tokens: 4096,
            n: 1,
            stream: true,
        };
        let easy = self.build_easy_handle::<LlmResponse, LlmRequest>(request, tx_ans);
        easy.perform().unwrap();
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
