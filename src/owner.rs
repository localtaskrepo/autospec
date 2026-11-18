//! Decisions only the owner can make: `[NEEDS OWNER: …]` markers in the docs and the
//! `## Clarifications` section that records the owner's answers.

use std::ops::Range;
use std::path::Path;

use crate::docs::read_text_allow_missing;
use crate::error::Result;

const MARKER_START: &str = "[NEEDS OWNER:";
const CLARIFICATIONS_HEADING: &str = "## Clarifications";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerOption {
    pub key: String,
    pub label: String,
}

/// One open `[NEEDS OWNER: question | a) … | b) … | recommended: a]` marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerQuestion {
    /// Repo-relative path of the doc that holds the marker.
    pub file: String,
    /// The marker exactly as it appears in the doc.
    pub raw: String,
    pub question: String,
    pub options: Vec<OwnerOption>,
    pub recommended: Option<String>,
}

impl OwnerQuestion {
    /// The question as filed outside the doc: prefixed with the doc path so it stays unique.
    pub fn title(&self) -> String {
        format!("{}: {}", self.file, self.question)
    }
}

/// Open markers in the given repo-relative files, in file order.
pub fn scan_questions(repo_root: &Path, files: &[String]) -> Result<Vec<OwnerQuestion>> {
    let mut questions = Vec::new();
    for file in files {
        let text = read_text_allow_missing(&repo_root.join(file))?;
        questions.extend(parse_questions(file, &text));
    }
    Ok(questions)
}

/// Open markers in `text`. Markers that start inside a code span or a fenced code block
/// are examples of the format, not questions.
pub fn parse_questions(file: &str, text: &str) -> Vec<OwnerQuestion> {
    let code = code_ranges(text);
    let mut questions = Vec::new();
    let mut offset = 0;
    while let Some(found) = text[offset..].find(MARKER_START) {
        let start = offset + found;
        if let Some(code) = code.iter().find(|code| code.range.contains(&start)) {
            offset = code.range.end;
            continue;
        }
        let Some(end) = closing_bracket(text, start, &code) else {
            break;
        };
        if let Some(question) = parse_marker(file, &text[start..=end]) {
            questions.push(question);
        }
        offset = end + 1;
    }
    questions
}

/// A fenced code block or an inline code span, as a byte range of the doc.
struct Code {
    range: Range<usize>,
    fence: bool,
}

/// Fenced code blocks and inline code spans, in order.
fn code_ranges(text: &str) -> Vec<Code> {
    let mut ranges = Vec::new();
    let mut fence: Option<(char, usize, usize)> = None;
    let mut paragraph: Option<Range<usize>> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_range = offset..offset + line.len();
        offset = line_range.end;

        if let Some((ch, len, start)) = fence {
            if fence_run(line).is_some_and(|(close, close_len, info)| {
                close == ch && close_len >= len && info.trim().is_empty()
            }) {
                ranges.push(Code {
                    range: start..line_range.end,
                    fence: true,
                });
                fence = None;
            }
            continue;
        }

        let opens = fence_run(line).filter(|(ch, _, info)| *ch == '~' || !info.contains('`'));
        let heading = is_atx_heading(line);
        if (opens.is_some() || line.trim().is_empty() || heading || is_list_item(line))
            && let Some(range) = paragraph.take()
        {
            ranges.extend(code_spans(text, range));
        }
        match opens {
            Some((ch, len, _)) => fence = Some((ch, len, line_range.start)),
            None if line.trim().is_empty() => {}
            None if heading => ranges.extend(code_spans(text, line_range)),
            None => {
                paragraph =
                    Some(paragraph.map_or(line_range.clone(), |range| range.start..line_range.end));
            }
        }
    }
    if let Some((_, _, start)) = fence {
        ranges.push(Code {
            range: start..text.len(),
            fence: true,
        });
    }
    if let Some(range) = paragraph {
        ranges.extend(code_spans(text, range));
    }
    ranges.sort_by_key(|code| code.range.start);
    ranges
}

