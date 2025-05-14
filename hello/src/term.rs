mod output_metadata_gen;
mod str_ext;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use str_ext::StrExt;
use std::io::{stdout, Stdout, Write};
use std::time::Duration;
use crossterm::{queue, execute, cursor, style, event, terminal};
use crossterm::event::{PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags, KeyboardEnhancementFlags};
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
use std::sync::mpsc::{Receiver, Sender};
use crate::context::Context;
use crate::request::RequestTaskMessage;
use output_metadata_gen::OutputMetadata;

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
    /// Represents the distance from the bottom row to the last output_row + 1
    /// This is used to calculate to always redraw multiline userin at the same row
    /// can only increase or be reset
    dist_to_output: u32,
    lines_info: LinesInfo,
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
            dist_to_output: 0,
        }
    }

    fn reset(&mut self) {
        self.buf = String::new();
        self.dist_to_output = 1;
        self.lines_info.numlines = 1;
        self.lines_info.last_ln_width = 0;
    }

    // actively recounts userin lines, use this if userin buffer has been modified, otherwise used
    // cached get_lines_info
    fn count_lines(&mut self, tsize: (u16, u16)) -> LinesInfo {
        let (mut numrows, last_ln_width) = format!("{} {}", Self::PREFIX, self.buf).wrapped_width(tsize.0);

        // might happen if the buffer is empty, lines would be an empty iterator
        if numrows == 0 { numrows = 1; }

        if numrows > self.dist_to_output { self.dist_to_output = numrows; }

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
}

pub enum TermTaskMessage {
    ReceivedUserPrompt {
        user_prompt: String,
        llm_answer_prev: Option<String>
    },
    Die,
}

#[allow(unused)]
pub struct TermTask {
    userin: UserIn,
    llmout_buf: String,
    stdout: Stdout,
    ctx: Context,
    polling_mode: PollingMode,
    metadata: OutputMetadata,
    selected_code_block: usize,
    tsize: (u16, u16),
}

impl TermTask {
    pub fn new(ctx: Context) -> Self {
        Self {
            userin: UserIn::new(),
            llmout_buf: String::new(),
            stdout: stdout(),
            ctx,
            polling_mode: PollingMode::AwaitRequestUpdate,
            metadata: OutputMetadata::new(),
            selected_code_block: 0,
            tsize: (0, 0),
        }
    }

    // Writes a piece on screen at position with proper wrapping and cursor movement,
    // scrolling at each newline
    pub fn print(&mut self, s: &str, mut col: u16, row: u16) -> std::io::Result<()> {
        queue!(self.stdout, cursor::MoveTo(col, row))?;

        for c in s.chars() {
            // when the row on which we print is not the last one we must force a scroll in case of newline
            let w = c.width().unwrap_or(0) as u16;
            let will_wrap = col + w > self.tsize.0 || c == '\n';
            if will_wrap {
                queue!(self.stdout,
                    cursor::SavePosition,
                    cursor::MoveToRow(self.tsize.1-1),
                    style::Print('\n'),
                    cursor::RestorePosition,
                    cursor::MoveToColumn(0)
                )?;
                col = 0;
            }

            match c {
                '\n' => {/* Case already handled above */},
                _ => {
                    queue!(self.stdout, style::Print(c))?;
                    col += w;
                }
            }
        }

        self.stdout.flush()?;
        Ok(())
    }

