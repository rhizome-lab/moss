use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("build-grammars") => build_grammars(&args[2..]),
        Some("help") | None => print_help(),
        Some(cmd) => {
            eprintln!("Unknown command: {cmd}");
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  build-grammars [--out <dir>] [--force]");
    eprintln!("      Compile tree-sitter grammars to shared libraries");
    eprintln!("      --out <dir>  Output directory (default: target/grammars)");
    eprintln!("      --force      Recompile even if grammar already exists");
    eprintln!("  help             Show this message");
}

fn build_grammars(args: &[String]) {
    let (out_dir, force) = parse_build_args(args);
    fs::create_dir_all(&out_dir).expect("Failed to create output directory");

    let registry_src = find_cargo_registry_src();
    let grammars = find_arborium_grammars(&registry_src);

    if grammars.is_empty() {
        eprintln!(
            "No arborium grammar crates found. Run 'cargo build' first to download dependencies."
        );
        std::process::exit(1);
    }

    println!(
        "Found {} grammars, output: {}",
        grammars.len(),
        out_dir.display()
    );

    let mut compiled = 0;
    let mut skipped = 0;
    let mut failed = 0;
    let mut queries_copied = 0;

    for (lang, crate_dir) in &grammars {
        // Always copy query files (highlights.scm, injections.scm)
        queries_copied += copy_query_files(lang, crate_dir, &out_dir);

        // Check if grammar already exists
        let lib_ext = lib_extension();
        let out_file = out_dir.join(format!("{lang}.{lib_ext}"));

        if out_file.exists() && !force {
            skipped += 1;
            continue;
        }

        match compile_grammar(lang, crate_dir, &out_dir) {
            Ok(size) => {
                println!("  {lang}: {}", human_size(size));
                compiled += 1;
            }
            Err(e) => {
                eprintln!("  {lang}: FAILED - {e}");
                failed += 1;
            }
        }
    }

    println!("\nCompiled {compiled} grammars, skipped {skipped} (already built), {failed} failed");
    if queries_copied > 0 {
        println!("Copied {queries_copied} query files");
    }
}

fn parse_build_args(args: &[String]) -> (PathBuf, bool) {
    let mut out_dir = PathBuf::from("target/grammars");
    let mut force = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" if i + 1 < args.len() => {
                out_dir = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--force" => force = true,
            _ => {}
        }
        i += 1;
    }
    (out_dir, force)
}

fn lib_extension() -> &'static str {
    if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

/// Copy query files (highlights.scm, injections.scm) if they don't exist.
/// Returns the number of files copied.
fn copy_query_files(lang: &str, crate_dir: &Path, out_dir: &Path) -> usize {
    let mut copied = 0;

    let query_files = [
        ("highlights.scm", format!("{lang}.highlights.scm")),
        ("injections.scm", format!("{lang}.injections.scm")),
    ];

    for (src_name, dest_name) in &query_files {
        let src = crate_dir.join("queries").join(src_name);
        let dest = out_dir.join(dest_name);

        if src.exists() && !dest.exists() {
            if fs::copy(&src, &dest).is_ok() {
                copied += 1;
            }
        }
    }

    copied
}

fn find_cargo_registry_src() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("No home directory");
    PathBuf::from(home).join(".cargo/registry/src")
}

fn find_arborium_grammars(registry_src: &Path) -> Vec<(String, PathBuf)> {
    let mut grammars = Vec::new();

    let Ok(entries) = fs::read_dir(registry_src) else {
        return grammars;
    };

    for entry in entries.flatten() {
        let index_dir = entry.path();
        if !index_dir.is_dir() {
            continue;
        }

        let Ok(crates) = fs::read_dir(&index_dir) else {
            continue;
        };

        for crate_entry in crates.flatten() {
            let crate_dir = crate_entry.path();
            let name = crate_dir.file_name().unwrap().to_string_lossy();

            if let Some(lang) = name.strip_prefix("arborium-") {
                // Skip non-language crates
                if matches!(lang.split('-').next(), Some("tree" | "theme" | "highlight")) {
                    continue;
                }

                // Strip version suffix (e.g., "c-sharp-2.4.5" -> "c-sharp")
                // Version is always at the end in format X.Y.Z
                let lang = strip_version_suffix(lang);

                // Check grammar exists
                if crate_dir.join("grammar/src/parser.c").exists() {
                    grammars.push((lang.to_string(), crate_dir));
                }
            }
        }
    }

    // Deduplicate - keep latest version (they're sorted lexically, so highest version wins)
    grammars.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    grammars.dedup_by(|a, b| a.0 == b.0);

    grammars.sort_by(|a, b| a.0.cmp(&b.0));
    grammars
}

fn compile_grammar(lang: &str, crate_dir: &Path, out_dir: &Path) -> Result<u64, String> {
    let parser_c = crate_dir.join("grammar/src/parser.c");
    let scanner_c = crate_dir.join("grammar/scanner.c");

    let out_file = out_dir.join(format!("{lang}.{}", lib_extension()));

    let mut cmd = Command::new("cc");
    cmd.arg("-shared")
        .arg("-fPIC")
        .arg("-O2")
        .arg("-I")
        .arg(crate_dir.join("grammar/src"))
        .arg("-I")
        .arg(crate_dir.join("grammar/include"))
        .arg("-I")
        .arg(crate_dir.join("grammar"))
        .arg("-I")
        .arg(crate_dir.join("grammar/src/tree_sitter"))
        .arg(&parser_c);

    if scanner_c.exists() {
        cmd.arg(&scanner_c);
    }

    // Scanner uses ts_calloc/ts_free - resolved at runtime
    #[cfg(target_os = "linux")]
    cmd.arg("-Wl,--unresolved-symbols=ignore-in-shared-libs");

    #[cfg(target_os = "macos")]
    cmd.arg("-undefined").arg("dynamic_lookup");

    cmd.arg("-o").arg(&out_file);

    let output = cmd.output().map_err(|e| format!("Failed to run cc: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Compilation failed: {stderr}"));
    }

    let size = fs::metadata(&out_file).map(|m| m.len()).unwrap_or(0);
    Ok(size)
}

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{}K", bytes / 1024)
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Strip version suffix from crate name (e.g., "c-sharp-2.4.5" -> "c-sharp").
fn strip_version_suffix(name: &str) -> &str {
    // Match semver pattern at end: -X.Y.Z (with optional pre-release/build metadata)
    // Cargo crate names end with -MAJOR.MINOR.PATCH
    if let Some(idx) = name.rfind('-') {
        let suffix = &name[idx + 1..];
        // Check if suffix looks like semver: starts with digit, contains dot
        if suffix
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && suffix.contains('.')
        {
            return &name[..idx];
        }
    }
    name
}
