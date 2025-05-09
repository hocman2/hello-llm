/// Functions for terminal rendering
use std::io::{stdout, Stdout, Write};
use std::time::Duration;
use crossterm::{queue, execute, cursor, style, event, terminal};
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
use std::sync::mpsc::{Receiver, Sender};
use crate::request::RequestTaskMessage;
use crate::context::Context;

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

enum PollingMode {
    AwaitUserin,
    AwaitRequestUpdate,
}

pub enum TermTaskMessage {
    ReceivedUserPrompt {
        user_prompt: String,
        llm_answer_prev: Option<String>
    },
    Die,
}

pub struct TermTask {
    userin_buf: String,
    llmout_buf: String,
    stdout: Stdout,
    ctx: Context,
    polling_mode: PollingMode,
}

impl TermTask {
    const USERIN_PREFIX: &'static str = ">";

    pub fn new(ctx: Context) -> Self {
        Self {
            userin_buf: String::new(),
            llmout_buf: String::new(),
            stdout: stdout(),
            ctx,
            polling_mode: PollingMode::AwaitRequestUpdate,
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
        );

        let _ = execute!(self.stdout, style::Print(format!("{} {}", Self::USERIN_PREFIX, self.userin_buf)));
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

    fn prepare_userin_ui(&mut self) -> std::io::Result<()> {
        print!("{} ", Self::USERIN_PREFIX);
        self.stdout.flush()?;
        Ok(())
    }

    pub fn run(mut self, tx_tty: Sender<TermTaskMessage>, rx_ans: Receiver<RequestTaskMessage>) -> std::io::Result<()> {
        // make some room
        println!("");
        self.prepare_userin_ui()?;
        terminal::enable_raw_mode()?;

        let mut last_wrap_idx: usize = 0;
        let run_task = true;
        let mut next_polling: Option<PollingMode> = None;
        while run_task {
            if let Some(next_polling) = next_polling.take() {
                self.polling_mode = next_polling;
            }

            let tsize = terminal::size()?;
            let message: Option<RequestTaskMessage> = match self.polling_mode {
                PollingMode::AwaitRequestUpdate => rx_ans.recv_timeout(Duration::from_millis(30)).map_or(None, |o| Some(o)),
                // try_recv out of safety but we could just return None
                PollingMode::AwaitUserin => rx_ans.try_recv().map_or(None, |o| Some(o)),
            }; 

            if let Some(message) = message {
                match message {
                    RequestTaskMessage::Done => {
                        next_polling = Some(PollingMode::AwaitUserin);
                    },
                    RequestTaskMessage::ReceivedPiece(piece) => {
                        let total_userin = Self::USERIN_PREFIX.width() + self.userin_buf.as_str().width() + 1;
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
                }
            }

            let event = match self.polling_mode {
                PollingMode::AwaitUserin => event::read()?,
                PollingMode::AwaitRequestUpdate => {
                    if !event::poll(Duration::ZERO)? { continue; /* wish i had a goto ... */}
                    event::read()?
                }
            };

            // leave with Enter OR CTRL-c
            if let event::Event::Key(ref event) = event {
                if event.code == event::KeyCode::Enter && self.userin_buf.is_empty() {
                    let _ = tx_tty.send(TermTaskMessage::Die);
                    break;
                }
                if event.code == event::KeyCode::Char('c') && event.modifiers == event::KeyModifiers::CONTROL {
                    let _ = tx_tty.send(TermTaskMessage::Die);
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
                        
                        let total_userin = Self::USERIN_PREFIX.width() + self.userin_buf.as_str().width() + 1;
                        let numrows = ((total_userin / (tsize.0) as usize) + 1) as u16;
                        if numrows == 1 {
                            self.refresh_userin();
                        } else {
                            let mut line_start_byte = 0;
                            let mut curr_width = Self::USERIN_PREFIX.width() + 1;
                            for (i, c) in self.userin_buf.char_indices() {
                                let w = c.width().unwrap_or(0);
                                if curr_width + w > (tsize.0) as usize {
                                    curr_width = w;
                                    line_start_byte = i; 
                                } else {
                                    curr_width += w;
                                }
                            }

                            if curr_width == tsize.0 as usize {
                                let _ = execute!(
                                    self.stdout,
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                    cursor::MoveToPreviousLine(1),
                                    cursor::MoveToColumn(tsize.0)
                                );
                            } else {
                                let _ = execute!(
                                    self.stdout, 
                                    cursor::MoveToColumn(0),
                                    terminal::Clear(terminal::ClearType::CurrentLine),
                                    style::Print(&self.userin_buf[line_start_byte..])
                                );
                            }
                        }
                    },
                    event::KeyCode::Enter => {
                        let userin_saved = self.userin_buf.clone();
                        let llmout_saved = self.llmout_buf.clone();

                        self.userin_buf.clear();
                        self.llmout_buf.clear();
                        last_wrap_idx = 0;
                        next_polling = Some(PollingMode::AwaitRequestUpdate);

                        self.print_ln(format!("{} {}", Self::USERIN_PREFIX, userin_saved))?;
                        self.refresh_userin();
                        self.move_userin_down();

                        let _ = tx_tty.send(TermTaskMessage::ReceivedUserPrompt {user_prompt: userin_saved, llm_answer_prev: Some(llmout_saved)});
                    }
                    _ => () 
                },
                _ => ()
            }
        }

        terminal::disable_raw_mode()?;
        Ok(())
    }
}
