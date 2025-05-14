#[derive(Debug, Clone)]
#[allow(unused)]
pub struct CodeBlock {
    pub lang: Option<String>,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
enum CurrentElement {
    None,
    CodeBlock(CodeBlock),
}

#[derive(Debug)]
pub struct OutputMetadata {
    code_blocks: Vec<CodeBlock>,
    last_stop: usize,
    curr: CurrentElement,
}

impl OutputMetadata {
    pub fn new() -> Self {
       Self {
            code_blocks: Vec::new(),
            last_stop: 0,
            curr: CurrentElement::None,
       }
    }
    pub fn clear(&mut self) {
        self.code_blocks = Vec::new();
        self.last_stop = 0;
        self.curr = CurrentElement::None;
    }

    pub fn generate(&mut self, content: &str) {
        let mut start_idx = 0;
        content.lines().for_each(|ln| {
            let end_idx = start_idx + ln.len();
            match ln.strip_prefix("```") {
                Some(trimmed) => {
                    if let CurrentElement::None = &self.curr {
                        self.curr = CurrentElement::CodeBlock(CodeBlock {
                            lang: if trimmed.len() == 0 { None } else { Some(String::from(trimmed)) },
                            start: end_idx + 1,
                            end: 0,
                        });
                    }
                    else if let CurrentElement::CodeBlock(ref mut block) = &mut self.curr {
                        self.code_blocks.push(block.clone());
                        self.curr = CurrentElement::None;
                    }
                },
                None => (),
            }

            match &mut self.curr {
                CurrentElement::CodeBlock(ref mut block) => {
                    block.end = end_idx;
                },
                CurrentElement::None => (),
            }

            start_idx = end_idx + 1;
        });
    }

    pub fn code_blocks(&self) -> &[CodeBlock] {
        &self.code_blocks
    }
}
