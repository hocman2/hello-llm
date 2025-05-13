mod output_metadata_gen;

use std::io::{stdout, Stdout, Write};
use std::time::Duration;
use crossterm::{queue, execute, cursor, style, event, terminal};
use crossterm::event::{PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags, KeyboardEnhancementFlags};
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
use std::sync::mpsc::{Receiver, Sender};
use crate::context::Context;
use crate::request::RequestTaskMessage;
use output_metadata_gen::OutputMetadata;

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

#[derive(Clone)]
struct LinesInfo {
    numlines: u32,
    last_ln_width: usize,
}

struct UserIn {
    buf: String,
    lines_info: LinesInfo,
    stdout: Stdout
}

impl UserIn {
    const PREFIX: &'static str = ">";

    fn new() -> Self {
        Self {
            buf: String::new(),
            lines_info: LinesInfo {
                numlines: 1,
                last_ln_width: 0,
            },
            stdout: stdout(),
        }
    }
    
    fn refresh(&mut self) {
        let _ = execute!(
            self.stdout, 
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToColumn(0),
            style::Print(format!("{} {}", Self::PREFIX, self.buf.as_str()))
        );
    }

    fn move_down(&mut self, n: u16) {
        let _ = execute!(self.stdout,
            style::Print("\n"), // < forces a one line scroll
            cursor::MoveToPreviousLine(n),
            terminal::Clear(terminal::ClearType::CurrentLine),
            cursor::MoveToNextLine(n),
        );

        let _ = execute!(self.stdout, style::Print(format!("{} {}", Self::PREFIX, self.buf)));
    }

    // actively recounts userin lines, use this if userin buffer has been modified, otherwise used
    // cached get_lines_info
    fn count_lines(&mut self, tsize: (u16, u16)) -> LinesInfo {
        let mut last_ln_width = 0;
        let mut numrows = self.buf.lines().fold(0, |mut rows, line| {
            last_ln_width = if rows == 0 {
                const SPACE_WIDTH: usize = 1;
                Self::PREFIX.width() + SPACE_WIDTH + line.width()
            } else {
                    line.width()
                }; 

            rows += 1 + (last_ln_width / (tsize.0 as usize)) as u16;
            last_ln_width %= tsize.0 as usize;
            rows
        });

        // might happen if the buffer is empty, lines would be an empty iterator
        if numrows == 0 { numrows = 1; }

        let lines_info = LinesInfo {
            numlines: numrows.into(),
            last_ln_width,
        };
        self.lines_info = lines_info.clone();
        lines_info
    }

    fn get_lines_info(&self) -> LinesInfo {
        self.lines_info.clone()
    }

    fn prepare_ui(&mut self) -> std::io::Result<()> {
        print!("{} ", Self::PREFIX);
        self.stdout.flush()?;
        Ok(())
    }

    fn remove_last(&mut self, tsize: (u16, u16)) {
        let popped = self.buf.pop().unwrap_or('\0');
        let LinesInfo {numlines, last_ln_width} = self.count_lines(tsize);

        if popped == '\n' {
            let _ = execute!(
                self.stdout,
                terminal::Clear(terminal::ClearType::CurrentLine),
                cursor::MoveToPreviousLine(1),
                cursor::MoveToColumn(last_ln_width as u16)
            );

            return;
        } 

        if numlines <= 1 {
            if let Some('\n') = self.buf.chars().last() {
                self.buf.pop();
                self.count_lines(tsize);
                let _ = execute!(
                    self.stdout,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToPreviousLine(1),
                    cursor::MoveToColumn((self.buf.as_str().width() + Self::PREFIX.width() + 1) as u16)
                );
            } else {
                self.refresh();
            }
        } else {
            let mut line_start_byte = 0;
            let mut pv_char: char = '\0';
            let mut curr_width = UserIn::PREFIX.width() + 1;
            for (i, c) in self.buf.char_indices() {
                let w = c.width().unwrap_or(0);
                if pv_char == '\n' || curr_width + w > (tsize.0) as usize {
                    curr_width = w;
                    line_start_byte = i; 
                } else {
                    curr_width += w;
                }
                pv_char = c;
            }

            if curr_width == tsize.0 as usize || pv_char == '\n' {
                if pv_char == '\n' {
                    self.buf.pop();
                    self.count_lines(tsize);
                }

                let _ = execute!(
                    self.stdout,
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    cursor::MoveToPreviousLine(1),
                    cursor::MoveToColumn(curr_width as u16)
                );
            } else {
                let _ = execute!(
                    self.stdout, 
                    cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    style::Print(&self.buf[line_start_byte..])
                );
            }
        }
    }
}

