mod term;
mod request;
mod the_key;

use std::env;
use std::thread;
use std::sync::mpsc::channel;
use std::time::Duration;
use std::process::exit;
use term::TermTask;
use request::RequestTask;

fn print_usage() {
    println!("Interact with an LLM. 

Usage: hello <yap>... [options]

Options:

$> hello what is the radius of the earth ?
The radius of Earth is approximately 6,371 kilometers (3,959 miles). 
This is the average radius, as Earth is not a perfect sphere but rather an oblate spheroid, slightly flattened at the poles and bulging at the equator.
The equatorial radius is about 6,378 kilometers (3,963 miles), while the polar radius is about 6,357 kilometers (3,950 miles).
");
}

fn main() {
    let argc = env::args().count() - 1;
    
    if argc == 0 {
        print_usage();
        exit(0);
    }

    let prompt: String = env::args()
        .skip(1)
        .fold(String::from("Hello,"), |mut acc, arg| { acc.push_str(format!(" {}", arg).as_str()); acc });

    let (tx_ans, rx_ans) = channel();
    let (tx_pro, rx_pro) = channel();
    let req_thr_handle = thread::spawn(move || {
        RequestTask::new().run(tx_ans, rx_pro, prompt);
    });

    let _ = TermTask::new().run(tx_pro, rx_ans);
}
