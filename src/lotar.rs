//! `--ask lotar`: owner questions as lotar question comments, answers read back from them.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::error::{AutospecError, Result};
use crate::owner::OwnerQuestion;

const LOTAR: &str = "lotar";

#[derive(Debug, Deserialize)]
struct CommentList {
    #[serde(default)]
    items: Vec<Comment>,
}

#[derive(Debug, Deserialize)]
struct Comment {
    date: String,
    #[serde(default)]
    text: String,
    question: Option<Question>,
    answer: Option<Answer>,
}

#[derive(Debug, Deserialize)]
struct Question {
    #[serde(default)]
    options: Vec<QuestionOption>,
}

#[derive(Debug, Deserialize)]
struct QuestionOption {
    key: String,
    label: String,
}

#[derive(Debug, Deserialize)]
struct Answer {
    to: String,
    #[serde(default)]
    choice: Vec<String>,
    note: Option<String>,
}

/// The question comments on one lotar task, keyed by question title.
#[derive(Debug, Default)]
pub struct AskedQuestions {
    /// Every question title asked on the task, answered or not.
    asked: Vec<String>,
    /// Answer text per question title.
    answers: HashMap<String, String>,
}

impl AskedQuestions {
    pub fn fetch(repo_root: &Path, task: &str) -> Result<Self> {
        let output = lotar(repo_root, &["--format", "json", "comment", task])?;
        Self::parse(&output)
    }

    fn parse(json: &str) -> Result<Self> {
        let list: CommentList =
            serde_json::from_str(json).map_err(|error| AutospecError::LotarFailed {
                args: "comment".to_owned(),
                detail: format!("unexpected JSON: {error}"),
            })?;

        // Questions in comment order: date, title, options; `open` is false once answered.
        let mut questions: Vec<(&str, String, &Question, bool)> = Vec::new();
        let mut answers = HashMap::new();
        for comment in &list.items {
            if let Some(question) = &comment.question {
                let title = comment
                    .text
                    .split("\n\nOptions:")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                questions.push((comment.date.as_str(), title, question, true));
            } else if let Some(answer) = &comment.answer {
                let Some(entry) = questions
                    .iter_mut()
                    .find(|(date, ..)| *date == answer.to.as_str())
                else {
                    continue;
                };
                let text = structured_answer(entry.2, answer);
                if !text.is_empty() {
                    entry.3 = false;
                    answers.insert(entry.1.clone(), text);
                }
            } else if let Some(text) = plain_answer(&comment.text) {
                // Like `lotar comment --answer` without `--to`: the last question still open.
                if let Some(entry) = questions.iter_mut().rev().find(|entry| entry.3) {
                    entry.3 = false;
                    answers.insert(entry.1.clone(), text);
                }
            }
        }
        let asked = questions.into_iter().map(|(_, title, ..)| title).collect();

        Ok(Self { asked, answers })
    }

    pub fn answer(&self, question: &OwnerQuestion) -> Option<&str> {
        self.answers.get(&question.title()).map(String::as_str)
    }

    pub fn was_asked(&self, question: &OwnerQuestion) -> bool {
        let title = question.title();
        self.asked.contains(&title)
    }
}

