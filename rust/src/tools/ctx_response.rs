use crate::core::tokens::count_tokens;
use crate::tools::CrpMode;

pub fn handle(response: &str, crp_mode: CrpMode) -> String {
    let original_tokens = count_tokens(response);

    if original_tokens <= 100 {
        return response.to_string();
    }

    let compressed = if crp_mode.is_tdd() {
        compress_tdd(response)
    } else {
        compress_standard(response)
    };

    let compressed_tokens = count_tokens(&compressed);
    let savings = original_tokens.saturating_sub(compressed_tokens);
    let pct = if original_tokens > 0 {
        (savings as f64 / original_tokens as f64 * 100.0) as u32
    } else {
        0
    };

    if savings < 20 {
        return response.to_string();
    }

    format!(
        "{compressed}\n[response compressed: {original_tokens}→{compressed_tokens} tok, -{pct}%]"
    )
}

fn compress_standard(text: &str) -> String {
    let mut result = Vec::new();
    let mut prev_empty = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !prev_empty {
                result.push(String::new());
                prev_empty = true;
            }
            continue;
        }
        prev_empty = false;

        if is_filler_line(trimmed) {
            continue;
        }

        result.push(line.to_string());
    }

    result.join("\n")
}

fn compress_tdd(text: &str) -> String {
    let mut result = Vec::new();
    let mut prev_empty = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if !prev_empty {
                prev_empty = true;
            }
            continue;
        }
        prev_empty = false;

        if is_filler_line(trimmed) {
            continue;
        }

        let compressed = apply_tdd_shortcuts(trimmed);
        result.push(compressed);
    }

    result.join("\n")
}

fn is_filler_line(line: &str) -> bool {
    let l = line.to_lowercase();

    // Preserve lines with genuine information signals
    if l.starts_with("note:")
        || l.starts_with("hint:")
        || l.starts_with("warning:")
        || l.starts_with("error:")
        || l.starts_with("however,")
        || l.starts_with("but ")
        || l.starts_with("caution:")
        || l.starts_with("important:")
    {
        return false;
    }

    // H=0 patterns: carry zero task-relevant information
    let prefix_fillers = [
        // Narration / preamble
        "here's what i",
        "here is what i",
        "let me explain",
        "let me walk you",
        "let me break",
        "i'll now",
        "i will now",
        "i'm going to",
        "first, let me",
        "allow me to",
        // Hedging
        "i think",
        "i believe",
        "i would say",
        "it seems like",
        "it looks like",
        "it appears that",
        // Meta-commentary
        "that's a great question",
        "that's an interesting",
        "good question",
        "great question",
        "sure thing",
        "sure,",
        "of course,",
        "absolutely,",
        // Transitions (zero-info)
        "now, let's",
        "now let's",
        "next, i'll",
        "moving on",
        "going forward",
        "with that said",
        "with that in mind",
        "having said that",
        "that being said",
        // Closings
        "hope this helps",
        "i hope this",
        "let me know if",
        "feel free to",
        "don't hesitate",
        "happy to help",
        // Filler connectives
        "as you can see",
        "as we can see",
        "this is because",
        "the reason is",
        "in this case",
        "in other words",
        "to summarize",
        "to sum up",
        "basically,",
        "essentially,",
        "it's worth noting",
        "it should be noted",
        "as mentioned",
        "as i mentioned",
        // Acknowledgments
        "understood.",
        "got it.",
        "i understand.",
        "i see.",
        "right,",
        "okay,",
        "ok,",
    ];

    prefix_fillers.iter().any(|f| l.starts_with(f))
}

