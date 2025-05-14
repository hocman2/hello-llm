use unicode_width::UnicodeWidthChar;

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
        let mut ln_num = 1;
        for c in self.chars() {
            let w = c.width().unwrap_or(0);
            if ln_width + w > t_width as usize || c == '\n' {
               ln_num += 1;
               ln_width = w;
            } else {
                ln_width += w;
            }
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
        let (nl, w) = TEST_STR_1.wrapped_width(96);
        println!("{TEST_STR_1:?}");
        assert_eq!(w, 0);
        assert_eq!(nl, 4);

        const TEST_STR_2: &'static str = "Once upon a time

    ";
        let (nl, w) = TEST_STR_2.wrapped_width(96);
        println!("{TEST_STR_2:?}");
        assert_eq!(w, 4);
        assert_eq!(nl, 3);

        const TEST_STR_3: &'static str = "Once upon a time
    x";
        let (nl, w) = TEST_STR_3.wrapped_width(96);
        println!("{TEST_STR_3:?}");
        assert_eq!(w, 5);
        assert_eq!(nl, 2);
    }
}
