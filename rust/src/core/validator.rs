#![allow(dead_code)]
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub path: String,
    pub is_valid: bool,
    pub method: &'static str,
    pub details: Option<String>,
}

impl ValidationResult {
    pub fn format_compact(&self) -> String {
        let short = crate::core::protocol::shorten_path(&self.path);
        if self.is_valid {
            format!("{short} ✓ {}", self.method)
        } else {
            let detail = self.details.as_deref().unwrap_or("unknown error");
            format!("{short} ✗ {} — {detail}", self.method)
        }
    }
}

pub fn validate_file(path: &str) -> ValidationResult {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ValidationResult {
                path: path.to_string(),
                is_valid: false,
                method: "fs",
                details: Some(format!("read error: {e}")),
            }
        }
    };

    validate_content(path, &content, ext)
}

pub fn validate_content(path: &str, content: &str, ext: &str) -> ValidationResult {
    match ext {
        "rs" => validate_rust_syntax(path, content),
        "ts" | "tsx" | "js" | "jsx" => validate_js_syntax(path, content),
        "py" => validate_python_syntax(path, content),
        "json" => validate_json(path, content),
        "toml" => validate_toml(path, content),
        _ => ValidationResult {
            path: path.to_string(),
            is_valid: true,
            method: "skip",
            details: Some(format!("no validator for .{ext}")),
        },
    }
}

fn validate_rust_syntax(path: &str, content: &str) -> ValidationResult {
    let checks = [
        check_balanced_braces(content),
        check_balanced_parens(content),
        check_balanced_brackets(content),
        check_no_dangling_strings(content),
    ];

    for (ok, msg) in &checks {
        if !ok {
            return ValidationResult {
                path: path.to_string(),
                is_valid: false,
                method: "syntax",
                details: Some(msg.clone()),
            };
        }
    }

    ValidationResult {
        path: path.to_string(),
        is_valid: true,
        method: "syntax",
        details: None,
    }
}

fn validate_js_syntax(path: &str, content: &str) -> ValidationResult {
    let checks = [
        check_balanced_braces(content),
        check_balanced_parens(content),
        check_balanced_brackets(content),
    ];

    for (ok, msg) in &checks {
        if !ok {
            return ValidationResult {
                path: path.to_string(),
                is_valid: false,
                method: "syntax",
                details: Some(msg.clone()),
            };
        }
    }

    ValidationResult {
        path: path.to_string(),
        is_valid: true,
        method: "syntax",
        details: None,
    }
}

fn validate_python_syntax(path: &str, content: &str) -> ValidationResult {
    let checks = [
        check_balanced_parens(content),
        check_balanced_brackets(content),
    ];

    for (ok, msg) in &checks {
        if !ok {
            return ValidationResult {
                path: path.to_string(),
                is_valid: false,
                method: "syntax",
                details: Some(msg.clone()),
            };
        }
    }

    ValidationResult {
        path: path.to_string(),
        is_valid: true,
        method: "syntax",
        details: None,
    }
}

fn validate_json(path: &str, content: &str) -> ValidationResult {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(_) => ValidationResult {
            path: path.to_string(),
            is_valid: true,
            method: "json-parse",
            details: None,
        },
        Err(e) => ValidationResult {
            path: path.to_string(),
            is_valid: false,
            method: "json-parse",
            details: Some(format!("line {}: {}", e.line(), e)),
        },
    }
}

fn validate_toml(path: &str, content: &str) -> ValidationResult {
    match content.parse::<toml::Value>() {
        Ok(_) => ValidationResult {
            path: path.to_string(),
            is_valid: true,
            method: "toml-parse",
            details: None,
        },
        Err(e) => ValidationResult {
            path: path.to_string(),
            is_valid: false,
            method: "toml-parse",
            details: Some(e.to_string()),
        },
    }
}

fn check_balanced_braces(content: &str) -> (bool, String) {
    check_balanced(content, '{', '}', "braces")
}

