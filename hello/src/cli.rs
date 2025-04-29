use std::str::FromStr;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::fs::File;
use std::io::{Read, Write};

#[derive(Debug)]
pub enum Error {
    ArgParse,
    ReadConfigAction(String),
    FileRW,
    ConfigParse,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum Provider {
    OpenAi
}

pub enum Verb {
    Get,
    Set
}

pub enum What {
    Key,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    keys: HashMap<Provider, String>
}

impl FromStr for Verb {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Verb, Self::Err> {
        match s {
            "get" => Ok(Verb::Get),
            "set" => Ok(Verb::Set),
            _ => Err(Error::ArgParse),
        }
    }
}

impl FromStr for What {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<What, Self::Err> {
        match s {
            "key" => Ok(What::Key),
            _ => Err(Error::ArgParse),
        }
    }
}

impl FromStr for Provider {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Provider, Self::Err> {
        match s {
            "openai" => Ok(Provider::OpenAi),
            _ => Err(Error::ArgParse),
        }
    }
}

pub fn print_usage(with_desc: bool) {
    if with_desc {
        println!("Interact with an LLM.\n");
    }

    println!("Usage:
    hello --configure <verb> <what> <who> [value]
    hello <yap>... [options]

Options:
    --configure Execute the command in configuration mode. If this flag is present, expects verb, what, who arguments. Must be the very first command argument.

Arguments:
    <verb>  An action to take on the <what>. One of: get, set
    <what>  The subject of the action. One of: key
    <who>   A specifier for which <what> to act on. One of: openai
    <yap>   Some words that make up a prompt. Beware that some shell programs interpret some characters so you may need to escape them. Alternativly you can enclose all of your prompt in double quotes to avoid this issue altogether.

$> hello what is the radius of the earth ?
The radius of Earth is approximately 6,371 kilometers (3,959 miles). 
This is the average radius, as Earth is not a perfect sphere but rather an oblate spheroid, slightly flattened at the poles and bulging at the equator.
The equatorial radius is about 6,378 kilometers (3,963 miles), while the polar radius is about 6,357 kilometers (3,950 miles).
");
}

pub fn get_config_action(args: &[String]) -> Result<(Verb, What, Provider)> {
    // could use a little bit more safeguards
    assert!(args.len() == 3);
    let verb = match Verb::from_str(args[0].as_str()) {
        Ok(v) => v,
        Err(_) => {
            return Err(Error::ReadConfigAction(String::from("Error: unrecognized verb argument")));
        }
    };
    let what = match What::from_str(args[1].as_str()) {
        Ok(w) => w,
        Err(_) => {
            return Err(Error::ReadConfigAction(String::from("Error: unrecognized what argument")));
        }
    };
    let who = match Provider::from_str(args[2].as_str()) {
        Ok(w) => w,
        Err(_) => {
            return Err(Error::ReadConfigAction(String::from("Error: unrecognized who argument")));
        }
    };

    Ok((verb, what, who))
}

impl Config {
    pub fn open<P: AsRef<Path>>(p: P) -> Result<Self> {
        let mut f = match File::open(p.as_ref()) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open config file: {e:?}");
                return Err(Error::FileRW)
            }
        }; 

        let mut contents = String::new();
        let _ = f.read_to_string(&mut contents);

        if contents.len() == 0 {
            Ok(Self {
                keys: HashMap::new(),
            })
        } else {
            match serde_json::from_str::<Config>(contents.as_str()) {
                Ok(c) => Ok(c),
                Err(c) => {
                    eprintln!("Failed to parse config file: {c:?}");
                    Err(Error::ConfigParse)
                }
            }
        }

    }

    pub fn insert_key(&mut self, provider: Provider, key: String) {
        let _ = self.keys.insert(provider, key);
    }

    pub fn get_key(&self, provider: Provider) -> Option<String> {
        self.keys.get(&provider).map(|s| s.to_owned())
    }

    pub fn save<P: AsRef<Path>>(&self, p: P) -> Result<()> {
        let json = match serde_json::to_string(self) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("Failed to save config File: {e:?}");
                return Err(Error::ConfigParse);
            }
        };

        match File::create(p.as_ref()) {
            Err(e) => {
                eprintln!("{e:?}");
                Err(Error::FileRW)
            }
            Ok(mut f) => {
                let _ = f.write_all(json.as_bytes());
                Ok(())
            }
        } 
    }
}
