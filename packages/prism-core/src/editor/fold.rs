use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::buffer::Buffer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldedRange {
    pub end_line: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoldState {
    folds: BTreeMap<usize, FoldedRange>,
}

impl FoldState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_fold(&mut self, line: usize, buffer: &Buffer, tab_width: usize) {
        if self.folds.contains_key(&line) {
            self.folds.remove(&line);
        } else {
            self.fold_at(line, buffer, tab_width);
        }
    }

    pub fn fold_at(&mut self, line: usize, buffer: &Buffer, tab_width: usize) {
        if self.folds.contains_key(&line) {
            return;
        }
        if let Some(end) = find_fold_end(buffer, line, tab_width) {
            let hidden = end - line;
            self.folds.insert(
                line,
                FoldedRange {
                    end_line: end,
                    preview: format!("... {hidden} lines"),
                },
            );
        }
    }

    pub fn unfold_at(&mut self, line: usize) {
        self.folds.remove(&line);
    }

    pub fn unfold_all(&mut self) {
        self.folds.clear();
    }

    pub fn is_fold_start(&self, line: usize) -> bool {
        self.folds.contains_key(&line)
    }

    pub fn is_hidden(&self, line: usize) -> bool {
        self.folds
            .iter()
            .any(|(&start, range)| line > start && line <= range.end_line)
    }

    pub fn get_fold(&self, line: usize) -> Option<&FoldedRange> {
        self.folds.get(&line)
    }

    pub fn folds(&self) -> &BTreeMap<usize, FoldedRange> {
        &self.folds
    }

    pub fn invalidate_from(&mut self, line: usize) {
        let stale: Vec<usize> = self
            .folds
            .iter()
            .filter(|(&start, range)| start >= line || range.end_line >= line)
            .map(|(&k, _)| k)
            .collect();
        for k in stale {
            self.folds.remove(&k);
        }
    }
}

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| *c == ' ').count()
}

pub fn is_foldable(buffer: &Buffer, line: usize, tab_width: usize) -> bool {
    find_fold_end(buffer, line, tab_width).is_some()
}

fn find_fold_end(buffer: &Buffer, line: usize, _tab_width: usize) -> Option<usize> {
    let count = buffer.line_count();
    if line + 1 >= count {
        return None;
    }

    let text = buffer.line(line)?;
    let trimmed = text.trim_end_matches('\n');
    if trimmed.is_empty() {
        return None;
    }

    let base_indent = leading_spaces(trimmed);

    // Find next non-blank line
    let mut next = line + 1;
    while next < count {
        if let Some(t) = buffer.line(next) {
            if !t.trim_end_matches('\n').is_empty() {
                break;
            }
        }
        next += 1;
    }
    if next >= count {
        return None;
    }

    let next_text = buffer.line(next)?;
    let next_indent = leading_spaces(next_text.trim_end_matches('\n'));
    if next_indent <= base_indent {
        return None;
    }

    let mut end = next;
    let mut last_nonblank_end = next;
    for scan in (next + 1)..count {
        if let Some(t) = buffer.line(scan) {
            let st = t.trim_end_matches('\n');
            if st.is_empty() {
                continue;
            }
            let indent = leading_spaces(st);
            if indent <= base_indent {
                // Include a closing bracket at exactly base_indent
                let first = st.trim_start().chars().next().unwrap_or(' ');
                if indent == base_indent && matches!(first, '}' | ')' | ']') {
                    end = scan;
                    last_nonblank_end = scan;
                }
                break;
            }
            end = scan;
            last_nonblank_end = scan;
        }
    }

    let _ = end;
    if last_nonblank_end > line {
        Some(last_nonblank_end)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_text(s)
    }

    #[test]
    fn simple_function_fold() {
        let b = buf("fn main() {\n    let x = 1;\n    let y = 2;\n}");
        assert!(is_foldable(&b, 0, 4));
        assert!(!is_foldable(&b, 1, 4));
        assert!(!is_foldable(&b, 3, 4));

        let end = find_fold_end(&b, 0, 4).unwrap();
        assert_eq!(end, 3); // includes closing brace
    }

    #[test]
    fn nested_folds() {
        let b = buf("fn main() {\n    if true {\n        x();\n    }\n}");
        assert!(is_foldable(&b, 0, 4));
        assert!(is_foldable(&b, 1, 4));

        assert_eq!(find_fold_end(&b, 0, 4).unwrap(), 4);
        assert_eq!(find_fold_end(&b, 1, 4).unwrap(), 3);
    }

    #[test]
    fn no_fold_for_flat() {
        let b = buf("let a = 1;\nlet b = 2;");
        assert!(!is_foldable(&b, 0, 4));
    }

    #[test]
    fn toggle_fold() {
        let b = buf("fn f() {\n    x();\n}");
        let mut state = FoldState::new();
        state.toggle_fold(0, &b, 4);
        assert!(state.is_fold_start(0));
        assert!(state.is_hidden(1));
        assert!(state.is_hidden(2));

        state.toggle_fold(0, &b, 4);
        assert!(!state.is_fold_start(0));
        assert!(!state.is_hidden(1));
    }

    #[test]
    fn fold_preview_text() {
        let b = buf("fn f() {\n    a();\n    b();\n    c();\n}");
        let mut state = FoldState::new();
        state.fold_at(0, &b, 4);
        let fold = state.get_fold(0).unwrap();
        assert_eq!(fold.preview, "... 4 lines");
    }

    #[test]
    fn invalidate_from_clears_affected() {
        let b = buf("fn f() {\n    x();\n}\nfn g() {\n    y();\n}");
        let mut state = FoldState::new();
        state.fold_at(0, &b, 4);
        state.fold_at(3, &b, 4);
        state.invalidate_from(2);
        assert!(state.folds().is_empty());
    }

    #[test]
    fn blank_lines_inside_fold() {
        let b = buf("fn f() {\n    a();\n\n    b();\n}");
        let end = find_fold_end(&b, 0, 4).unwrap();
        assert_eq!(end, 4);
    }

    #[test]
    fn unfold_all() {
        let b = buf("fn f() {\n    x();\n}\nfn g() {\n    y();\n}");
        let mut state = FoldState::new();
        state.fold_at(0, &b, 4);
        state.fold_at(3, &b, 4);
        state.unfold_all();
        assert!(state.folds().is_empty());
    }
}