fn check_balanced_parens(content: &str) -> (bool, String) {
    check_balanced(content, '(', ')', "parentheses")
}

fn check_balanced_brackets(content: &str) -> (bool, String) {
    check_balanced(content, '[', ']', "brackets")
}

fn check_balanced(content: &str, open: char, close: char, name: &str) -> (bool, String) {
    let mut depth: i64 = 0;
    let mut in_string = false;
    let mut in_line_comment = false;
    let mut prev_char = '\0';

    for ch in content.chars() {
        if ch == '\n' {
            in_line_comment = false;
        }
        if in_line_comment {
            prev_char = ch;
            continue;
        }
        if ch == '/' && prev_char == '/' {
            in_line_comment = true;
            prev_char = ch;
            continue;
        }

        if ch == '"' && prev_char != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
            }
            if depth < 0 {
                return (false, format!("unmatched closing {name}"));
            }
        }
        prev_char = ch;
    }

    if depth == 0 {
        (true, String::new())
    } else {
        (false, format!("unbalanced {name}: depth {depth}"))
    }
}

fn check_no_dangling_strings(content: &str) -> (bool, String) {
    let mut in_string = false;
    let mut prev_char = '\0';
    let mut line_num = 1;

    for ch in content.chars() {
        if ch == '\n' {
            if in_string {
                // Multi-line raw strings are ok in Rust (r"...")
                // Regular strings shouldn't span lines without continuation
            }
            line_num += 1;
        }
        if ch == '"' && prev_char != '\\' {
            in_string = !in_string;
        }
        prev_char = ch;
    }

    if in_string {
        (
            false,
            format!("unclosed string literal near line {line_num}"),
        )
    } else {
        (true, String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_rust() {
        let code = "fn main() {\n    let x = vec![1, 2, 3];\n    println!(\"{:?}\", x);\n}\n";
        let result = validate_content("test.rs", code, "rs");
        assert!(result.is_valid);
    }

    #[test]
    fn test_invalid_rust_unbalanced() {
        let code = "fn main() {\n    let x = 1;\n";
        let result = validate_content("test.rs", code, "rs");
        assert!(!result.is_valid);
        assert!(result.details.unwrap().contains("brace"));
    }

    #[test]
    fn test_valid_json() {
        let json = r#"{"key": "value", "num": 42}"#;
        let result = validate_content("test.json", json, "json");
        assert!(result.is_valid);
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"{"key": "value",}"#;
        let result = validate_content("test.json", json, "json");
        assert!(!result.is_valid);
    }

    #[test]
    fn test_valid_toml() {
        let toml = "[package]\nname = \"test\"\nversion = \"1.0.0\"\n";
        let result = validate_content("test.toml", toml, "toml");
        assert!(result.is_valid);
    }

    #[test]
    fn test_format_compact_valid() {
        let r = ValidationResult {
            path: "/src/main.rs".to_string(),
            is_valid: true,
            method: "syntax",
            details: None,
        };
        let formatted = r.format_compact();
        assert!(formatted.contains("✓"));
    }

    #[test]
    fn test_format_compact_invalid() {
        let r = ValidationResult {
            path: "/src/main.rs".to_string(),
            is_valid: false,
            method: "syntax",
            details: Some("unbalanced braces".to_string()),
        };
        let formatted = r.format_compact();
        assert!(formatted.contains("✗"));
        assert!(formatted.contains("unbalanced"));
    }

    #[test]
    fn test_unknown_extension_skips() {
        let result = validate_content("readme.md", "# Title", "md");
        assert!(result.is_valid);
        assert_eq!(result.method, "skip");
    }

    #[test]
    fn test_balanced_with_strings() {
        let code = r#"let s = "{ not a brace }";"#;
        let (ok, _) = check_balanced_braces(code);
        assert!(ok);
    }

    #[test]
    fn test_balanced_with_comments() {
        let code = "fn main() {\n    // { this is a comment\n}\n";
        let (ok, _) = check_balanced_braces(code);
        assert!(ok);
    }
}
