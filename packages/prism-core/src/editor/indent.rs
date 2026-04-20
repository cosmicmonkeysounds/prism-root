use super::buffer::Buffer;

#[derive(Debug, Clone, PartialEq)]
pub struct IndentGuide {
    pub depth: u32,
    pub active: bool,
}

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| *c == ' ').count()
}

fn line_indent_depth(buffer: &Buffer, line: usize, tab_width: usize) -> usize {
    buffer
        .line(line)
        .map(|t| leading_spaces(t.trim_end_matches('\n')) / tab_width.max(1))
        .unwrap_or(0)
}

fn effective_indent(buffer: &Buffer, line: usize, tab_width: usize) -> usize {
    if let Some(text) = buffer.line(line) {
        let trimmed = text.trim_end_matches('\n');
        if !trimmed.is_empty() {
            return leading_spaces(trimmed) / tab_width.max(1);
        }
        // Blank line: inherit the shallower of nearest non-blank neighbors
        let back = scan_nonblank_up(buffer, line)
            .map(|l| line_indent_depth(buffer, l, tab_width))
            .unwrap_or(0);
        let fwd = scan_nonblank_down(buffer, line)
            .map(|l| line_indent_depth(buffer, l, tab_width))
            .unwrap_or(0);
        back.min(fwd)
    } else {
        0
    }
}

fn scan_nonblank_up(buffer: &Buffer, from: usize) -> Option<usize> {
    for i in (0..from).rev().take(25) {
        if let Some(t) = buffer.line(i) {
            if !t.trim_end_matches('\n').is_empty() {
                return Some(i);
            }
        }
    }
    None
}

fn scan_nonblank_down(buffer: &Buffer, from: usize) -> Option<usize> {
    let count = buffer.line_count();
    for i in (from + 1)..count.min(from + 26) {
        if let Some(t) = buffer.line(i) {
            if !t.trim_end_matches('\n').is_empty() {
                return Some(i);
            }
        }
    }
    None
}

fn enclosing_indent_depth(buffer: &Buffer, cursor_line: usize, tab_width: usize) -> usize {
    if let Some(text) = buffer.line(cursor_line) {
        let trimmed = text.trim_end_matches('\n');
        if trimmed.is_empty() {
            return effective_indent(buffer, cursor_line, tab_width);
        }
        let own = leading_spaces(trimmed) / tab_width.max(1);
        if own > 0 {
            return own;
        }
        // Depth 0 line — check if it starts a deeper block
        if let Some(below) = scan_nonblank_down(buffer, cursor_line) {
            let below_depth = line_indent_depth(buffer, below, tab_width);
            if below_depth > 0 {
                return 1;
            }
        }
    }
    0
}

pub fn compute_line_indent_guides(
    buffer: &Buffer,
    line: usize,
    tab_width: usize,
    active_depth: usize,
) -> Vec<IndentGuide> {
    let depth = effective_indent(buffer, line, tab_width);
    (1..=depth)
        .map(|d| IndentGuide {
            depth: d as u32,
            active: d == active_depth,
        })
        .collect()
}

pub fn active_indent_depth(buffer: &Buffer, cursor_line: usize, tab_width: usize) -> usize {
    enclosing_indent_depth(buffer, cursor_line, tab_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_text(s)
    }

    #[test]
    fn flat_code_no_guides() {
        let b = buf("fn main() {}");
        let guides = compute_line_indent_guides(&b, 0, 4, 0);
        assert!(guides.is_empty());
    }

    #[test]
    fn single_indent_level() {
        let b = buf("fn main() {\n    let x = 1;\n}");
        let guides = compute_line_indent_guides(&b, 1, 4, 1);
        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].depth, 1);
        assert!(guides[0].active);
    }

    #[test]
    fn nested_indent_levels() {
        let b = buf("fn main() {\n    if true {\n        println!();\n    }\n}");
        let guides = compute_line_indent_guides(&b, 2, 4, 2);
        assert_eq!(guides.len(), 2);
        assert_eq!(guides[0].depth, 1);
        assert!(!guides[0].active);
        assert_eq!(guides[1].depth, 2);
        assert!(guides[1].active);
    }

    #[test]
    fn blank_line_inherits_shallower() {
        let b = buf("fn main() {\n    let a = 1;\n\n    let b = 2;\n}");
        // Line 2 is blank, surrounded by depth-1 lines
        let depth = effective_indent(&b, 2, 4);
        assert_eq!(depth, 1);
        let guides = compute_line_indent_guides(&b, 2, 4, 1);
        assert_eq!(guides.len(), 1);
    }

    #[test]
    fn blank_line_at_boundary() {
        let b = buf("fn main() {\n    let x = 1;\n}\n\nfn other() {}");
        // Line 3 is blank between depth-0 lines
        let depth = effective_indent(&b, 3, 4);
        assert_eq!(depth, 0);
    }

    #[test]
    fn active_depth_at_cursor() {
        let b = buf("fn main() {\n    if true {\n        x();\n    }\n}");
        assert_eq!(active_indent_depth(&b, 0, 4), 1);
        assert_eq!(active_indent_depth(&b, 1, 4), 1);
        assert_eq!(active_indent_depth(&b, 2, 4), 2);
        assert_eq!(active_indent_depth(&b, 3, 4), 1);
        assert_eq!(active_indent_depth(&b, 4, 4), 0);
    }

    #[test]
    fn tab_width_2() {
        let b = buf("fn f() {\n  x();\n  if true {\n    y();\n  }\n}");
        let guides = compute_line_indent_guides(&b, 3, 2, 2);
        assert_eq!(guides.len(), 2);
    }
}
