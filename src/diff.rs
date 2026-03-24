use similar::{ChangeTag, TextDiff};

use crate::state::ScopeSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDelta {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
}

impl FileDelta {
    pub fn display(&self) -> String {
        format!("+{}/-{}", self.insertions, self.deletions)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeDelta {
    pub insertions: usize,
    pub deletions: usize,
    pub display: String,
    pub files: Vec<FileDelta>,
}

impl ScopeDelta {
    pub fn total_changed(&self) -> usize {
        self.insertions + self.deletions
    }
}

pub fn diff_file(before: &str, after: &str) -> Option<(usize, usize, String)> {
    let diff = TextDiff::from_lines(before, after);
    let mut insertions = 0;
    let mut deletions = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Insert => insertions += 1,
            ChangeTag::Equal => {}
        }
    }

    if insertions == 0 && deletions == 0 {
        None
    } else {
        Some((insertions, deletions, format!("+{insertions}/-{deletions}")))
    }
}

pub fn scope_diff(before: &ScopeSnapshot, after: &ScopeSnapshot) -> Option<ScopeDelta> {
    let mut insertions = 0;
    let mut deletions = 0;
    let mut files = Vec::new();

    for path in before.files.keys().chain(after.files.keys()) {
        let before_text = before.files.get(path).map(String::as_str).unwrap_or("");
        let after_text = after.files.get(path).map(String::as_str).unwrap_or("");
        if let Some((file_ins, file_dels, _)) = diff_file(before_text, after_text) {
            if files.iter().any(|delta: &FileDelta| delta.path == *path) {
                continue;
            }
            insertions += file_ins;
            deletions += file_dels;
            files.push(FileDelta {
                path: path.clone(),
                insertions: file_ins,
                deletions: file_dels,
            });
        }
    }

    if insertions == 0 && deletions == 0 {
        None
    } else {
        Some(ScopeDelta {
            insertions,
            deletions,
            display: format!("+{insertions}/-{deletions}"),
            files,
        })
    }
}
