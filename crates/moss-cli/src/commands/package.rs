//! Package registry queries.

use clap::Subcommand;
use moss_packages::{detect_all_ecosystems, all_ecosystems, PackageInfo, PackageError};
use std::path::Path;

#[derive(Subcommand)]
pub enum PackageAction {
    /// Query package info from registry
    Info {
        /// Package name to query (optionally with @version)
        package: String,
    },
    /// List declared dependencies from manifest
    List,
    /// Show dependency tree from lockfile
    Tree,
    /// Show outdated packages (installed vs latest)
    Outdated,
}

pub fn cmd_package(
    action: PackageAction,
    ecosystem: Option<&str>,
    root: Option<&Path>,
    json: bool,
) -> i32 {
    let project_root = root.unwrap_or(Path::new("."));

    // Get ecosystem either by name or by detection
    if let Some(name) = ecosystem {
        // Explicit ecosystem specified
        match find_ecosystem_by_name(name) {
            Some(eco) => run_for_ecosystem(eco, &action, project_root, json),
            None => {
                eprintln!("error: unknown ecosystem '{}'", name);
                eprintln!("available: {}", available_ecosystems().join(", "));
                1
            }
        }
    } else {
        // Auto-detect ecosystems
        let ecosystems = detect_all_ecosystems(project_root);
        if ecosystems.is_empty() {
            eprintln!("error: could not detect ecosystem from project files");
            eprintln!("hint: use --ecosystem to specify explicitly");
            eprintln!("available: {}", available_ecosystems().join(", "));
            return 1;
        }

        // For list/tree, run for all detected ecosystems
        // For info/outdated, use first ecosystem only
        match &action {
            PackageAction::List | PackageAction::Tree => {
                if json && ecosystems.len() > 1 {
                    // Collect all results into a JSON array
                    run_all_ecosystems_json(&ecosystems, &action, project_root)
                } else {
                    let mut exit_code = 0;
                    for (i, eco) in ecosystems.iter().enumerate() {
                        if i > 0 {
                            println!(); // Separator between ecosystems
                        }
                        let result = run_for_ecosystem(*eco, &action, project_root, json);
                        if result != 0 {
                            exit_code = result;
                        }
                    }
                    exit_code
                }
            }
            _ => {
                if ecosystems.len() > 1 {
                    let names: Vec<_> = ecosystems.iter().map(|e| e.name()).collect();
                    eprintln!("note: multiple ecosystems detected: {}", names.join(", "));
                    eprintln!("hint: use --ecosystem to specify which one");
                }
                run_for_ecosystem(ecosystems[0], &action, project_root, json)
            }
        }
    }
}

fn run_all_ecosystems_json(
    ecosystems: &[&dyn moss_packages::Ecosystem],
    action: &PackageAction,
    project_root: &Path,
) -> i32 {
    let mut results = serde_json::Map::new();

    for eco in ecosystems {
        match action {
            PackageAction::List => {
                match eco.list_dependencies(project_root) {
                    Ok(deps) => {
                        results.insert(eco.name().to_string(), serde_json::json!({
                            "dependencies": deps.iter().map(|d| serde_json::json!({
                                "name": d.name,
                                "version_req": d.version_req,
                                "optional": d.optional,
                            })).collect::<Vec<_>>()
                        }));
                    }
                    Err(e) => {
                        results.insert(eco.name().to_string(), serde_json::json!({
                            "error": e.to_string()
                        }));
                    }
                }
            }
            PackageAction::Tree => {
                match eco.dependency_tree(project_root) {
                    Ok(tree) => {
                        results.insert(eco.name().to_string(), serde_json::json!({
                            "tree": tree
                        }));
                    }
                    Err(e) => {
                        results.insert(eco.name().to_string(), serde_json::json!({
                            "error": e.to_string()
                        }));
                    }
                }
            }
            _ => {}
        }
    }

    println!("{}", serde_json::Value::Object(results));
    0
}

fn run_for_ecosystem(
    eco: &dyn moss_packages::Ecosystem,
    action: &PackageAction,
    project_root: &Path,
    json: bool,
) -> i32 {
    match action {
        PackageAction::Info { package } => cmd_info(eco, package, project_root, json),
        PackageAction::List => cmd_list(eco, project_root, json),
        PackageAction::Tree => cmd_tree(eco, project_root, json),
        PackageAction::Outdated => cmd_outdated(eco, project_root, json),
    }
}

fn cmd_info(eco: &dyn moss_packages::Ecosystem, package: &str, project_root: &Path, json: bool) -> i32 {
    match eco.query(package, project_root) {
        Ok(info) => {
            if json {
                print_json(&info);
            } else {
                print_human(&info, eco.name());
            }
            0
        }
        Err(e) => {
            match e {
                PackageError::NotFound(name) => {
                    eprintln!("error: package '{}' not found in {} registry", name, eco.name());
                }
                PackageError::NoToolFound => {
                    eprintln!("error: no {} tools found in PATH", eco.name());
                    eprintln!("hint: install one of: {:?}", eco.tools());
                }
                _ => {
                    eprintln!("error: {}", e);
                }
            }
            1
        }
    }
}

