use std::path::Path;

use crate::core::compressor;
use crate::core::config;
use crate::core::deps as dep_extract;
use crate::core::entropy;
use crate::core::patterns::deps_cmd;
use crate::core::protocol;
use crate::core::signatures;
use crate::core::stats;
use crate::core::tokens::count_tokens;
use crate::hooks::to_bash_compatible_path;

pub fn cmd_read(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: lean-ctx read <file> [--mode full|map|signatures|aggressive|entropy]");
        std::process::exit(1);
    }

    let path = &args[0];
    let mode = args
        .iter()
        .position(|a| a == "--mode" || a == "-m")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("full");

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let short = protocol::shorten_path(path);
    let line_count = content.lines().count();
    let original_tokens = count_tokens(&content);

    match mode {
        "map" => {
            let sigs = signatures::extract_signatures(&content, ext);
            let dep_info = dep_extract::extract_deps(&content, ext);

            println!("{short} [{line_count}L]");
            if !dep_info.imports.is_empty() {
                println!("  deps: {}", dep_info.imports.join(", "));
            }
            if !dep_info.exports.is_empty() {
                println!("  exports: {}", dep_info.exports.join(", "));
            }
            let key_sigs: Vec<_> = sigs
                .iter()
                .filter(|s| s.is_exported || s.indent == 0)
                .collect();
            if !key_sigs.is_empty() {
                println!("  API:");
                for sig in &key_sigs {
                    println!("    {}", sig.to_compact());
                }
            }
            let sent = count_tokens(&short.to_string());
            print_savings(original_tokens, sent);
        }
        "signatures" => {
            let sigs = signatures::extract_signatures(&content, ext);
            println!("{short} [{line_count}L]");
            for sig in &sigs {
                println!("{}", sig.to_compact());
            }
            let sent = count_tokens(&short.to_string());
            print_savings(original_tokens, sent);
        }
        "aggressive" => {
            let compressed = compressor::aggressive_compress(&content, Some(ext));
            println!("{short} [{line_count}L]");
            println!("{compressed}");
            let sent = count_tokens(&compressed);
            print_savings(original_tokens, sent);
        }
        "entropy" => {
            let result = entropy::entropy_compress(&content);
            let avg_h = entropy::analyze_entropy(&content).avg_entropy;
            println!("{short} [{line_count}L] (H̄={avg_h:.1})");
            for tech in &result.techniques {
                println!("{tech}");
            }
            println!("{}", result.output);
            let sent = count_tokens(&result.output);
            print_savings(original_tokens, sent);
        }
        _ => {
            println!("{short} [{line_count}L]");
            println!("{content}");
        }
    }
}

pub fn cmd_diff(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Usage: lean-ctx diff <file1> <file2>");
        std::process::exit(1);
    }

    let content1 = match std::fs::read_to_string(&args[0]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {e}", args[0]);
            std::process::exit(1);
        }
    };

    let content2 = match std::fs::read_to_string(&args[1]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {e}", args[1]);
            std::process::exit(1);
        }
    };

    let diff = compressor::diff_content(&content1, &content2);
    let original = count_tokens(&content1) + count_tokens(&content2);
    let sent = count_tokens(&diff);

    println!(
        "diff {} {}",
        protocol::shorten_path(&args[0]),
        protocol::shorten_path(&args[1])
    );
    println!("{diff}");
    print_savings(original, sent);
}

pub fn cmd_grep(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: lean-ctx grep <pattern> [path]");
        std::process::exit(1);
    }

    let pattern = &args[0];
    let path = args.get(1).map(|s| s.as_str()).unwrap_or(".");

    let command = if cfg!(windows) {
        format!(
            "findstr /S /N /R \"{}\" {}\\*",
            pattern,
            path.replace('/', "\\")
        )
    } else {
        format!("grep -rn '{}' {}", pattern.replace('\'', "'\\''"), path)
    };
    let code = crate::shell::exec(&command);
    std::process::exit(code);
}

pub fn cmd_find(args: &[String]) {
    if args.is_empty() {
        eprintln!("Usage: lean-ctx find <pattern> [path]");
        std::process::exit(1);
    }

    let pattern = &args[0];
    let path = args.get(1).map(|s| s.as_str()).unwrap_or(".");
    let command = if cfg!(windows) {
        format!("dir /S /B {}\\{}", path.replace('/', "\\"), pattern)
    } else {
        format!("find {path} -name \"{pattern}\" -not -path '*/node_modules/*' -not -path '*/.git/*' -not -path '*/target/*'")
    };
    let code = crate::shell::exec(&command);
    std::process::exit(code);
}