fn apply_tdd_shortcuts(line: &str) -> String {
    let mut result = line.to_string();

    let replacements = [
        // Structural
        ("function", "fn"),
        ("configuration", "cfg"),
        ("implementation", "impl"),
        ("dependencies", "deps"),
        ("dependency", "dep"),
        ("request", "req"),
        ("response", "res"),
        ("context", "ctx"),
        ("parameter", "param"),
        ("argument", "arg"),
        ("variable", "val"),
        ("directory", "dir"),
        ("repository", "repo"),
        ("application", "app"),
        ("environment", "env"),
        ("description", "desc"),
        ("information", "info"),
        // Symbols (1 token each, replaces 5-10 tokens of prose)
        ("returns ", "→ "),
        ("therefore", "∴"),
        ("approximately", "≈"),
        ("successfully", "✓"),
        ("completed", "✓"),
        ("failed", "✗"),
        ("warning", "⚠"),
        // Operators
        (" is not ", " ≠ "),
        (" does not ", " ≠ "),
        (" equals ", " = "),
        (" and ", " & "),
        ("error", "err"),
        ("module", "mod"),
        ("package", "pkg"),
        ("initialize", "init"),
    ];

    for (from, to) in &replacements {
        result = result.replace(from, to);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filler_detection_original() {
        assert!(is_filler_line("Here's what I found"));
        assert!(is_filler_line("Let me explain how this works"));
        assert!(!is_filler_line("fn main() {}"));
        assert!(!is_filler_line("Note: important detail"));
    }

    #[test]
    fn test_filler_hedging_patterns() {
        assert!(is_filler_line("I think the issue is here"));
        assert!(is_filler_line("I believe this is correct"));
        assert!(is_filler_line("It seems like the problem is"));
        assert!(is_filler_line("It looks like we need to"));
        assert!(is_filler_line("It appears that something broke"));
    }

    #[test]
    fn test_filler_meta_commentary() {
        assert!(is_filler_line("That's a great question!"));
        assert!(is_filler_line("Good question, let me check"));
        assert!(is_filler_line("Sure thing, I'll do that"));
        assert!(is_filler_line("Of course, here's the code"));
        assert!(is_filler_line("Absolutely, that makes sense"));
    }

    #[test]
    fn test_filler_closings() {
        assert!(is_filler_line("Hope this helps!"));
        assert!(is_filler_line("Let me know if you need more"));
        assert!(is_filler_line("Feel free to ask questions"));
        assert!(is_filler_line("Don't hesitate to reach out"));
        assert!(is_filler_line("Happy to help with anything"));
    }

    #[test]
    fn test_filler_transitions() {
        assert!(is_filler_line("Now, let's move on"));
        assert!(is_filler_line("Moving on to the next part"));
        assert!(is_filler_line("Going forward, we should"));
        assert!(is_filler_line("With that said, here's what"));
        assert!(is_filler_line("Having said that, let's"));
    }

    #[test]
    fn test_filler_acknowledgments() {
        assert!(is_filler_line("Understood."));
        assert!(is_filler_line("Got it."));
        assert!(is_filler_line("I understand."));
        assert!(is_filler_line("I see."));
    }

    #[test]
    fn test_filler_false_positive_protection() {
        assert!(!is_filler_line("Note: this is critical"));
        assert!(!is_filler_line("Warning: deprecated API"));
        assert!(!is_filler_line("Error: connection refused"));
        assert!(!is_filler_line("However, the edge case fails"));
        assert!(!is_filler_line("But the second argument is wrong"));
        assert!(!is_filler_line("Important: do not skip this step"));
        assert!(!is_filler_line("Caution: this deletes all data"));
        assert!(!is_filler_line("Hint: use --force flag"));
        assert!(!is_filler_line("fn validate_token()"));
        assert!(!is_filler_line("  let result = parse(input);"));
        assert!(!is_filler_line("The token is expired after 24h"));
    }

    #[test]
    fn test_tdd_shortcuts() {
        let result = apply_tdd_shortcuts("the function returns successfully");
        assert!(result.contains("fn"));
        assert!(result.contains("→"));
        assert!(result.contains("✓"));
    }

    #[test]
    fn test_tdd_shortcuts_extended() {
        let result = apply_tdd_shortcuts("the application environment failed");
        assert!(result.contains("app"));
        assert!(result.contains("env"));
        assert!(result.contains("✗"));
    }

    #[test]
    fn test_compress_integration() {
        let response = "Let me explain how this works.\n\
            I think this is correct.\n\
            Hope this helps!\n\
            \n\
            The function returns an error when the token is expired.\n\
            Note: always check the expiry first.";

        let compressed = compress_standard(response);
        assert!(!compressed.contains("Let me explain"));
        assert!(!compressed.contains("I think"));
        assert!(!compressed.contains("Hope this helps"));
        assert!(compressed.contains("error when the token"));
        assert!(compressed.contains("Note:"));
    }
}
