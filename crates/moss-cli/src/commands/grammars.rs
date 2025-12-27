//! Grammar management commands.

use clap::Subcommand;
use moss_languages::GrammarLoader;
use std::io::Read;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum GrammarAction {
    /// List installed grammars
    List,

    /// Install grammars from GitHub release
    Install {
        /// Specific version to install (default: latest)
        #[arg(long)]
        version: Option<String>,

        /// Force reinstall even if grammars exist
        #[arg(long)]
        force: bool,
    },

    /// Show grammar search paths
    Paths,
}

/// Run the grammars command
pub fn cmd_grammars(action: GrammarAction, json: bool) -> i32 {
    match action {
        GrammarAction::List => cmd_list(json),
        GrammarAction::Install { version, force } => cmd_install(version, force, json),
        GrammarAction::Paths => cmd_paths(json),
    }
}

fn cmd_list(json: bool) -> i32 {
    let loader = GrammarLoader::new();
    let grammars = loader.available_external();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "grammars": grammars,
                "count": grammars.len()
            })
        );
    } else if grammars.is_empty() {
        println!("No grammars installed.");
        println!();
        println!("Install grammars with: moss grammars install");
        println!("Or set MOSS_GRAMMAR_PATH to a directory containing .so/.dylib files");
    } else {
        println!("Installed grammars ({}):", grammars.len());
        for name in &grammars {
            println!("  {}", name);
        }
    }

    0
}

fn cmd_paths(json: bool) -> i32 {
    let mut paths = Vec::new();

    // Environment variable
    if let Ok(env_path) = std::env::var("MOSS_GRAMMAR_PATH") {
        for p in env_path.split(':') {
            if !p.is_empty() {
                paths.push(("env", PathBuf::from(p)));
            }
        }
    }

    // User config directory
    if let Some(config) = dirs::config_dir() {
        paths.push(("config", config.join("moss/grammars")));
    }

    if json {
        let path_objs: Vec<_> = paths
            .iter()
            .map(|(source, path)| {
                serde_json::json!({
                    "source": source,
                    "path": path.display().to_string(),
                    "exists": path.exists()
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "paths": path_objs }));
    } else {
        println!("Grammar search paths:");
        for (source, path) in &paths {
            let exists = if path.exists() { "" } else { " (not found)" };
            println!("  [{}] {}{}", source, path.display(), exists);
        }
    }

    0
}

fn cmd_install(version: Option<String>, force: bool, json: bool) -> i32 {
    const GITHUB_REPO: &str = "pterror/moss";

    // Determine install directory
    let install_dir = match dirs::config_dir() {
        Some(config) => config.join("moss/grammars"),
        None => {
            eprintln!("Could not determine config directory");
            return 1;
        }
    };

    // Check if grammars already exist
    if install_dir.exists() && !force {
        if let Ok(entries) = std::fs::read_dir(&install_dir) {
            let count = entries.filter(|e| e.is_ok()).count();
            if count > 0 {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "already_installed",
                            "path": install_dir.display().to_string(),
                            "count": count
                        })
                    );
                } else {
                    println!(
                        "Grammars already installed at {} ({} files)",
                        install_dir.display(),
                        count
                    );
                    println!("Use --force to reinstall");
                }
                return 0;
            }
        }
    }

    let client = ureq::agent();

    // Fetch release info
    let release_url = match &version {
        Some(v) => format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            GITHUB_REPO, v
        ),
        None => format!(
            "https://api.github.com/repos/{}/releases/latest",
            GITHUB_REPO
        ),
    };

    if !json {
        println!("Fetching release info...");
    }

    let response = match client
        .get(&release_url)
        .set("User-Agent", "moss-cli")
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to fetch release: {}", e);
            return 1;
        }
    };

    let body: serde_json::Value = match response.into_json() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to parse response: {}", e);
            return 1;
        }
    };

    let version = body["tag_name"].as_str().unwrap_or("unknown").to_string();

    // Find grammar asset for this platform
    let target = get_target_triple();
    let asset_name = format!("moss-grammars-{}.tar.gz", target);

    let assets = body["assets"].as_array();
    let asset_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some(&asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    let asset_url = match asset_url {
        Some(url) => url,
        None => {
            eprintln!("No grammars available for your platform: {}", target);
            eprintln!("Available assets:");
            if let Some(arr) = assets {
                for a in arr {
                    if let Some(name) = a["name"].as_str() {
                        if name.contains("grammars") {
                            eprintln!("  - {}", name);
                        }
                    }
                }
            }
            return 1;
        }
    };

    // Download grammars
    if !json {
        println!("Downloading {} grammars...", version);
    }

    let archive_response = match client.get(asset_url).call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to download grammars: {}", e);
            return 1;
        }
    };

    let mut archive_data = Vec::new();
    if let Err(e) = archive_response
        .into_reader()
        .read_to_end(&mut archive_data)
    {
        eprintln!("Failed to read download: {}", e);
        return 1;
    }

    // Create install directory
    if let Err(e) = std::fs::create_dir_all(&install_dir) {
        eprintln!("Failed to create directory: {}", e);
        return 1;
    }

    // Extract grammars
    if !json {
        println!("Installing to {}...", install_dir.display());
    }

    let count = match extract_grammars(&archive_data, &install_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to extract grammars: {}", e);
            return 1;
        }
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "installed",
                "version": version,
                "path": install_dir.display().to_string(),
                "count": count
            })
        );
    } else {
        println!("Installed {} grammars from {}", count, version);
    }

    0
}

fn get_target_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };

    format!("{}-{}", arch, os)
}

fn extract_grammars(data: &[u8], dest: &std::path::Path) -> Result<usize, String> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let decoder = GzDecoder::new(data);
    let mut archive = Archive::new(decoder);
    let mut count = 0;

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;

        // Only extract .so, .dylib, or .dll files
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.ends_with(".so")
                || name_str.ends_with(".dylib")
                || name_str.ends_with(".dll")
            {
                let dest_path = dest.join(name);
                entry.unpack(&dest_path).map_err(|e| e.to_string())?;
                count += 1;
            }
        }
    }

    Ok(count)
}