pub fn cmd_ls(args: &[String]) {
    let path = args.first().map(|s| s.as_str()).unwrap_or(".");
    let command = if cfg!(windows) {
        format!("dir {}", path.replace('/', "\\"))
    } else {
        format!("ls -la {path}")
    };
    let code = crate::shell::exec(&command);
    std::process::exit(code);
}

pub fn cmd_deps(args: &[String]) {
    let path = args.first().map(|s| s.as_str()).unwrap_or(".");

    match deps_cmd::detect_and_compress(path) {
        Some(result) => println!("{result}"),
        None => {
            eprintln!("No dependency file found in {path}");
            std::process::exit(1);
        }
    }
}

pub fn cmd_discover(_args: &[String]) {
    let history = load_shell_history();
    if history.is_empty() {
        println!("No shell history found.");
        return;
    }

    let compressible_commands = [
        "git ",
        "npm ",
        "yarn ",
        "pnpm ",
        "cargo ",
        "docker ",
        "kubectl ",
        "gh ",
        "pip ",
        "pip3 ",
        "eslint",
        "prettier",
        "ruff ",
        "go ",
        "golangci-lint",
        "playwright",
        "cypress",
        "next ",
        "vite ",
        "tsc",
        "curl ",
        "wget ",
        "grep ",
        "rg ",
        "find ",
        "env",
        "ls ",
    ];

    let mut missed: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut total_compressible = 0u32;
    let mut via_lean_ctx = 0u32;

    for line in &history {
        let cmd = line.trim().to_lowercase();
        if cmd.starts_with("lean-ctx") {
            via_lean_ctx += 1;
            continue;
        }
        for pattern in &compressible_commands {
            if cmd.starts_with(pattern) {
                total_compressible += 1;
                let key = cmd.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
                *missed.entry(key).or_insert(0) += 1;
                break;
            }
        }
    }

    if missed.is_empty() {
        println!("All compressible commands are already using lean-ctx!");
        return;
    }

    let mut sorted: Vec<(String, u32)> = missed.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    println!(
        "Found {} compressible commands not using lean-ctx:\n",
        total_compressible
    );
    for (cmd, count) in sorted.iter().take(15) {
        let est_savings = count * 150;
        println!("  {cmd:<30} (used {count}x, ~{est_savings} tokens saveable)");
    }
    if sorted.len() > 15 {
        println!("  ... +{} more command types", sorted.len() - 15);
    }

    let total_est = total_compressible * 150;
    println!("\nEstimated missed savings: ~{total_est} tokens");
    println!("Already using lean-ctx: {via_lean_ctx} commands");
    println!("\nRun 'lean-ctx init --global' to enable compression for all commands.");
}

pub fn cmd_session() {
    let history = load_shell_history();
    let gain = stats::load_stats();

    let compressible_commands = [
        "git ",
        "npm ",
        "yarn ",
        "pnpm ",
        "cargo ",
        "docker ",
        "kubectl ",
        "gh ",
        "pip ",
        "pip3 ",
        "eslint",
        "prettier",
        "ruff ",
        "go ",
        "golangci-lint",
        "curl ",
        "wget ",
        "grep ",
        "rg ",
        "find ",
        "ls ",
    ];

    let mut total = 0u32;
    let mut via_hook = 0u32;

    for line in &history {
        let cmd = line.trim().to_lowercase();
        if cmd.starts_with("lean-ctx") {
            via_hook += 1;
            total += 1;
        } else {
            for p in &compressible_commands {
                if cmd.starts_with(p) {
                    total += 1;
                    break;
                }
            }
        }
    }

    let pct = if total > 0 {
        (via_hook as f64 / total as f64 * 100.0).round() as u32
    } else {
        0
    };

    println!("lean-ctx session statistics\n");
    println!(
        "Adoption:    {}% ({}/{} compressible commands)",
        pct, via_hook, total
    );
    println!("Saved:       {} tokens total", gain.total_saved);
    println!("Calls:       {} compressed", gain.total_calls);

    if total > via_hook {
        let missed = total - via_hook;
        let est = missed * 150;
        println!(
            "Missed:      {} commands (~{} tokens saveable)",
            missed, est
        );
    }

    println!("\nRun 'lean-ctx discover' for details on missed commands.");
}

