use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ELLIPSIS: char = '…';
const MAX_CELL_WIDTH: usize = 40;

pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn elide(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if width(text) <= max {
        return text.to_string();
    }

    let budget = max - 1;
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cell = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cell > budget {
            break;
        }
        out.push(ch);
        used += cell;
    }
    out.push(ELLIPSIS);
    out
}

pub fn pad(text: &str, target: usize) -> String {
    if target == 0 {
        return text.to_string();
    }
    let current = width(text);
    if current >= target {
        return elide(text, target);
    }
    let mut out = String::with_capacity(text.len() + (target - current));
    out.push_str(text);
    out.extend(std::iter::repeat_n(' ', target - current));
    out
}

pub fn column_widths(headers: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = headers.iter().map(|header| width(header)).collect();

    for row in rows {
        for (index, cell) in row.iter().enumerate().take(widths.len()) {
            widths[index] = widths[index].max(width(cell));
        }
    }

    for value in &mut widths {
        *value = (*value).min(MAX_CELL_WIDTH);
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_short_text_untouched() {
        assert_eq!(elide("abc", 8), "abc");
    }

    #[test]
    fn elides_to_budget_including_ellipsis() {
        let elided = elide("abcdefghij", 5);
        assert_eq!(elided, "abcd…");
        assert_eq!(width(&elided), 5);
    }

    #[test]
    fn elides_multibyte_without_overflowing() {
        let elided = elide("héllo wörld", 6);
        assert!(width(&elided) <= 6);
    }

    #[test]
    fn pads_to_exact_cell_width() {
        let padded = pad("ab", 5);
        assert_eq!(padded, "ab   ");
        assert_eq!(width(&padded), 5);
    }

    #[test]
    fn padding_to_zero_leaves_text_untouched() {
        assert_eq!(pad("2/2", 0), "2/2");
    }

    #[test]
    fn column_widths_use_widest_cell_and_cap() {
        let headers = vec!["field".to_string(), "value".to_string()];
        let rows = vec![
            vec!["a".to_string(), "short".to_string()],
            vec!["b".repeat(MAX_CELL_WIDTH), "x".to_string()],
        ];
        let widths = column_widths(&headers, &rows);

        assert_eq!(widths[0], MAX_CELL_WIDTH, "widest cell, capped");
        assert_eq!(widths[1], width("short"), "widest of header and cells");
    }

    #[test]
    fn column_widths_cap_an_oversized_cell() {
        let headers = vec!["field".to_string()];
        let rows = vec![vec!["b".repeat(MAX_CELL_WIDTH + 20)]];
        assert_eq!(column_widths(&headers, &rows)[0], MAX_CELL_WIDTH);
    }
}
