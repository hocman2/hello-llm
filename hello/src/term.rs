/// Functions for terminal rendering
use std::io::{stdout, Stdout, Write};
use std::time::Duration;
use crossterm::{queue, execute, cursor, style, event, terminal};
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
use std::sync::mpsc::{Receiver, Sender};
use crate::request::PromptPayload;

// helper founction
fn last_line_width(buf: &str, last_wrap_idx: usize) -> usize {
    if buf.len() == 0 || buf.ends_with('\n') {
        return 0;
    }

    let split_buf = buf.split_at(last_wrap_idx).1;
    if let Some(idx) = split_buf.rfind('\n') {
        return split_buf.split_at(idx).1.trim_matches('\n').width();    
    } else {
        return split_buf.width();
    }
}

pub struct TermTask {
    userin_buf: String,
    llmout_buf: String,
    stdout: Stdout,
}

impl TermTask {
    const USERIN_PREFIX: &'static str = ">";

    pub fn new() -> Self {
        Self {
            userin_buf: String::new(),
            llmout_buf: String::new(),
            stdout: stdout(),
        }
    }

    fn refresh_userin(&mut self) {
        let _ = execute!(
            self.stdout, 
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToColumn(0),
            style::Print(format!("{} {}", Self::USERIN_PREFIX, self.userin_buf.as_str()))
        );
    }

    fn move_userin_down(&mut self) {
        let _ = execute!(self.stdout,
            style::Print("\n"), // < forces a one line scroll
            cursor::MoveToPreviousLine(1),
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToNextLine(1),
            style::Print(format!("{} {}", Self::USERIN_PREFIX, self.userin_buf)),
        );
    }

    fn print_ln(&mut self, s: String) -> std::io::Result<()> {
        let tsize = terminal::size()?;
        let total_userin = Self::USERIN_PREFIX.len() + self.userin_buf.as_str().width() + 1;
        let inpnrow = (total_userin / (tsize.0 as usize) + 1) as u16;
        self.move_userin_down(); 
        let _ = execute!(self.stdout,
            cursor::SavePosition,
            cursor::MoveToPreviousLine(inpnrow),
            style::Print(format!("{s}\n")),
            cursor::RestorePosition,
        );
        Ok(())
    }

    pub fn run(mut self, tx_pro: Sender<PromptPayload>, rx_ans: Receiver<String>) -> std::io::Result<()> {
        // make room for output + input line
        println!("");

        let _ = terminal::enable_raw_mode();

        let mut last_wrap_idx: usize = 0;

        print!("{} ", Self::USERIN_PREFIX);
        self.stdout.flush()?;

        loop {
            let tsize = terminal::size()?;
            let piece: Option<String> = rx_ans.recv_timeout(Duration::from_millis(30)).map_or(None, |o| Some(o));
            if let Some(piece) = piece {
                let total_userin = Self::USERIN_PREFIX.len() + self.userin_buf.as_str().width() + 1;
                let inpnrow = (total_userin / (tsize.0 as usize) + 1) as u16;
                piece.chars().for_each(|c| {

                    let mut curscol = last_line_width(self.llmout_buf.as_str(), last_wrap_idx) as u16;
                    self.llmout_buf.push(c);
                    match c {
                        '\n' => {
                            self.move_userin_down();
                        },
                        _ => {
                            let will_wrap = (curscol as usize) + c.width().unwrap_or(0) > tsize.0 as usize;
                            if will_wrap {
                                last_wrap_idx = self.llmout_buf
                                    .char_indices()
                                    .last()
                                    .unwrap_or((0,'\0')).0;

                                self.move_userin_down();
                                curscol = 0;
                            }
                            let _ = queue!(self.stdout,
                                cursor::SavePosition,
                                cursor::MoveToPreviousLine(inpnrow),
                                cursor::MoveToColumn(curscol),
                                style::Print(c),
                                cursor::RestorePosition,
                            );
                        }
                    }
                });

                let _ = self.stdout.flush();
            }

            if !event::poll(Duration::ZERO)? { continue; }
            let event = event::read()?;

            // leave with Enter OR CTRL-c
            if let event::Event::Key(ref event) = event {
                if event.code == event::KeyCode::Enter && self.userin_buf.is_empty() {
                    break;
                }
                if event.code == event::KeyCode::Char('c') && event.modifiers == event::KeyModifiers::CONTROL {
                    break;
                }
            }

            match event {
                event::Event::Key(event) => match event.code {
                    event::KeyCode::Char(c) => {
                        self.userin_buf.push(c);
                        let _ = execute!(self.stdout, style::Print(c));
                    }
                    event::KeyCode::Backspace => {
                        self.userin_buf.pop();
                        self.refresh_userin();
                    },
                    event::KeyCode::Enter => {
                        let userin_saved = self.userin_buf.clone();
                        let llmout_saved = self.llmout_buf.clone();
                        self.userin_buf.clear();
                        self.llmout_buf.clear();
                        last_wrap_idx = 0;
                        self.print_ln(format!("{} {}", Self::USERIN_PREFIX, userin_saved))?;
                        self.refresh_userin();
                        self.move_userin_down();
                        let _ = tx_pro.send(PromptPayload {user_prompt: userin_saved, llm_answer_prev: Some(llmout_saved)});
                    }
                    _ => () 
                },
                _ => ()
            }
        }

        let _ = terminal::disable_raw_mode();
        Ok(())
    }
}