pub fn cmd_wrapped(args: &[String]) {
    let period = if args.iter().any(|a| a == "--month") {
        "month"
    } else if args.iter().any(|a| a == "--all") {
        "all"
    } else {
        "week"
    };

    let report = crate::core::wrapped::WrappedReport::generate(period);
    println!("{}", report.format_ascii());
}

pub fn cmd_sessions(args: &[String]) {
    use crate::core::session::SessionState;

    let action = args.first().map(|s| s.as_str()).unwrap_or("list");

    match action {
        "list" | "ls" => {
            let sessions = SessionState::list_sessions();
            if sessions.is_empty() {
                println!("No sessions found.");
                return;
            }
            println!("Sessions ({}):\n", sessions.len());
            for s in sessions.iter().take(20) {
                let task = s.task.as_deref().unwrap_or("(no task)");
                let task_short: String = task.chars().take(50).collect();
                let date = s.updated_at.format("%Y-%m-%d %H:%M");
                println!(
                    "  {} | v{:3} | {:5} calls | {:>8} tok | {} | {}",
                    s.id,
                    s.version,
                    s.tool_calls,
                    format_tokens_cli(s.tokens_saved),
                    date,
                    task_short
                );
            }
            if sessions.len() > 20 {
                println!("  ... +{} more", sessions.len() - 20);
            }
        }
        "show" => {
            let id = args.get(1);
            let session = if let Some(id) = id {
                SessionState::load_by_id(id)
            } else {
                SessionState::load_latest()
            };
            match session {
                Some(s) => println!("{}", s.format_compact()),
                None => println!("Session not found."),
            }
        }
        "cleanup" => {
            let days = args.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(7);
            let removed = SessionState::cleanup_old_sessions(days);
            println!("Cleaned up {removed} session(s) older than {days} days.");
        }
        _ => {
            eprintln!("Usage: lean-ctx sessions [list|show [id]|cleanup [days]]");
            std::process::exit(1);
        }
    }
}

pub fn cmd_benchmark(args: &[String]) {
    use crate::core::benchmark;

    let action = args.first().map(|s| s.as_str()).unwrap_or("run");

    match action {
        "run" => {
            let path = args.get(1).map(|s| s.as_str()).unwrap_or(".");
            let is_json = args.iter().any(|a| a == "--json");

            let result = benchmark::run_project_benchmark(path);
            if is_json {
                println!("{}", benchmark::format_json(&result));
            } else {
                println!("{}", benchmark::format_terminal(&result));
            }
        }
        "report" => {
            let path = args.get(1).map(|s| s.as_str()).unwrap_or(".");
            let result = benchmark::run_project_benchmark(path);
            println!("{}", benchmark::format_markdown(&result));
        }
        _ => {
            if std::path::Path::new(action).exists() {
                let result = benchmark::run_project_benchmark(action);
                println!("{}", benchmark::format_terminal(&result));
            } else {
                eprintln!("Usage: lean-ctx benchmark run [path] [--json]");
                eprintln!("       lean-ctx benchmark report [path]");
                std::process::exit(1);
            }
        }
    }
}

fn format_tokens_cli(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens}")
    }
}

