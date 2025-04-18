pub mod openai;

use std::sync::mpsc::Sender;

pub trait ApiResponseTransmit {
    fn transmit_response(data: &[u8], tx_ans: Sender<String>) -> Option<usize>;
}
