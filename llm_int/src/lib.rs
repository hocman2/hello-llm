pub mod openai;

pub trait ApiResponseTransmit {
    fn transmit_response(data: &[u8]) -> (usize, String);
}
