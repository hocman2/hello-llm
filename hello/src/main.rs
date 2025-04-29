mod cli;
mod term;
mod request;
mod context;

use std::env;
use std::path::PathBuf;
use std::fs;
use std::thread;
use std::sync::mpsc::channel;
use std::process::exit;
use std::io::{stdin, Read};
use std::os::fd::AsRawFd;
use term::TermTask;
use request::RequestTask;
use context::Context;
use directories::ProjectDirs;

const CONFIG_FILE_NAME: &'static str = ".config.json";

extern "C" {
    fn isatty(fd: i32) -> i32;
}

fn is_tty<T: AsRawFd>(fd: &T) -> bool {
    unsafe {isatty(fd.as_raw_fd()) != 0}
}

fn open_config() -> (PathBuf, cli::Config) {
    let data_dir: PathBuf = match ProjectDirs::from("", "", "hello-llm") {
        Some(project_dir) => {
            project_dir.data_dir().to_owned()
        },
        None => { panic!("Unable to find an appropriate directory for config file"); }
    };
    fs::create_dir_all(&data_dir)
        .expect(format!("Unable to create data directory at: {}", data_dir.to_string_lossy()).as_str());
    let mut config_file_path = data_dir.clone();
    config_file_path.push(CONFIG_FILE_NAME);
    if !config_file_path.exists() {
        fs::write(&config_file_path, "")
            .expect(format!("Unable to create file {}", config_file_path.to_string_lossy()).as_str());
    }

    let cfg = match cli::Config::open(&config_file_path) {
        Ok(c) => c,
        Err(_) => { exit(-1); }
    };

    (config_file_path, cfg)
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

    let (config_file_path, mut config) = open_config();

    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();

    if argc == 1 {
        cli::print_usage(true);
        exit(0);
    }

    if argv[1] == "--configure" {
        if argc < 5 {
            eprintln!("Error: configure mode expects at least three extra arguments");
            cli::print_usage(false);
            exit(1);
        }

        match cli::get_config_action(&argv[2..5]) {
            Err(cli::Error::ReadConfigAction(e)) => {
                eprintln!("{}", e);
                cli::print_usage(false);
                exit(1);
            },
            Err(_) => { unimplemented!(); }
            Ok((verb, what, who)) => {
                match verb {
                    cli::Verb::Set => {
                        match what {
                            cli::What::Key => {
                                if argc != 6 {
                                    eprintln!("Error: missing a key value");
                                    cli::print_usage(false);
                                    exit(1);
                                }
                                config.insert_key(who, argv[5].clone());
                            }
                        }
                    },
                    cli::Verb::Get => {

                    }
                };
            }
        };
        let _ = config.save(&config_file_path);
    } else {
        let prompt: String = argv.iter()
            .skip(1)
            .fold(String::from("Hello,"), |mut acc, arg| { acc.push_str(" "); acc.push_str(arg); acc });

        let ctx = Context::new(prompt, piped, config);

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
}
