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

        let mut by_date = HashMap::new();
        let mut asked = Vec::new();
        for comment in &list.items {
            if let Some(question) = &comment.question {
                let title = comment
                    .text
                    .split("\n\nOptions:")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                by_date.insert(comment.date.as_str(), (title.clone(), question));
                asked.push(title);
            }
        }

        let mut answers = HashMap::new();
        for comment in &list.items {
            let Some(answer) = &comment.answer else {
                continue;
            };
            let Some((title, question)) = by_date.get(answer.to.as_str()) else {
                continue;
            };
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
            let note = answer.note.as_deref().map(str::trim).unwrap_or_default();
            let text = match (labels.is_empty(), note.is_empty()) {
                (false, false) => format!("{labels} ({note})"),
                (false, true) => labels,
                (true, _) => note.to_owned(),
            };
            if !text.is_empty() {
                answers.insert(title.clone(), text);
            }
        }

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
}
