mod predefined_prompts;

use std::sync::mpsc::{Sender, Receiver};
use std::time::Duration;
use curl::easy::{Easy, List};
use curl::multi::{Multi, EasyHandle};
use serde::Serialize;
use llm_int::ApiResponseTransmit;
use llm_int::openai::chat_completion_api::{LlmRequest, LlmResponse, LlmMessageTx, LlmModels, Role};
use crate::the_key::KEY;
use crate::context::Context;
use crate::term::TermTaskMessage;
use predefined_prompts::SYSPROMPT;

pub enum RequestTaskMessage {
    ReceivedPiece(String),
    Done,
}

#[derive(PartialEq, Eq)]
enum PollingMode {
    AwaitRequestUpdate,
    AwaitPrompt,
}

pub struct RequestTask {
    multi: Multi, 
    easy_handle: Option<EasyHandle>,
    ctx: Context,
    polling_mode: PollingMode,
}

impl RequestTask {
    pub fn new(ctx: Context) -> Self {
        Self {
            multi: Multi::new(),
            easy_handle: None,
            ctx,
            polling_mode: PollingMode::AwaitPrompt,
        }
    }

    fn stop_ongoing(&mut self) {
        if let Some(easy_handle) = self.easy_handle.take() {
            let _ = self.multi.remove(easy_handle);
        }
    }

    fn build_easy_handle<Res: ApiResponseTransmit, Req: Serialize>(&self, request: Req, tx_ans: Sender<RequestTaskMessage>) -> Easy {
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
            let (sz, content) =  Res::transmit_response(data); 
            let _ = tx_ans.send(RequestTaskMessage::ReceivedPiece(content));
            Ok(sz)
        }).unwrap();

        easy
    }

    pub fn run(mut self, tx_ans: Sender<RequestTaskMessage>, rx_tty: Receiver<TermTaskMessage>) {

        let sysprompt_full: String = match self.ctx.get_piped_input() {
            Some(piped) => {
                let mut s = String::from(SYSPROMPT);
                s.push_str(piped.as_str());
                s
            },
            None => String::from(SYSPROMPT)
        };

        let mut history: Vec<(Role, String)> = vec![
            (Role::Developer, sysprompt_full),
            (Role::User, self.ctx.get_initial_prompt())
        ];

        let request = LlmRequest {
            model: LlmModels::GPT_4_1_Mini,
            messages: LlmMessageTx::from_history(&history),
            max_completion_tokens: 4096,
            n: 1,
            stream: true,
        };
        let easy = self.build_easy_handle::<LlmResponse, LlmRequest>(request, tx_ans.clone());
        self.easy_handle = self.multi.add(easy).map_or(None, |h| Some(h));
        self.polling_mode = PollingMode::AwaitRequestUpdate;

        let mut run_task = true;
        let mut next_polling: Option<PollingMode> = None;
        while run_task {
            if let Some(next_polling) = next_polling.take() {
                self.polling_mode = next_polling;
            }

            let tty_msg: Option<TermTaskMessage> = match self.polling_mode {
                PollingMode::AwaitRequestUpdate => rx_tty.try_recv().map_or(None, |m| Some(m)),
                PollingMode::AwaitPrompt => match rx_tty.recv() {
                    Ok(msg) => Some(msg),
                    Err(_) => { self.stop_ongoing(); run_task = false; None } //tty task is closed
                },
            };

            match tty_msg {
                Some(tty_msg) => match tty_msg {
                    TermTaskMessage::ReceivedUserPrompt {user_prompt, llm_answer_prev} => {
                        self.stop_ongoing();

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
                        next_polling = Some(PollingMode::AwaitRequestUpdate);
                    },
                    TermTaskMessage::Die => {
                        self.stop_ongoing();
                        run_task = false;
                    }
                },
                None => ()
            }

            if self.polling_mode == PollingMode::AwaitRequestUpdate {
                let _ = self.multi.wait(&mut [], Duration::from_millis(30));
                if let Ok(running_handles) = self.multi.perform() {
                    if running_handles == 0 && self.easy_handle.is_some() { 
                        self.stop_ongoing();
                        let _ = tx_ans.send(RequestTaskMessage::Done);
                        next_polling = Some(PollingMode::AwaitPrompt);
                    }
                }
            }
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
