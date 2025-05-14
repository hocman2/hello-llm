use unicode_width::UnicodeWidthStr;

pub trait StrExt {
    /// returns the total number of lines, including wrapped lines and the width of the last line
    fn wrapped_width(&self, t_width: u16) -> (u32, usize);
}

impl StrExt for str {
    fn wrapped_width(&self, t_width: u16) -> (u32, usize) {
        if self.len() == 0 {
            return (0, 0);
        }

        let mut ln_width = 0;
        let ln_num: u32 = self.lines().fold(0, |mut ln_num, ln| {
            let w = ln.width();
            ln_num += (w as u32 + t_width as u32 - 1) / (t_width as u32);

            ln_width = w % (t_width as usize);
            // edge case if it perfectly matches t_width
            if ln_width == 0 && w > 0 { ln_width = t_width as usize; }

            ln_num
        });

        // if the last line ends in a newline, the lines iterator would miss this since it cuts after the \n character
        if self.chars().last().unwrap_or('\0') == '\n' {
            ln_width = 0;
        }

        (ln_num, ln_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_width() {
        const TEST_STR_1: &'static str = "Once upon a time
A lazy person decided to...

";
        let (_nl, w) = TEST_STR_1.wrapped_width(96);
        println!("{TEST_STR_1:?}");
        assert_eq!(w, 0);

        const TEST_STR_2: &'static str = "Once upon a time

    ";
        let (_nl, w) = TEST_STR_2.wrapped_width(96);
        println!("{TEST_STR_2:?}");
        assert_eq!(w, 4);

        const TEST_STR_3: &'static str = "Once upon a time

    x";
        let (_nl, w) = TEST_STR_3.wrapped_width(96);
        println!("{TEST_STR_3:?}");
        assert_eq!(w, 5);
    }
}
