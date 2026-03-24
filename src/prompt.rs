use crate::config::ScopeMode;

pub const DEFAULT_PROMPT: &str = include_str!("../templates/default_prompt.md");

#[derive(Debug, Clone)]
pub struct PromptContext {
    pub doc: String,
    pub scope: ScopeMode,
    pub scope_dir: String,
    pub goal: String,
    pub iteration: u32,
    pub max_iters: u32,
    pub last_delta: String,
    pub scope_file_count: usize,
    pub last_changed_files: usize,
}

pub fn build_prompt(template: &str, context: &PromptContext) -> String {
    let scope_instruction = scope_instruction(context.scope, &context.doc, &context.scope_dir);
    let goal_block = if context.goal.is_empty() {
        String::new()
    } else {
        format!("\n\n## Goal\n\n{}", context.goal)
    };

    let mut scope_block = String::new();
    if context.scope == ScopeMode::Sweep {
        scope_block = format!(
            "\n\nThis sweep covers **{} docs** in `{}/`. Prefer the smallest touched-file set that resolves concrete problems.",
            context.scope_file_count, context.scope_dir
        );
        if context.iteration >= 2 && context.last_changed_files > 0 {
            scope_block.push_str(&format!(
                " Previous iteration touched **{} file(s)**. Touch fewer files this pass unless widening the sweep is strictly necessary.",
                context.last_changed_files
            ));
        }
        if context.scope_dir == "docs" {
            scope_block.push_str(
                "\n\nThis is a **full docs-tree sweep**. Treat it as a canonical-consistency pass, not a rewrite of every page:\n\n- Prefer fixing authoritative docs first: `entity-dictionary.md`, `permissions-matrix.md`, `matching-privacy-matrix.md`, `lifecycle-rules.md`, `validation-contract.md`, `product.md`, `configuration.md`, and `database.md`\n- Only edit downstream or leaf docs when a contradiction would otherwise leave that page unimplementable\n- Do NOT propagate wording cleanup across many dependent docs in the same pass\n- In later iterations, shrink the touched set rather than widening it",
            );
        }
    }

    let iteration_block = if context.iteration == 1 {
        format!(
            "\n\n## Iteration Context\n\nThis is the **first pass**. Focus on substantive issues: vague requirements, missing states/transitions, incorrect cross-references, and gaps that would block implementation. Ignore cosmetic concerns.{}",
            scope_block
        )
    } else if context.iteration <= 3 {
        format!(
            "\n\n## Iteration Context\n\nThis is **iteration {} of {}**. Previous iteration changed {} lines.\n\nThe doc has already been reviewed and improved. Only make changes that fix a **concrete problem**: a rule that is ambiguous, a state that is missing, a cross-reference that is wrong, or a value that is unspecified. Do not reword for style. Do not expand existing explanations that are already precise enough to implement from.{}",
            context.iteration, context.max_iters, context.last_delta, scope_block
        )
    } else {
        format!(
            "\n\n## Iteration Context\n\nThis is **iteration {} of {}**. Previous iteration changed {} lines.\n\nThe doc has been refined through multiple passes. **Treat the current text as the best version so far.** Only touch it if you find a clear, objective defect:\n\n- A requirement that a coding agent cannot implement without guessing\n- A factual contradiction with a cross-referenced doc\n- A missing enum value, state, or transition in a defined lifecycle\n\nIf you would make fewer than 5 line changes, the doc is likely converged - respond with `CONVERGED` instead.{}",
            context.iteration, context.max_iters, context.last_delta, scope_block
        )
    };

    template
        .replace("{{DOC}}", &context.doc)
        .replace("{{SCOPE_INSTRUCTION}}", &scope_instruction)
        .replace("{{GOAL}}", &goal_block)
        .replace("{{ITERATION_CONTEXT}}", &iteration_block)
}

fn scope_instruction(scope: ScopeMode, doc: &str, scope_dir: &str) -> String {
    match scope {
        ScopeMode::Strict => "Do NOT touch files other than the target doc.".to_owned(),
        ScopeMode::Ripple => format!(
            "Focus on `{doc}`. You may also edit other docs in `{scope_dir}/` when necessary for consistency - but keep cross-doc changes minimal."
        ),
        ScopeMode::Sweep => format!(
            "Review all docs in `{scope_dir}/`. Prefer the smallest set of file edits needed to resolve concrete issues."
        ),
    }
}