/// Whether `line` is an ATX heading: up to 3 spaces, 1 to 6 `#`, then whitespace or the line end.
fn is_atx_heading(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    line.len() - trimmed.len() <= 3
        && (1..=6).contains(&hashes)
        && trimmed[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

/// Whether `line` starts a list item: `-`, `*`, `+` or 1 to 9 digits and `.` or `)`, then
/// whitespace or the line end.
fn is_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    let marker = match trimmed[digits..].chars().next() {
        Some('-' | '*' | '+') if digits == 0 => 1,
        Some('.' | ')') if (1..=9).contains(&digits) => digits + 1,
        _ => return false,
    };
    trimmed[marker..]
        .chars()
        .next()
        .is_none_or(char::is_whitespace)
}

/// The fence character, its run length and the rest of the line, when `line` starts a fence.
fn fence_run(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let ch = trimmed
        .chars()
        .next()
        .filter(|ch| *ch == '`' || *ch == '~')?;
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    (len >= 3).then(|| (ch, len, &trimmed[len..]))
}

/// Code spans in `text[range]`: a backtick run closed by the next run of the same length.
fn code_spans(text: &str, range: Range<usize>) -> Vec<Code> {
    let bytes = text.as_bytes();
    let run_at = |index: usize| {
        bytes[index..range.end]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count()
    };
    let mut spans = Vec::new();
    let mut index = range.start;
    while index < range.end {
        match bytes[index] {
            b'\\' => index += 2,
            b'`' => {
                let len = run_at(index);
                let mut search = index + len;
                let mut close = None;
                while search < range.end {
                    if bytes[search] == b'`' {
                        let run = run_at(search);
                        if run == len {
                            close = Some(search + run);
                            break;
                        }
                        search += run;
                    } else {
                        search += 1;
                    }
                }
                match close {
                    Some(end) => {
                        spans.push(Code {
                            range: index..end,
                            fence: false,
                        });
                        index = end;
                    }
                    None => index += len,
                }
            }
            _ => index += 1,
        }
    }
    spans
}

/// Index in `text` of the `]` that closes the `[` at `start`, allowing nested brackets such
/// as links. Brackets inside code spans do not count. A marker is paragraph text: a blank
/// line or a fenced code block ends it unclosed.
fn closing_bracket(text: &str, start: usize, code: &[Code]) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = start;
    while let Some(ch) = text[index..].chars().next() {
        if let Some(code) = code.iter().find(|code| code.range.contains(&index)) {
            if code.fence {
                return None;
            }
            index = code.range.end;
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            '\n' if text[index + 1..].starts_with('\n') => return None,
            _ => {}
        }
        index += ch.len_utf8();
    }
    None
}

fn parse_marker(file: &str, raw: &str) -> Option<OwnerQuestion> {
    let body = &raw[MARKER_START.len()..raw.len() - 1];
    let mut parts = body.split('|').map(str::trim);
    let question = parts.next().filter(|part| !part.is_empty())?.to_owned();

    let mut options = Vec::new();
    let mut recommended = None;
    for part in parts.filter(|part| !part.is_empty()) {
        if let Some(key) = part.strip_prefix("recommended:") {
            recommended = Some(key.trim().trim_end_matches(')').to_owned());
            continue;
        }
        let (key, label) = match part.split_once(')') {
            Some((key, label)) if is_option_key(key.trim()) => {
                (key.trim().to_owned(), label.trim().to_owned())
            }
            _ => (option_key(options.len()), part.to_owned()),
        };
        options.push(OwnerOption { key, label });
    }
    let recommended = recommended.filter(|key| options.iter().any(|option| &option.key == key));

    Some(OwnerQuestion {
        file: file.to_owned(),
        raw: raw.to_owned(),
        question,
        options,
        recommended,
    })
}

fn is_option_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 12
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn option_key(index: usize) -> String {
    char::from(b'a' + (index % 26) as u8).to_string()
}

/// Replaces the marker (its first copy outside code) with the answer and records `- Q: … → A: …` under
/// `## Clarifications` / `### Session <date>`.
pub fn write_back(text: &str, question: &OwnerQuestion, answer: &str, date: &str) -> String {
    let code = code_ranges(text);
    let replaced = match text
        .match_indices(&question.raw)
        .map(|(start, _)| start)
        .find(|start| !code.iter().any(|code| code.range.contains(start)))
    {
        Some(start) => format!(
            "{}{answer}{}",
            &text[..start],
            &text[start + question.raw.len()..]
        ),
        None => text.to_owned(),
    };
    let entry = format!("- Q: {} → A: {}", question.question, answer);
    add_clarification(&replaced, &entry, date)
}