fn cmd_list(eco: &dyn moss_packages::Ecosystem, project_root: &Path, json: bool) -> i32 {
    match eco.list_dependencies(project_root) {
        Ok(deps) => {
            if json {
                println!("{}", serde_json::json!({
                    "ecosystem": eco.name(),
                    "dependencies": deps.iter().map(|d| serde_json::json!({
                        "name": d.name,
                        "version_req": d.version_req,
                        "optional": d.optional,
                    })).collect::<Vec<_>>()
                }));
            } else {
                println!("{} dependencies ({})", deps.len(), eco.name());
                println!();
                for dep in &deps {
                    let version = dep.version_req.as_deref().unwrap_or("*");
                    let optional = if dep.optional { " (optional)" } else { "" };
                    println!("  {} {}{}", dep.name, version, optional);
                }
            }
            0
        }
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

fn cmd_tree(eco: &dyn moss_packages::Ecosystem, project_root: &Path, json: bool) -> i32 {
    match eco.dependency_tree(project_root) {
        Ok(tree) => {
            if json {
                println!("{}", serde_json::json!({
                    "ecosystem": eco.name(),
                    "tree": tree,
                }));
            } else {
                print_tree(&tree);
            }
            0
        }
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

fn print_tree(tree: &moss_packages::DependencyTree) {
    for root in &tree.roots {
        print_node(root, 0);
    }
}

fn print_node(node: &moss_packages::TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    if node.version.is_empty() {
        println!("{}{}", indent, node.name);
    } else {
        println!("{}{} v{}", indent, node.name, node.version);
    }
    for child in &node.dependencies {
        print_node(child, depth + 1);
    }
}

fn cmd_outdated(eco: &dyn moss_packages::Ecosystem, project_root: &Path, json: bool) -> i32 {
    // Get declared dependencies
    let deps = match eco.list_dependencies(project_root) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}", e);
            return 1;
        }
    };

    #[derive(serde::Serialize)]
    struct OutdatedPackage {
        name: String,
        installed: Option<String>,
        latest: String,
        wanted: Option<String>,
    }

    let mut outdated = Vec::new();
    let mut errors = Vec::new();

    for dep in &deps {
        // Get installed version from lockfile
        let installed = eco.installed_version(&dep.name, project_root);

        // Get latest version from registry
        match eco.query(&dep.name, project_root) {
            Ok(info) => {
                // Only show if installed differs from latest
                let is_outdated = match &installed {
                    Some(v) => v != &info.version,
                    None => true, // Not installed = show it
                };

                if is_outdated {
                    outdated.push(OutdatedPackage {
                        name: dep.name.clone(),
                        installed: installed.clone(),
                        latest: info.version,
                        wanted: dep.version_req.clone(),
                    });
                }
            }
            Err(e) => {
                errors.push((dep.name.clone(), e.to_string()));
            }
        }
    }

    if json {
        println!("{}", serde_json::json!({
            "outdated": outdated,
            "errors": errors.iter().map(|(n, e)| serde_json::json!({"name": n, "error": e})).collect::<Vec<_>>()
        }));
    } else {
        if outdated.is_empty() && errors.is_empty() {
            println!("All packages are up to date");
        } else {
            if !outdated.is_empty() {
                println!("Outdated packages ({}):", outdated.len());
                println!();
                for pkg in &outdated {
                    let installed = pkg.installed.as_deref().unwrap_or("(not installed)");
                    println!("  {} {} → {}", pkg.name, installed, pkg.latest);
                }
            }
            if !errors.is_empty() {
                println!();
                println!("Errors ({}):", errors.len());
                for (name, err) in &errors {
                    println!("  {}: {}", name, err);
                }
            }
        }
    }

    0
}

fn find_ecosystem_by_name(name: &str) -> Option<&'static dyn moss_packages::Ecosystem> {
    all_ecosystems()
        .iter()
        .find(|e| e.name() == name)
        .copied()
}

fn available_ecosystems() -> Vec<&'static str> {
    all_ecosystems().iter().map(|e| e.name()).collect()
}

fn print_json(info: &PackageInfo) {
    if let Ok(json) = serde_json::to_string_pretty(info) {
        println!("{}", json);
    }
}

fn print_human(info: &PackageInfo, ecosystem: &str) {
    println!("{} {} ({})", info.name, info.version, ecosystem);

    if let Some(desc) = &info.description {
        println!("{}", desc);
    }

    println!();

    if let Some(license) = &info.license {
        println!("license: {}", license);
    }

    if let Some(homepage) = &info.homepage {
        println!("homepage: {}", homepage);
    }

    if let Some(repo) = &info.repository {
        println!("repository: {}", repo);
    }

    if !info.features.is_empty() {
        println!();
        println!("features:");
        for feature in &info.features {
            if feature.dependencies.is_empty() {
                println!("  {}", feature.name);
            } else {
                println!("  {} = [{}]", feature.name, feature.dependencies.join(", "));
            }
        }
    }

    if !info.dependencies.is_empty() {
        println!();
        println!("dependencies:");
        for dep in &info.dependencies {
            let version = dep.version_req.as_deref().unwrap_or("*");
            let optional = if dep.optional { " (optional)" } else { "" };
            println!("  {} {}{}", dep.name, version, optional);
        }
    }
}