pub fn cmd_config(args: &[String]) {
    let cfg = config::Config::load();

    if args.is_empty() {
        println!("{}", cfg.show());
        return;
    }

    match args[0].as_str() {
        "init" | "create" => {
            let default = config::Config::default();
            match default.save() {
                Ok(()) => {
                    let path = config::Config::path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "~/.lean-ctx/config.toml".to_string());
                    println!("Created default config at {path}");
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        "set" => {
            if args.len() < 3 {
                eprintln!("Usage: lean-ctx config set <key> <value>");
                std::process::exit(1);
            }
            let mut cfg = cfg;
            let key = &args[1];
            let val = &args[2];
            match key.as_str() {
                "ultra_compact" => cfg.ultra_compact = val == "true",
                "tee_on_error" => cfg.tee_on_error = val == "true",
                "checkpoint_interval" => {
                    cfg.checkpoint_interval = val.parse().unwrap_or(15);
                }
                _ => {
                    eprintln!("Unknown config key: {key}");
                    std::process::exit(1);
                }
            }
            match cfg.save() {
                Ok(()) => println!("Updated {key} = {val}"),
                Err(e) => eprintln!("Error saving config: {e}"),
            }
        }
        _ => {
            eprintln!("Usage: lean-ctx config [init|set <key> <value>]");
            std::process::exit(1);
        }
    }
}

pub fn cmd_tee(args: &[String]) {
    let tee_dir = match dirs::home_dir() {
        Some(h) => h.join(".lean-ctx").join("tee"),
        None => {
            eprintln!("Cannot determine home directory");
            std::process::exit(1);
        }
    };

    let action = args.first().map(|s| s.as_str()).unwrap_or("list");
    match action {
        "list" | "ls" => {
            if !tee_dir.exists() {
                println!("No tee logs found (~/.lean-ctx/tee/ does not exist)");
                return;
            }
            let mut entries: Vec<_> = std::fs::read_dir(&tee_dir)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                })
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("log"))
                .collect();
            entries.sort_by_key(|e| e.file_name());

            if entries.is_empty() {
                println!("No tee logs found.");
                return;
            }

            println!("Tee logs ({}):\n", entries.len());
            for entry in &entries {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let name = entry.file_name();
                let size_str = if size > 1024 {
                    format!("{}K", size / 1024)
                } else {
                    format!("{}B", size)
                };
                println!("  {:<60} {}", name.to_string_lossy(), size_str);
            }
            println!("\nUse 'lean-ctx tee clear' to delete all logs.");
        }
        "clear" | "purge" => {
            if !tee_dir.exists() {
                println!("No tee logs to clear.");
                return;
            }
            let mut count = 0u32;
            if let Ok(entries) = std::fs::read_dir(&tee_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|x| x.to_str()) == Some("log")
                        && std::fs::remove_file(entry.path()).is_ok()
                    {
                        count += 1;
                    }
                }
            }
            println!("Cleared {count} tee log(s) from {}", tee_dir.display());
        }
        "show" => {
            let filename = args.get(1);
            if filename.is_none() {
                eprintln!("Usage: lean-ctx tee show <filename>");
                std::process::exit(1);
            }
            let path = tee_dir.join(filename.unwrap());
            match std::fs::read_to_string(&path) {
                Ok(content) => print!("{content}"),
                Err(e) => {
                    eprintln!("Error reading {}: {e}", path.display());
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Usage: lean-ctx tee [list|clear|show <file>]");
            std::process::exit(1);
        }
    }
}

pub fn cmd_init(args: &[String]) {
    let global = args.iter().any(|a| a == "--global" || a == "-g");
    let with_graph = args.iter().any(|a| a == "--with-graph");

    let agents: Vec<&str> = args
        .windows(2)
        .filter(|w| w[0] == "--agent")
        .map(|w| w[1].as_str())
        .collect();

    if with_graph {
        init_graph(args);
        return;
    }

    if !agents.is_empty() {
        for agent_name in &agents {
            crate::hooks::install_agent_hook(agent_name, global);
        }
        println!("\nRun 'lean-ctx gain' after using some commands to see your savings.");
        return;
    }

    let shell_name = std::env::var("SHELL").unwrap_or_default();
    let is_zsh = shell_name.contains("zsh");
    let is_fish = shell_name.contains("fish");
    let is_powershell = cfg!(windows) && shell_name.is_empty();

    let binary = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "lean-ctx".to_string());

    if is_powershell {
        init_powershell(&binary);
    } else {
        let bash_binary = to_bash_compatible_path(&binary);
        if is_fish {
            init_fish(&bash_binary);
        } else {
            init_posix(is_zsh, &bash_binary);
        }
    }

    let lean_dir = dirs::home_dir().map(|h| h.join(".lean-ctx"));
    if let Some(dir) = lean_dir {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
            println!("Created {}", dir.display());
        }
    }

    if global && !is_powershell {
        let rc = if is_fish {
            "config.fish"
        } else if is_zsh {
            ".zshrc"
        } else {
            ".bashrc"
        };
        println!("\nRestart your shell or run: source ~/{rc}");
    } else if global && is_powershell {
        println!("\nRestart PowerShell or run: . $PROFILE");
    }

    println!("\nlean-ctx init complete. (23 aliases installed)");
    println!("Binary: {binary}");
    println!("\nFor AI tool integration, use: lean-ctx init --agent <tool>");
    println!("  Supported: claude, cursor, gemini, codex, windsurf, cline, copilot");
    println!("\nRun 'lean-ctx gain' after using some commands to see your savings.");
    println!("Run 'lean-ctx discover' to find missed savings in your shell history.");
}