fn add_clarification(text: &str, entry: &str, date: &str) -> String {
    let session = format!("### Session {date}");
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();

    let Some(section) = lines
        .iter()
        .position(|line| line.trim_end() == CLARIFICATIONS_HEADING)
    else {
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend([
            CLARIFICATIONS_HEADING.to_owned(),
            String::new(),
            session,
            String::new(),
            entry.to_owned(),
        ]);
        return join_lines(lines);
    };

    let section_end = lines[section + 1..]
        .iter()
        .position(|line| is_heading_up_to(line, 2))
        .map_or(lines.len(), |offset| section + 1 + offset);

    let insert_at = match lines[section + 1..section_end]
        .iter()
        .position(|line| line.trim_end() == session)
    {
        Some(offset) => {
            let start = section + 1 + offset;
            let end = lines[start + 1..section_end]
                .iter()
                .position(|line| is_heading_up_to(line, 3))
                .map_or(section_end, |offset| start + 1 + offset);
            let insert_at = last_content_line(&lines, start, end) + 1;
            lines.insert(insert_at, entry.to_owned());
            insert_at
        }
        None => {
            let insert_at = last_content_line(&lines, section, section_end) + 1;
            lines.splice(
                insert_at..insert_at,
                [String::new(), session, String::new(), entry.to_owned()],
            );
            insert_at + 3
        }
    };

    if lines
        .get(insert_at + 1)
        .is_some_and(|line| !line.trim().is_empty())
    {
        lines.insert(insert_at + 1, String::new());
    }
    join_lines(lines)
}

/// The last non-empty line in `start..end`, or `start` when all are empty.
fn last_content_line(lines: &[String], start: usize, end: usize) -> usize {
    (start..end)
        .rev()
        .find(|index| !lines[*index].trim().is_empty())
        .unwrap_or(start)
}

fn is_heading_up_to(line: &str, level: usize) -> bool {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    (1..=level).contains(&hashes) && line[hashes..].starts_with(' ')
}

