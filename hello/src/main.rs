mod term;
mod request;
mod the_key;
mod context;

use std::env;
use std::thread;
use std::sync::mpsc::channel;
use std::process::exit;
use std::io::{stdin, Read};
use std::os::fd::AsRawFd;
use term::TermTask;
use request::RequestTask;
use context::Context;

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

extern "C" {
    fn isatty(fd: i32) -> i32;
}

fn is_tty<T: AsRawFd>(fd: &T) -> bool {
    unsafe {isatty(fd.as_raw_fd()) != 0}
}

fn main() {
    let mut stdin = stdin();
    let piped = if !is_tty(&stdin) {
        let mut buffer = String::new();
        stdin.read_to_string(&mut buffer).unwrap();
        Some(buffer)
    } else {
        None
    };

    let argc = env::args().count() - 1;
    
    if argc == 0 {
        print_usage();
        exit(0);
    }

    let prompt: String = env::args()
        .skip(1)
        .fold(String::from("Hello,"), |mut acc, arg| { acc.push_str(format!(" {}", arg).as_str()); acc });

    let ctx = Context::new(prompt, piped);

    let (tx_ans, rx_ans) = channel();
    let (tx_tty, rx_tty) = channel();
    let req_thr_handle = thread::spawn({
        let ctx = ctx.clone();
        move || {
            RequestTask::new(ctx).run(tx_ans, rx_tty);
        }
    });

    match TermTask::new(ctx.clone()).run(tx_tty, rx_ans) {
        Err(e) => { println!("{e:?}"); },
        Ok(()) => ()
    };

    let _ = req_thr_handle.join();
}