fn init_powershell(binary: &str) {
    let profile_dir = dirs::home_dir().map(|h| h.join("Documents").join("PowerShell"));
    let profile_path = match profile_dir {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            dir.join("Microsoft.PowerShell_profile.ps1")
        }
        None => {
            eprintln!("Could not resolve PowerShell profile directory");
            return;
        }
    };

    let binary_escaped = binary.replace('\\', "\\\\");
    let functions = format!(
        r#"
# lean-ctx shell hook — transparent CLI compression (90+ patterns)
if (-not $env:LEAN_CTX_ACTIVE) {{
  $LeanCtxBin = "{binary_escaped}"
  function git {{ & $LeanCtxBin -c "git $($args -join ' ')" }}
  function npm {{ & $LeanCtxBin -c "npm $($args -join ' ')" }}
  function pnpm {{ & $LeanCtxBin -c "pnpm $($args -join ' ')" }}
  function yarn {{ & $LeanCtxBin -c "yarn $($args -join ' ')" }}
  function cargo {{ & $LeanCtxBin -c "cargo $($args -join ' ')" }}
  function docker {{ & $LeanCtxBin -c "docker $($args -join ' ')" }}
  function kubectl {{ & $LeanCtxBin -c "kubectl $($args -join ' ')" }}
  function gh {{ & $LeanCtxBin -c "gh $($args -join ' ')" }}
  function pip {{ & $LeanCtxBin -c "pip $($args -join ' ')" }}
  function pip3 {{ & $LeanCtxBin -c "pip3 $($args -join ' ')" }}
  function ruff {{ & $LeanCtxBin -c "ruff $($args -join ' ')" }}
  function go {{ & $LeanCtxBin -c "go $($args -join ' ')" }}
  function eslint {{ & $LeanCtxBin -c "eslint $($args -join ' ')" }}
  function prettier {{ & $LeanCtxBin -c "prettier $($args -join ' ')" }}
  function tsc {{ & $LeanCtxBin -c "tsc $($args -join ' ')" }}
  function curl {{ & $LeanCtxBin -c "curl $($args -join ' ')" }}
  function wget {{ & $LeanCtxBin -c "wget $($args -join ' ')" }}
}}
"#
    );

    if let Ok(existing) = std::fs::read_to_string(&profile_path) {
        if existing.contains("lean-ctx shell hook") {
            let cleaned = remove_lean_ctx_block_ps(&existing);
            match std::fs::write(&profile_path, format!("{cleaned}{functions}")) {
                Ok(()) => {
                    println!("Updated lean-ctx functions in {}", profile_path.display());
                    println!("  Binary: {binary}");
                    return;
                }
                Err(e) => {
                    eprintln!("Error updating {}: {e}", profile_path.display());
                    return;
                }
            }
        }
    }

    match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&profile_path)
    {
        Ok(mut f) => {
            use std::io::Write;
            let _ = f.write_all(functions.as_bytes());
            println!("Added lean-ctx functions to {}", profile_path.display());
            println!("  Binary: {binary}");
        }
        Err(e) => eprintln!("Error writing {}: {e}", profile_path.display()),
    }
}