fn join_lines(lines: Vec<String>) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const MARKER: &str =
        "[NEEDS OWNER: How long do we keep logs? | a) 30 days | b) 90 days | recommended: a]";

    #[test]
    fn parses_marker_with_options_and_recommendation() {
        let text = format!("# Logs\n\nRetention: {MARKER}.\n");
        let questions = parse_questions("docs/logs.md", &text);

        assert_eq!(questions.len(), 1);
        let question = &questions[0];
        assert_eq!(question.raw, MARKER);
        assert_eq!(question.question, "How long do we keep logs?");
        assert_eq!(
            question.options,
            vec![
                OwnerOption {
                    key: "a".to_owned(),
                    label: "30 days".to_owned()
                },
                OwnerOption {
                    key: "b".to_owned(),
                    label: "90 days".to_owned()
                },
            ]
        );
        assert_eq!(question.recommended.as_deref(), Some("a"));
        assert_eq!(question.title(), "docs/logs.md: How long do we keep logs?");
    }

    #[test]
    fn parses_nested_links_and_options_without_keys() {
        let text = "[NEEDS OWNER: Which store, see [db](db.md)? | Postgres | SQLite]";
        let questions = parse_questions("docs/a.md", text);

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "Which store, see [db](db.md)?");
        assert_eq!(questions[0].options[1].key, "b");
        assert_eq!(questions[0].options[1].label, "SQLite");
        assert_eq!(questions[0].recommended, None);
    }

    #[test]
    fn brackets_in_code_spans_do_not_close_the_marker() {
        let text = "[NEEDS OWNER: use `]` here? | a) y] and `[NEEDS OWNER: x`] tail]";
        let questions = parse_questions("docs/a.md", text);

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "use `]` here?");
        assert_eq!(
            questions[0].options,
            vec![OwnerOption {
                key: "a".to_owned(),
                label: "y".to_owned()
            }]
        );
    }

    #[test]
    fn unmatched_backtick_in_marker_is_literal() {
        let questions = parse_questions("docs/a.md", "[NEEDS OWNER: use ` here? | a) y] after");

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "use ` here?");
        assert_eq!(questions[0].options[0].label, "y");
    }

    #[test]
    fn fenced_block_ends_the_marker_unclosed() {
        let text = "[NEEDS OWNER: q\n```\n\n\n```\n| a) y]";
        assert!(parse_questions("docs/a.md", text).is_empty());

        let text = "[NEEDS OWNER: q\n~~~\n] | a) y\n~~~\n| b) z]";
        assert!(parse_questions("docs/a.md", text).is_empty());
    }

    #[test]
    fn ignores_unterminated_markers() {
        let text = "[NEEDS OWNER: open question\n\nNext paragraph]";
        assert!(parse_questions("docs/a.md", text).is_empty());

        let text = "[NEEDS OWNER: use `x\n\n]` here? | a) y]";
        assert!(parse_questions("docs/a.md", text).is_empty());
    }

    #[test]
    fn skips_markers_in_code_spans() {
        let text = format!(
            "Mark it as `{MARKER}` or ``[NEEDS OWNER: q | a) `x` ]``.\n\
             Split `[NEEDS OWNER: across\nlines | a) x]` span.\n\n\
             Escaped \\`{MARKER}.\n"
        );
        let questions = parse_questions("docs/a.md", &text);

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].raw, MARKER);
    }

    #[test]
    fn code_spans_end_at_list_items_and_headings() {
        let cases = [
            "- item `a\n- [NEEDS OWNER: q | a) y] `b`\n",
            "1. item `a\n2) [NEEDS OWNER: q | a) y] `b`\n",
            "  * nested `a\n+ [NEEDS OWNER: q | a) y] `b`\n",
            "# Title `a\n[NEEDS OWNER: q | a) y] `b`\n",
            "Text `a\n### [NEEDS OWNER: q | a) y] `b`\n",
        ];
        for text in cases {
            assert_eq!(parse_questions("docs/a.md", text).len(), 1, "{text:?}");
        }
    }

    #[test]
    fn code_spans_still_run_within_one_list_item_or_heading() {
        let cases = [
            "- item `[NEEDS OWNER: q\n  | a) y]` end\n",
            "## `[NEEDS OWNER: q | a) y]`\n",
            "-1 `x\n--- [NEEDS OWNER: q | a) y] `\n",
            "#hashtag `x\n[NEEDS OWNER: q | a) y] `\n",
        ];
        for text in cases {
            assert!(parse_questions("docs/a.md", text).is_empty(), "{text:?}");
        }
    }

    #[test]
    fn skips_markers_in_fenced_blocks() {
        let text = format!(
            "```md\n{MARKER}\n```\n\n  ~~~~\n{MARKER}\n~~~\n~~~~\n\nOpen: [NEEDS OWNER: use `x`? | a) yes]\n\n```\n{MARKER}\n"
        );
        let questions = parse_questions("docs/a.md", &text);

        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].question, "use `x`?");
    }

    #[test]
    fn repo_docs_marker_examples_are_not_questions() {
        for (file, text) in [
            ("README.md", include_str!("../README.md")),
            (
                "docs/rust-v1-spec.md",
                include_str!("../docs/rust-v1-spec.md"),
            ),
            (
                "templates/default_prompt.md",
                include_str!("../templates/default_prompt.md"),
            ),
        ] {
            assert_eq!(parse_questions(file, text), Vec::new(), "{file}");
        }
    }

    #[test]
    fn write_back_skips_copy_in_code_span() {
        let text = format!("Format: `{MARKER}`\n\nRetention: {MARKER}.\n");
        let question = &parse_questions("docs/logs.md", &text)[0];

        let updated = write_back(&text, question, "30 days", "2026-11-17");

        assert!(updated.starts_with(&format!("Format: `{MARKER}`\n\nRetention: 30 days.\n")));
    }

    #[test]
    fn write_back_adds_clarifications_section() {
        let text = format!("# Logs\n\nRetention: {MARKER}.\n");
        let question = &parse_questions("docs/logs.md", &text)[0];

        let updated = write_back(&text, question, "30 days", "2026-11-17");

        assert_eq!(
            updated,
            "# Logs\n\nRetention: 30 days.\n\n## Clarifications\n\n### Session 2026-11-17\n\n- Q: How long do we keep logs? → A: 30 days\n"
        );
    }

    #[test]
    fn write_back_appends_to_existing_session_before_next_section() {
        let text = format!(
            "# Logs\n\nRetention: {MARKER}.\n\n## Clarifications\n\n### Session 2026-11-17\n\n- Q: Earlier? → A: yes\n\n## Appendix\n\nText\n"
        );
        let question = &parse_questions("docs/logs.md", &text)[0];

        let updated = write_back(&text, question, "90 days", "2026-11-17");

        assert_eq!(
            updated,
            "# Logs\n\nRetention: 90 days.\n\n## Clarifications\n\n### Session 2026-11-17\n\n- Q: Earlier? → A: yes\n- Q: How long do we keep logs? → A: 90 days\n\n## Appendix\n\nText\n"
        );
    }

    #[test]
    fn write_back_starts_new_session_for_new_date() {
        let text = format!(
            "Retention: {MARKER}.\n\n## Clarifications\n\n### Session 2026-11-01\n\n- Q: Earlier? → A: yes\n"
        );
        let question = &parse_questions("docs/logs.md", &text)[0];

        let updated = write_back(&text, question, "30 days", "2026-11-17");

        assert_eq!(
            updated,
            "Retention: 30 days.\n\n## Clarifications\n\n### Session 2026-11-01\n\n- Q: Earlier? → A: yes\n\n### Session 2026-11-17\n\n- Q: How long do we keep logs? → A: 30 days\n"
        );
    }
}