pub enum TermTaskMessage {
    ReceivedUserPrompt {
        user_prompt: String,
        llm_answer_prev: Option<String>
    },
    Die,
}

pub struct TermTask {
    userin: UserIn,
    llmout_buf: String,
    llmout_numln: u32,
    stdout: Stdout,
    ctx: Context,
    polling_mode: PollingMode,
    metadata: OutputMetadata,
}

impl TermTask {
    pub fn new(ctx: Context) -> Self {
        Self {
            userin: UserIn::new(),
            llmout_buf: String::new(),
            llmout_numln: 1,
            stdout: stdout(),
            ctx,
            polling_mode: PollingMode::AwaitRequestUpdate,
            metadata: OutputMetadata::new(),
        }
    }

    fn print_ln(&mut self, s: String) -> std::io::Result<()> {
        let tsize = terminal::size()?;
        let LinesInfo {numlines, ..} = self.userin.get_lines_info();
        let strwidth = s.as_str().width();
        let strnrow = (strwidth / (tsize.0 as usize) + 1) as u16;
        self.userin.move_down(strnrow); 
        execute!(self.stdout,
            cursor::SavePosition,
            cursor::MoveToPreviousLine(numlines as u16),
            style::Print(format!("{s}\n")),
            cursor::RestorePosition,
        )?;
        Ok(())
    }

    fn output_idx_to_term_pos(&self, tsize: (u16, u16), output_str_idx: usize) -> (u16, u16) {
        let mut idx_row = 0;
        let mut idx_col = 0;
        self.llmout_buf.char_indices().for_each(|(i, c)| {
            if i >= output_str_idx { return; }
            match c {
                '\n' => { 
                    idx_row += 1;
                    idx_col = 0;
                },
                _ => {
                    let will_wrap = (idx_col as usize) + c.width().unwrap_or(0) > tsize.0 as usize;
                    if will_wrap {
                        idx_row += 1;
                        idx_col = 0;
                    } else {
                        idx_col += c.width().unwrap_or(0);
                    }
                }
            }
        });
        (idx_row as u16, idx_col as u16)
    }