fn remove_lean_ctx_block_ps(content: &str) -> String {
    let mut result = String::new();
    let mut in_block = false;
    let mut brace_depth = 0i32;

    for line in content.lines() {
        if line.contains("lean-ctx shell hook") {
            in_block = true;
            continue;
        }
        if in_block {
            brace_depth += line.matches('{').count() as i32;
            brace_depth -= line.matches('}').count() as i32;
            if brace_depth <= 0 && (line.trim() == "}" || line.trim().is_empty()) {
                if line.trim() == "}" {
                    in_block = false;
                    brace_depth = 0;
                }
                continue;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

fn init_fish(binary: &str) {
    let config = dirs::home_dir()
        .map(|h| h.join(".config/fish/config.fish"))
        .unwrap_or_default();

    let aliases = format!(
        "\n# lean-ctx shell hook — transparent CLI compression (90+ patterns)\n\
        if not set -q LEAN_CTX_ACTIVE\n\
        \talias git '{binary} -c git'\n\
        \talias npm '{binary} -c npm'\n\
        \talias pnpm '{binary} -c pnpm'\n\
        \talias yarn '{binary} -c yarn'\n\
        \talias cargo '{binary} -c cargo'\n\
        \talias docker '{binary} -c docker'\n\
        \talias docker-compose '{binary} -c docker-compose'\n\
        \talias kubectl '{binary} -c kubectl'\n\
        \talias k '{binary} -c kubectl'\n\
        \talias gh '{binary} -c gh'\n\
        \talias pip '{binary} -c pip'\n\
        \talias pip3 '{binary} -c pip3'\n\
        \talias ruff '{binary} -c ruff'\n\
        \talias go '{binary} -c go'\n\
        \talias golangci-lint '{binary} -c golangci-lint'\n\
        \talias eslint '{binary} -c eslint'\n\
        \talias prettier '{binary} -c prettier'\n\
        \talias tsc '{binary} -c tsc'\n\
        \talias ls '{binary} -c ls'\n\
        \talias find '{binary} -c find'\n\
        \talias grep '{binary} -c grep'\n\
        \talias curl '{binary} -c curl'\n\
        \talias wget '{binary} -c wget'\n\
        end\n"
    );

    if let Ok(existing) = std::fs::read_to_string(&config) {
        if existing.contains("lean-ctx shell hook") {
            let cleaned = remove_lean_ctx_block(&existing);
            match std::fs::write(&config, format!("{cleaned}{aliases}")) {
                Ok(()) => {
                    println!("Updated lean-ctx aliases in {}", config.display());
                    println!("  Binary: {binary}");
                    return;
                }
                Err(e) => {
                    eprintln!("Error updating {}: {e}", config.display());
                    return;
                }
            }
        }
    }

    match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&config)
    {
        Ok(mut f) => {
            use std::io::Write;
            let _ = f.write_all(aliases.as_bytes());
            println!("Added lean-ctx aliases to {}", config.display());
            println!("  Binary: {binary}");
        }
        Err(e) => eprintln!("Error writing {}: {e}", config.display()),
    }
}

fn init_posix(is_zsh: bool, binary: &str) {
    let rc_file = if is_zsh {
        dirs::home_dir()
            .map(|h| h.join(".zshrc"))
            .unwrap_or_default()
    } else {
        dirs::home_dir()
            .map(|h| h.join(".bashrc"))
            .unwrap_or_default()
    };

    let aliases = format!(
        r#"
# lean-ctx shell hook — transparent CLI compression (90+ patterns)
if [ -z "$LEAN_CTX_ACTIVE" ]; then
alias git='{binary} -c git'
alias npm='{binary} -c npm'
alias pnpm='{binary} -c pnpm'
alias yarn='{binary} -c yarn'
alias cargo='{binary} -c cargo'
alias docker='{binary} -c docker'
alias docker-compose='{binary} -c docker-compose'
alias kubectl='{binary} -c kubectl'
alias k='{binary} -c kubectl'
alias gh='{binary} -c gh'
alias pip='{binary} -c pip'
alias pip3='{binary} -c pip3'
alias ruff='{binary} -c ruff'
alias go='{binary} -c go'
alias golangci-lint='{binary} -c golangci-lint'
alias eslint='{binary} -c eslint'
alias prettier='{binary} -c prettier'
alias tsc='{binary} -c tsc'
alias ls='{binary} -c ls'
alias find='{binary} -c find'
alias grep='{binary} -c grep'
alias curl='{binary} -c curl'
alias wget='{binary} -c wget'
fi
"#
    );

    if let Ok(existing) = std::fs::read_to_string(&rc_file) {
        if existing.contains("lean-ctx shell hook") {
            let cleaned = remove_lean_ctx_block(&existing);
            match std::fs::write(&rc_file, format!("{cleaned}{aliases}")) {
                Ok(()) => {
                    println!("Updated lean-ctx aliases in {}", rc_file.display());
                    println!("  Binary: {binary}");
                    return;
                }
                Err(e) => {
                    eprintln!("Error updating {}: {e}", rc_file.display());
                    return;
                }
            }
        }
    }

    match std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&rc_file)
    {
        Ok(mut f) => {
            use std::io::Write;
            let _ = f.write_all(aliases.as_bytes());
            println!("Added lean-ctx aliases to {}", rc_file.display());
            println!("  Binary: {binary}");
        }
        Err(e) => eprintln!("Error writing {}: {e}", rc_file.display()),
    }
}

fn remove_lean_ctx_block(content: &str) -> String {
    let mut result = String::new();
    let mut in_block = false;

    for line in content.lines() {
        if line.contains("lean-ctx shell hook") {
            in_block = true;
            continue;
        }
        if in_block {
            if line.trim() == "fi" || line.trim() == "end" || line.trim().is_empty() {
                if line.trim() == "fi" || line.trim() == "end" {
                    in_block = false;
                }
                continue;
            }
            if !line.starts_with("alias ") && !line.starts_with('\t') && !line.starts_with("if ") {
                in_block = false;
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

pub fn load_shell_history_pub() -> Vec<String> {
    load_shell_history()
}

fn load_shell_history() -> Vec<String> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Vec::new(),
    };

    let history_file = if shell.contains("zsh") {
        home.join(".zsh_history")
    } else if shell.contains("fish") {
        home.join(".local/share/fish/fish_history")
    } else if cfg!(windows) && shell.is_empty() {
        home.join("AppData")
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("PowerShell")
            .join("PSReadLine")
            .join("ConsoleHost_history.txt")
    } else {
        home.join(".bash_history")
    };

    match std::fs::read_to_string(&history_file) {
        Ok(content) => content
            .lines()
            .filter_map(|l| {
                let trimmed = l.trim();
                if trimmed.starts_with(':') {
                    trimmed.split(';').nth(1).map(|s| s.to_string())
                } else {
                    Some(trimmed.to_string())
                }
            })
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn print_savings(original: usize, sent: usize) {
    let saved = original.saturating_sub(sent);
    if original > 0 && saved > 0 {
        let pct = (saved as f64 / original as f64 * 100.0).round() as usize;
        println!("[{saved} tok saved ({pct}%)]");
    }
}

fn init_graph(args: &[String]) {
    // Determine project root: explicit arg or cwd
    let project_root = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

    let root_path = std::path::Path::new(&project_root);
    if !root_path.exists() {
        eprintln!("Error: directory '{}' does not exist", project_root);
        std::process::exit(1);
    }

    let dg_dir = root_path.join(".dual-graph");

    // 1. Create .dual-graph directory
    println!("Creating .dual-graph/ directory...");
    if let Err(e) = std::fs::create_dir_all(&dg_dir) {
        eprintln!("Error creating .dual-graph/: {e}");
        std::process::exit(1);
    }

    // 2. Run graph_scan
    println!("Scanning project with tree-sitter...");
    let (info_graph, symbol_index) = crate::graph::scanner::scan(&project_root);
    println!(
        "  {} files, {} symbols, {} edges",
        info_graph.file_count, info_graph.symbol_count, info_graph.edge_count
    );

    // Save graph files
    let state = crate::graph::GraphState {
        dual_graph_dir: Some(dg_dir.clone()),
        project_root: Some(project_root.clone()),
        info_graph: Some(info_graph),
        symbol_index,
        ..Default::default()
    };
    if let Err(e) = state.save_info_graph() {
        eprintln!("Error saving info_graph.json: {e}");
    }
    if let Err(e) = state.save_context_store() {
        eprintln!("Error saving context-store.json: {e}");
    }
    if let Err(e) = state.save_action_graph() {
        eprintln!("Error saving chat_action_graph.json: {e}");
    }

    // 3. Add .dual-graph/ to .gitignore
    let gitignore_path = root_path.join(".gitignore");
    let needs_entry = if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        !content.lines().any(|l| l.trim() == ".dual-graph" || l.trim() == ".dual-graph/")
    } else {
        true
    };
    if needs_entry {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gitignore_path)
        {
            let _ = writeln!(f, "\n# Dual-graph context data\n.dual-graph/");
            println!("Added .dual-graph/ to .gitignore");
        }
    }

    // 4. Write CLAUDE.md if it doesn't exist
    let claude_md = root_path.join("CLAUDE.md");
    if !claude_md.exists() {
        let policy = GRAPH_CLAUDE_MD;
        if let Err(e) = std::fs::write(&claude_md, policy) {
            eprintln!("Error writing CLAUDE.md: {e}");
        } else {
            println!("Created CLAUDE.md with dual-graph context policy");
        }
    } else {
        println!("CLAUDE.md already exists (not overwritten)");
    }

    println!("\nlean-ctx init --with-graph complete.");
    println!("  Graph data: {}", dg_dir.display());
    println!("  The MCP server will auto-load .dual-graph/ on startup.");
    println!("\nUsage: the LLM should call graph_continue first on each turn,");
    println!("then graph_read for recommended files (with compression).");
}

const GRAPH_CLAUDE_MD: &str = r#"# Dual-Graph Context Policy

This project uses lean-ctx with dual-graph for efficient context retrieval + compression.

## MANDATORY: Always follow this order

1. **Call `graph_continue` first** — before any file exploration, grep, or code reading.

2. **If `graph_continue` returns `needs_project=true`**: call `graph_scan` with the
   current project directory. Do NOT ask the user.

3. **If `graph_continue` returns `skip=true`**: project has fewer than 5 files.
   Do NOT do broad or recursive exploration. Read only specific files if their names
   are mentioned, or ask the user what to work on.

4. **Read `recommended_files`** using `graph_read` — **one call per file**.
   - `graph_read` accepts a single `file` parameter (string).
   - `recommended_files` may contain `file::symbol` entries (e.g. `src/auth.ts::handleLogin`).
     Pass them verbatim — it reads only that symbol's lines, not the full file.

5. **Check `confidence` and obey the caps strictly:**
   - `confidence=high` → Stop. Do NOT grep or explore further.
   - `confidence=medium` → call `fallback_rg` at most `max_supplementary_greps` times,
     then `graph_read` at most `max_supplementary_files` additional files. Then stop.
   - `confidence=low` → same caps as medium but slightly more files allowed.

## Rules

- Do NOT use `rg`, `grep`, or bash file exploration before calling `graph_continue`.
- Do NOT do broad/recursive exploration at any confidence level.
- `max_supplementary_greps` and `max_supplementary_files` are hard caps — never exceed them.
- After edits, call `graph_register_edit` with the changed files.

## Context Store

Call `graph_add_memory` for decisions, tasks, next steps, facts, or blockers:
```
graph_add_memory(type="decision", content="max 15 words", tags=["topic"], files=["file.rs"])
```
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_lean_ctx_block_posix() {
        let input = r#"# existing config
export PATH="$HOME/bin:$PATH"

# lean-ctx shell hook — transparent CLI compression (90+ patterns)
if [ -z "$LEAN_CTX_ACTIVE" ]; then
alias git='lean-ctx -c git'
alias npm='lean-ctx -c npm'
fi

# other stuff
export EDITOR=vim
"#;
        let result = remove_lean_ctx_block(input);
        assert!(!result.contains("lean-ctx"), "block should be removed");
        assert!(result.contains("export PATH"), "other content preserved");
        assert!(
            result.contains("export EDITOR"),
            "trailing content preserved"
        );
    }

    #[test]
    fn test_remove_lean_ctx_block_fish() {
        let input = "# other fish config\nset -x FOO bar\n\n# lean-ctx shell hook — transparent CLI compression (90+ patterns)\nif not set -q LEAN_CTX_ACTIVE\n\talias git 'lean-ctx -c git'\n\talias npm 'lean-ctx -c npm'\nend\n\n# more config\nset -x BAZ qux\n";
        let result = remove_lean_ctx_block(input);
        assert!(!result.contains("lean-ctx"), "block should be removed");
        assert!(result.contains("set -x FOO"), "other content preserved");
        assert!(result.contains("set -x BAZ"), "trailing content preserved");
    }

    #[test]
    fn test_remove_lean_ctx_block_ps() {
        let input = "# PowerShell profile\n$env:FOO = 'bar'\n\n# lean-ctx shell hook — transparent CLI compression (90+ patterns)\nif (-not $env:LEAN_CTX_ACTIVE) {\n  $LeanCtxBin = \"C:\\\\bin\\\\lean-ctx.exe\"\n  function git { & $LeanCtxBin -c \"git $($args -join ' ')\" }\n}\n\n# other stuff\n$env:EDITOR = 'vim'\n";
        let result = remove_lean_ctx_block_ps(input);
        assert!(
            !result.contains("lean-ctx shell hook"),
            "block should be removed"
        );
        assert!(result.contains("$env:FOO"), "other content preserved");
        assert!(result.contains("$env:EDITOR"), "trailing content preserved");
    }

    #[test]
    fn test_remove_block_no_lean_ctx() {
        let input = "# normal bashrc\nexport PATH=\"$HOME/bin:$PATH\"\n";
        let result = remove_lean_ctx_block(input);
        assert!(result.contains("export PATH"), "content unchanged");
    }
}
