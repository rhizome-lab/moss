//! Reindex command - refresh the file index.

use crate::index;
use std::path::Path;

/// Refresh the file index
pub fn cmd_reindex(root: Option<&Path>, call_graph: bool) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    match index::FileIndex::open(&root) {
        Ok(mut idx) => {
            match idx.refresh() {
                Ok(count) => {
                    println!("Indexed {} files", count);

                    // Optionally rebuild call graph
                    if call_graph {
                        match idx.refresh_call_graph() {
                            Ok((symbols, calls, imports)) => {
                                println!(
                                    "Indexed {} symbols, {} calls, {} imports",
                                    symbols, calls, imports
                                );
                            }
                            Err(e) => {
                                eprintln!("Error indexing call graph: {}", e);
                                return 1;
                            }
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Error refreshing index: {}", e);
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("Error opening index: {}", e);
            1
        }
    }
}