    pub fn run(mut self, tx_tty: Sender<TermTaskMessage>, rx_ans: Receiver<RequestTaskMessage>) -> std::io::Result<()> {
        // make some room
        println!("");
        self.userin.prepare_ui()?;
        terminal::enable_raw_mode()?;

        let supports_keyboard_enhancement = matches!(
            crossterm::terminal::supports_keyboard_enhancement(),
            Ok(true)
        );

        if supports_keyboard_enhancement {
            execute!(
                self.stdout,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
        }

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
                        self.metadata.generate(&self.llmout_buf);
                        match self.metadata.get() {
                            Some((start, end)) => {
                                let (start_row, start_col) = self.output_idx_to_term_pos(tsize, start);
                                let LinesInfo {numlines: userin_ln, ..} = self.userin.get_lines_info();

                                let t_height = tsize.1 as i32;
                                let block_height = (userin_ln + self.llmout_numln) as i32;
                                let start_row_rel = (t_height - block_height + start_row as i32).clamp(0, u16::MAX.into()) as u16;

                                let mut longest_ln_width = 0;
                                for ln in self.llmout_buf[start..end].lines() {
                                    let w = ln.width();
                                    if w > longest_ln_width { longest_ln_width = w; }
                                }

                                let mut content_style = style::ContentStyle::new();
                                content_style.background_color = Some(style::Color::Black);
                                content_style.foreground_color = Some(style::Color::White);

                                queue!(self.stdout, cursor::SavePosition, cursor::MoveTo(start_col, start_row_rel))?;
                                for ln in self.llmout_buf[start..end].lines() {
                                    queue!(self.stdout, 
                                        style::PrintStyledContent(content_style.apply(ln)), 
                                        style::PrintStyledContent(content_style.apply(" ".repeat(longest_ln_width - ln.width()))),
                                        style::Print("\n"),
                                        cursor::MoveToColumn(0)
                                    )?;
                                }
                                queue!(self.stdout, cursor::RestorePosition)?;
                                self.stdout.flush()?;
                            },
                            None => (),
                        }
                        next_polling = Some(PollingMode::AwaitUserin);
                    },
                    RequestTaskMessage::ReceivedPiece(piece) => {
                        let LinesInfo {numlines, ..} = self.userin.get_lines_info();
                        piece.chars().for_each(|c| {
                            let mut curscol = last_line_width(self.llmout_buf.as_str(), last_wrap_idx) as u16;
                            self.llmout_buf.push(c);
                            match c {
                                '\n' => {
                                    self.userin.move_down(1);
                                    self.llmout_numln += 1;
                                },
                                _ => {
                                    let will_wrap = (curscol as usize) + c.width().unwrap_or(0) > tsize.0 as usize;
                                    if will_wrap {
                                        last_wrap_idx = self.llmout_buf
                                            .char_indices()
                                            .last()
                                            .unwrap_or((0,'\0')).0;

                                        self.userin.move_down(1);
                                        self.llmout_numln += 1;
                                        curscol = 0;
                                    }

                                    let _ = queue!(self.stdout,
                                        cursor::SavePosition,
                                        cursor::MoveToPreviousLine(numlines as u16),
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
                if 
                event.kind == event::KeyEventKind::Press && 
                event.code == event::KeyCode::Enter && 
                event.modifiers == event::KeyModifiers::NONE && 
                self.userin.buf.is_empty() {
                    let _ = tx_tty.send(TermTaskMessage::Die);
                    break;
                }
                if event.modifiers == event::KeyModifiers::CONTROL && event.code == event::KeyCode::Char('c') {
                    let _ = tx_tty.send(TermTaskMessage::Die);
                    break;
                }
            }

            match event {
                event::Event::Key(evt) if evt.kind != event::KeyEventKind::Release => match evt.code {
                    event::KeyCode::Char(c) => {
                        self.userin.buf.push(c);
                        let _ = execute!(self.stdout, style::Print(c));
                    }
                    event::KeyCode::Backspace => { self.userin.remove_last(tsize); },
                    event::KeyCode::Enter => {
                        if evt.modifiers == event::KeyModifiers::SHIFT {
                            self.userin.buf.push('\n');
                            let _ = execute!(self.stdout, style::Print('\n'), cursor::MoveToColumn(0));
                        } else {
                            let userin_saved = self.userin.buf.clone();
                            let llmout_saved = self.llmout_buf.clone();

                            self.userin.buf.clear();
                            self.llmout_buf.clear();
                            self.llmout_numln = 1;
                            last_wrap_idx = 0;
                            next_polling = Some(PollingMode::AwaitRequestUpdate);

                            self.print_ln(format!("{} {}", UserIn::PREFIX, userin_saved))?;
                            self.userin.refresh();
                            self.userin.move_down(1);
                            
                            self.metadata.clear();

                            let _ = tx_tty.send(TermTaskMessage::ReceivedUserPrompt {user_prompt: userin_saved, llm_answer_prev: Some(llmout_saved)});
                        }

                        self.userin.count_lines(tsize);
                    }
                    _ => () 
                },
                _ => ()
            }
        }

        if supports_keyboard_enhancement {
            execute!(self.stdout, PopKeyboardEnhancementFlags)?;
        }

        terminal::disable_raw_mode()?;
        Ok(())
    }
}