    fn clear_userin(&mut self) -> std::io::Result<()> {
        queue!(self.stdout, cursor::SavePosition)?;

        let dist_to_output = self.userin.dist_to_output.clamp(0, u16::MAX as u32) as u16;
        // Will crash if the input is outside of the screen upwards !!! will be fixed
        let userin_top = self.tsize.1 - dist_to_output;

        for n in userin_top..self.tsize.1+1 {
            queue!(self.stdout,
                cursor::MoveToRow(n),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
        }

        queue!(self.stdout, cursor::RestorePosition)?;
        self.stdout.flush()?;
        Ok(())
    }

    /// The refresh function is meant to be called in an input round, it redraws the userin always at a fixed point
    fn refresh_userin(&mut self) -> std::io::Result<()> {
        self.clear_userin()?;
        let userin_str = format!("{} {}", UserIn::PREFIX, self.userin.buf.as_str());
        let dist_to_output = self.userin.dist_to_output.clamp(0, u16::MAX as u32) as u16;

        let _ = disable_raw_mode();
        execute!(self.stdout,
            cursor::MoveTo(0, self.tsize.1 - dist_to_output),
            style::Print(userin_str)
        )?;
        let _ = enable_raw_mode();

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

    fn set_highlight(&mut self, highlight: bool, start: usize, end: usize) -> std::io::Result<()> {
        let (start_row, start_col) = self.output_idx_to_term_pos(self.tsize, start);
        let LinesInfo {numlines: userin_ln, ..} = self.userin.get_lines_info();
        let (out_ln, _) = self.llmout_buf.wrapped_width(self.tsize.0);

        let t_height = self.tsize.1 as i32;
        let block_height = (userin_ln + out_ln) as i32;
        let start_row_rel = (t_height - block_height + start_row as i32).clamp(0, u16::MAX.into()) as u16;

        let mut longest_ln_width = 0;
        let lines: Vec<&str> = self.llmout_buf[start..end].lines().collect();
        let last_widths: Vec<usize> = lines.iter().map(|ln| {
            let w = ln.width();
            let clamped = w.clamp(0, self.tsize.0 as usize);
            if clamped > longest_ln_width { longest_ln_width = clamped; }
            w % self.tsize.0 as usize
        }).collect();

        let mut content_style = style::ContentStyle::new();
        content_style.background_color = if highlight { Some(style::Color::White) } else { Some(style::Color::Reset) };
        content_style.foreground_color = if highlight { Some(style::Color::Black) } else { Some(style::Color::Reset) };

        queue!(self.stdout, cursor::SavePosition, cursor::MoveTo(start_col, start_row_rel))?;
        for (ln, w) in lines.iter().zip(last_widths.iter()) {
            queue!(self.stdout,
                style::PrintStyledContent(content_style.apply(ln)),
                style::PrintStyledContent(content_style.apply(" ".repeat(longest_ln_width - w))),
                style::Print("\n"),
                cursor::MoveToColumn(0)
            )?;
        }
        queue!(self.stdout, cursor::RestorePosition)?;
        self.stdout.flush()?;
        Ok(())
    }

    pub fn run(mut self, tx_tty: Sender<TermTaskMessage>, rx_ans: Receiver<RequestTaskMessage>) -> std::io::Result<()> {
        // make some room
        println!("");
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

        let run_task = true;
        let mut next_polling: Option<PollingMode> = None;
        while run_task {
            if let Some(next_polling) = next_polling.take() {
                self.polling_mode = next_polling;
            }

            let tsize = terminal::size()?;
            self.tsize = tsize;
            let message: Option<RequestTaskMessage> = match self.polling_mode {
                PollingMode::AwaitRequestUpdate => rx_ans.recv_timeout(Duration::from_millis(30)).map_or(None, |o| Some(o)),
                // try_recv out of safety but we could just return None
                PollingMode::AwaitUserin => rx_ans.try_recv().map_or(None, |o| Some(o)),
            };

            if let Some(message) = message {
                match message {
                    RequestTaskMessage::Done => {
                        self.metadata.generate(&self.llmout_buf);
                        next_polling = Some(PollingMode::AwaitUserin);
                    },
                    RequestTaskMessage::ReceivedPiece(piece) => {
                        self.clear_userin()?;

                        let LinesInfo {numlines: userin_ln, ..} = self.userin.get_lines_info();
                        let (_, curscol) = self.llmout_buf.wrapped_width(tsize.0);
                        let current_row = tsize.1 - (userin_ln as u16) - 1;
                        self.print(&piece, curscol as u16, current_row)?;
                        self.llmout_buf.push_str(&piece);

                        let userin_str = format!("{} {}", UserIn::PREFIX, self.userin.buf.as_str());
                        self.print(&userin_str, 0, self.tsize.1)?;
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
                        self.userin.count_lines(self.tsize);
                        execute!(self.stdout, style::Print(c))?;
                    }
                    event::KeyCode::Up => {
                        let block_maybe = self.metadata.code_blocks().get(self.selected_code_block).cloned();
                        match block_maybe {
                            Some(block) => { self.set_highlight(false, block.start, block.end)?; }
                            None => ()
                        };

                        let code_blocks = self.metadata.code_blocks();
                        self.selected_code_block = if self.selected_code_block == 0 {
                            code_blocks.len() - 1
                        }
                        else {
                            self.selected_code_block - 1
                        };

                        match code_blocks.get(self.selected_code_block) {
                            Some(block) => { self.set_highlight(true, block.start, block.end)?; }
                            None => ()
                        };
                    },
                    event::KeyCode::Down => {
                        let block_maybe = self.metadata.code_blocks().get(self.selected_code_block).cloned();
                        match block_maybe {
                            Some(block) => { self.set_highlight(false, block.start, block.end)?; }
                            None => ()
                        };

                        let code_blocks = self.metadata.code_blocks();
                        self.selected_code_block = if self.selected_code_block == code_blocks.len() - 1 {
                            0
                        }
                        else {
                            self.selected_code_block + 1
                        };

                        match code_blocks.get(self.selected_code_block) {
                            Some(block) => { self.set_highlight(true, block.start, block.end)?; }
                            None => ()
                        };
                    },
                    event::KeyCode::Backspace => {
                        self.userin.buf.pop();
                        self.userin.count_lines(self.tsize);
                        self.refresh_userin()?;
                    },
                    event::KeyCode::Enter => {
                        if evt.modifiers == event::KeyModifiers::SHIFT {
                            self.userin.buf.push('\n');
                            self.userin.count_lines(self.tsize);
                            execute!(self.stdout, style::Print('\n'), cursor::MoveToColumn(0))?;
                        } else {
                            next_polling = Some(PollingMode::AwaitRequestUpdate);

                            match self.metadata.code_blocks().get(self.selected_code_block) {
                                Some(block) => {
                                    self.set_highlight(false, block.start, block.end)?;
                                },
                                None => (),
                            }

                            // printing two newlines just shifts current userin up and leaves a blank space for future llm output
                            self.print("\n\n", 0, tsize.1 - self.userin.get_lines_info().numlines as u16 - 1)?;

                            let llmout_saved = self.llmout_buf.clone();
                            let userin_saved = self.userin.buf.clone();

                            self.llmout_buf.clear();
                            self.metadata.clear();

                            self.userin.reset();
                            let userin_str = format!("{} {}", UserIn::PREFIX, self.userin.buf.as_str());
                            self.print(&userin_str, 0, self.tsize.1)?;

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