/// Chosen option labels, then the note in parentheses; a free-text answer is the note. A note relayed by
/// triage ends with `(answered in <ID>)`, dropped here; a note that is only that suffix counts as none.
fn structured_answer(question: &Question, answer: &Answer) -> String {
    let labels = answer
        .choice
        .iter()
        .map(|key| {
            question
                .options
                .iter()
                .find(|option| &option.key == key)
                .map_or(key.as_str(), |option| option.label.as_str())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let note = answer.note.as_deref().unwrap_or_default();
    let note = strip_relay_suffix(note).unwrap_or(note).trim();
    match (labels.is_empty(), note.is_empty()) {
        (false, false) => format!("{labels} ({note})"),
        (false, true) => labels,
        (true, _) => note.to_owned(),
    }
}

/// A plain comment that answers: it starts with `Answer:` or ends with `(answered in <ID>)`, as the owner's
/// answers relayed by triage do. The answer is the text without both, on one line; `None` when nothing is left.
fn plain_answer(text: &str) -> Option<String> {
    let text = text.trim();
    let prefixed = text.strip_prefix("Answer:");
    let rest = prefixed.unwrap_or(text);
    let relayed = strip_relay_suffix(rest);
    if prefixed.is_none() && relayed.is_none() {
        return None;
    }
    let rest = relayed.unwrap_or(rest);
    let answer = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    (!answer.is_empty()).then_some(answer)
}

/// `text` without a trailing `(answered in <ID>)`; `None` when it has none.
fn strip_relay_suffix(text: &str) -> Option<&str> {
    let text = text.trim_end();
    let open = text.rfind("(answered in ")?;
    let id = text[open + "(answered in ".len()..].strip_suffix(')')?;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(&text[..open])
}

/// Files the question as a question comment on `task`.
pub fn ask(repo_root: &Path, task: &str, question: &OwnerQuestion) -> Result<()> {
    let title = question.title();
    let mut args = vec![
        "comment".to_owned(),
        task.to_owned(),
        "--question".to_owned(),
        title,
    ];
    for option in &question.options {
        args.push("--option".to_owned());
        args.push(format!("{}: {}", option.key, option.label));
    }
    if let Some(recommended) = &question.recommended {
        args.push("--recommend".to_owned());
        args.push(recommended.clone());
    }
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    lotar(repo_root, &args).map(|_| ())
}

fn lotar(repo_root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(LOTAR)
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|source| AutospecError::SpawnFailed {
            command: LOTAR.to_owned(),
            source,
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(AutospecError::LotarFailed {
            args: args.join(" "),
            detail: if stderr.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            } else {
                stderr
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner::parse_questions;

    const COMMENTS: &str = r#"{
      "status": "ok",
      "items": [
        {"date": "2026-11-18T00:14:34.316723+00:00", "author": "autospec",
         "text": "docs/logs.md: How long do we keep logs?\n\nOptions:\n- a: 30 days (recommended)\n- b: 90 days",
         "question": {"options": [{"key": "a", "label": "30 days"}, {"key": "b", "label": "90 days"}], "recommended": "a"}},
        {"date": "2026-11-18T00:14:35.000000+00:00", "author": "autospec",
         "text": "docs/logs.md: Who reads them?\n\nOptions:\n- a: Admins",
         "question": {"options": [{"key": "a", "label": "Admins"}]}},
        {"date": "2026-11-18T00:15:00.000000+00:00", "author": "mallox",
         "text": "Answer: b (90 days)\n\nlegal asks for it",
         "answer": {"to": "2026-11-18T00:14:34.316723+00:00", "choice": ["b"], "note": "legal asks for it"}},
        {"date": "2026-11-18T00:16:00.000000+00:00", "author": "mallox", "text": "plain comment"}
      ]
    }"#;

    #[test]
    fn reads_answers_by_question_title() {
        let asked = AskedQuestions::parse(COMMENTS).unwrap();
        let text = "[NEEDS OWNER: How long do we keep logs? | a) 30 days | b) 90 days]\n\
                    [NEEDS OWNER: Who reads them? | a) Admins]\n\
                    [NEEDS OWNER: New question? | a) x]\n";
        let questions = parse_questions("docs/logs.md", text);

        assert_eq!(
            asked.answer(&questions[0]),
            Some("90 days (legal asks for it)")
        );
        assert!(asked.was_asked(&questions[1]));
        assert_eq!(asked.answer(&questions[1]), None);
        assert!(!asked.was_asked(&questions[2]));
    }

    #[test]
    fn free_text_answer_uses_the_note() {
        let json = r#"{"items": [
          {"date": "d1", "text": "docs/a.md: Q?\n\nOptions:\n- a: x", "question": {"options": [{"key": "a", "label": "x"}]}},
          {"date": "d2", "text": "later", "answer": {"to": "d1", "note": "later"}}
        ]}"#;
        let asked = AskedQuestions::parse(json).unwrap();
        let question = &parse_questions("docs/a.md", "[NEEDS OWNER: Q? | a) x]")[0];

        assert_eq!(asked.answer(question), Some("later"));
    }

    #[test]
    fn relayed_structured_answer_drops_the_relay_suffix_from_the_note() {
        // As on STD-182: the owner picked an option only, triage relayed it with `--answer`.
        let json = r#"{"items": [
          {"date": "d1", "text": "docs/a.md: First?\n\nOptions:\n- wontdo: Close as WontDo", "question": {"options": [{"key": "wontdo", "label": "Close as WontDo"}]}},
          {"date": "d2", "text": "docs/a.md: Second?\n\nOptions:\n- a: 1 year", "question": {"options": [{"key": "a", "label": "1 year"}]}},
          {"date": "d3", "text": "Answer: wontdo (Close as WontDo)\n\n(answered in INBOX-347)", "answer": {"to": "d1", "choice": ["wontdo"], "note": "(answered in INBOX-347)"}},
          {"date": "d4", "text": "Answer: a (1 year)", "answer": {"to": "d2", "choice": ["a"], "note": "legal asks for it\n(answered in INBOX-348)"}}
        ]}"#;
        let asked = AskedQuestions::parse(json).unwrap();
        let questions = parse_questions(
            "docs/a.md",
            "[NEEDS OWNER: First? | wontdo) Close as WontDo]\n[NEEDS OWNER: Second? | a) 1 year]\n",
        );

        assert_eq!(asked.answer(&questions[0]), Some("Close as WontDo"));
        assert_eq!(
            asked.answer(&questions[1]),
            Some("1 year (legal asks for it)")
        );
    }

    #[test]
    fn relayed_plain_answer_answers_the_last_open_question() {
        // As on ASP-5: triage relays the owner's answer as a plain comment.
        let json = r#"{"items": [
          {"date": "d1", "text": "docs/a.md: First?\n\nOptions:\n- a: x", "question": {"options": [{"key": "a", "label": "x"}]}},
          {"date": "d2", "text": "docs/a.md: Second?\n\nOptions:\n- yes: Yes", "question": {"options": [{"key": "yes", "label": "Yes"}]}},
          {"date": "d3", "author": "claude", "text": "Working on it; tests pass."},
          {"date": "d4", "author": "mallox", "text": "Answer: yes-ask (Adopt, plus --ask lotar filing each as a question comment) (answered in INBOX-477)"},
          {"date": "d5", "author": "mallox", "text": "  30 days,\n  not more (answered in INBOX-478)"}
        ]}"#;
        let asked = AskedQuestions::parse(json).unwrap();
        let questions = parse_questions(
            "docs/a.md",
            "[NEEDS OWNER: First? | a) x]\n[NEEDS OWNER: Second? | yes) Yes]\n",
        );

        assert_eq!(
            asked.answer(&questions[1]),
            Some("yes-ask (Adopt, plus --ask lotar filing each as a question comment)")
        );
        assert_eq!(asked.answer(&questions[0]), Some("30 days, not more"));
    }

    #[test]
    fn plain_comments_that_are_not_answers_answer_nothing() {
        let json = r#"{"items": [
          {"date": "d0", "text": "Answer: before any question (answered in INBOX-1)"},
          {"date": "d1", "text": "docs/a.md: Q?\n\nOptions:\n- a: x", "question": {"options": [{"key": "a", "label": "x"}]}},
          {"date": "d2", "text": "Done as answered; see (answered in the thread)"},
          {"date": "d3", "text": "Answer:   (answered in INBOX-2)"}
        ]}"#;
        let asked = AskedQuestions::parse(json).unwrap();
        let question = &parse_questions("docs/a.md", "[NEEDS OWNER: Q? | a) x]")[0];

        assert!(asked.was_asked(question));
        assert_eq!(asked.answer(question), None);
    }

    #[test]
    fn plain_answer_skips_answered_questions_and_a_later_structured_answer_wins() {
        let json = r#"{"items": [
          {"date": "d1", "text": "docs/a.md: First?\n\nOptions:\n- a: x", "question": {"options": [{"key": "a", "label": "x"}]}},
          {"date": "d2", "text": "docs/a.md: Second?\n\nOptions:\n- a: y", "question": {"options": [{"key": "a", "label": "y"}]}},
          {"date": "d3", "text": "Answer: a (y)", "answer": {"to": "d2", "choice": ["a"]}},
          {"date": "d4", "text": "Answer: my own words"},
          {"date": "d5", "text": "Answer: a (x)", "answer": {"to": "d1", "choice": ["a"]}}
        ]}"#;
        let asked = AskedQuestions::parse(json).unwrap();
        let questions = parse_questions(
            "docs/a.md",
            "[NEEDS OWNER: First? | a) x]\n[NEEDS OWNER: Second? | a) y]\n",
        );

        assert_eq!(asked.answer(&questions[1]), Some("y"));
        assert_eq!(asked.answer(&questions[0]), Some("x"));
    }
}
