//! LSP (Language Server Protocol) server for moss.
//!
//! Provides IDE integration with document symbols, workspace symbols, and hover.

use crate::index::FileIndex;
use crate::skeleton::SkeletonExtractor;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Moss LSP backend.
struct MossBackend {
    client: Client,
    root: Mutex<Option<PathBuf>>,
    index: Mutex<Option<FileIndex>>,
}

impl MossBackend {
    fn new(client: Client) -> Self {
        Self {
            client,
            root: Mutex::new(None),
            index: Mutex::new(None),
        }
    }

    /// Initialize index for the workspace root.
    async fn init_index(&self, root: PathBuf) {
        if let Some(idx) = FileIndex::open_if_enabled(&root).await {
            *self.index.lock().await = Some(idx);
        }
        *self.root.lock().await = Some(root);
    }

    /// Convert moss symbol kind to LSP SymbolKind.
    fn to_lsp_symbol_kind(kind: &str) -> SymbolKind {
        match kind {
            "class" | "struct" => SymbolKind::CLASS,
            "function" => SymbolKind::FUNCTION,
            "method" => SymbolKind::METHOD,
            "interface" | "trait" => SymbolKind::INTERFACE,
            "enum" => SymbolKind::ENUM,
            "constant" | "const" => SymbolKind::CONSTANT,
            "variable" | "field" => SymbolKind::VARIABLE,
            "property" => SymbolKind::PROPERTY,
            "module" => SymbolKind::MODULE,
            "type" | "type_alias" => SymbolKind::TYPE_PARAMETER,
            "namespace" => SymbolKind::NAMESPACE,
            _ => SymbolKind::VARIABLE,
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MossBackend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Get workspace root from params
        if let Some(root_uri) = params.root_uri
            && let Ok(path) = root_uri.to_file_path()
        {
            self.init_index(path).await;
        } else if let Some(folders) = params.workspace_folders
            && let Some(folder) = folders.first()
            && let Ok(path) = folder.uri.to_file_path()
        {
            self.init_index(path).await;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "moss".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "moss LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Read file content
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        // Extract symbols using skeleton extractor
        let extractor = SkeletonExtractor::new();
        let result = extractor.extract(&file_path, &content);

        // Convert to LSP document symbols (nested structure)
        fn to_document_symbol(sym: &crate::skeleton::SkeletonSymbol) -> DocumentSymbol {
            let range = Range {
                start: Position {
                    line: sym.start_line.saturating_sub(1) as u32,
                    character: 0,
                },
                end: Position {
                    line: sym.end_line.saturating_sub(1) as u32,
                    character: 0,
                },
            };

            let children: Vec<DocumentSymbol> =
                sym.children.iter().map(to_document_symbol).collect();

            #[allow(deprecated)]
            DocumentSymbol {
                name: sym.name.clone(),
                detail: if sym.signature.is_empty() {
                    None
                } else {
                    Some(sym.signature.clone())
                },
                kind: MossBackend::to_lsp_symbol_kind(sym.kind.as_str()),
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
            }
        }

        let symbols: Vec<DocumentSymbol> = result.symbols.iter().map(to_document_symbol).collect();

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = &params.query;

        let index = self.index.lock().await;
        let root = self.root.lock().await;

        let (index, root) = match (index.as_ref(), root.as_ref()) {
            (Some(i), Some(r)) => (i, r.clone()),
            _ => return Ok(None),
        };

        // Search symbols in index
        let matches = match index.find_symbols(query, None, false, 50).await {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };

        #[allow(deprecated)]
        let symbols: Vec<SymbolInformation> = matches
            .into_iter()
            .map(|sym| {
                let file_path = root.clone().join(&sym.file);
                let uri = Url::from_file_path(&file_path)
                    .unwrap_or_else(|_| Url::parse("file:///unknown").unwrap());

                SymbolInformation {
                    name: sym.name,
                    kind: Self::to_lsp_symbol_kind(&sym.kind),
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri,
                        range: Range {
                            start: Position {
                                line: sym.start_line.saturating_sub(1) as u32,
                                character: 0,
                            },
                            end: Position {
                                line: sym.end_line.saturating_sub(1) as u32,
                                character: 0,
                            },
                        },
                    },
                    container_name: None,
                }
            })
            .collect();

        Ok(Some(symbols))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Read file content
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        // Extract symbols
        let extractor = SkeletonExtractor::new();
        let result = extractor.extract(&file_path, &content);

        // Find symbol at position (1-indexed line)
        let line = position.line as usize + 1;

        fn find_symbol_at_line<'a>(
            symbols: &'a [crate::skeleton::SkeletonSymbol],
            line: usize,
        ) -> Option<&'a crate::skeleton::SkeletonSymbol> {
            for sym in symbols {
                if line >= sym.start_line && line <= sym.end_line {
                    // Check children first for more specific match
                    if let Some(child) = find_symbol_at_line(&sym.children, line) {
                        return Some(child);
                    }
                    return Some(sym);
                }
            }
            None
        }

        let symbol = find_symbol_at_line(&result.symbols, line);

        match symbol {
            Some(sym) => {
                let mut content = format!("**{}** `{}`", sym.kind.as_str(), sym.name);
                if !sym.signature.is_empty() {
                    content.push_str(&format!("\n\n```\n{}\n```", sym.signature));
                }
                if let Some(doc) = &sym.docstring {
                    content.push_str(&format!("\n\n{}", doc));
                }

                Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: content,
                    }),
                    range: Some(Range {
                        start: Position {
                            line: sym.start_line.saturating_sub(1) as u32,
                            character: 0,
                        },
                        end: Position {
                            line: sym.end_line.saturating_sub(1) as u32,
                            character: 0,
                        },
                    }),
                }))
            }
            None => Ok(None),
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Read file content to get the word at position
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        // Get the word at the cursor position
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }

        let line = lines[line_idx];
        let col = position.character as usize;

        // Find word boundaries
        let word = extract_word_at_position(line, col);
        if word.is_empty() {
            return Ok(None);
        }

        // Search for symbol definition in index
        let index = self.index.lock().await;
        let root = self.root.lock().await;

        let (index, root) = match (index.as_ref(), root.as_ref()) {
            (Some(i), Some(r)) => (i, r.clone()),
            _ => return Ok(None),
        };

        // Look up symbol in index
        let matches = match index.find_symbol(&word).await {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };

        if matches.is_empty() {
            return Ok(None);
        }

        // Return first match (could enhance to return all)
        let (file, _kind, start_line, _end_line) = &matches[0];
        let target_path = root.join(file);
        let target_uri = match Url::from_file_path(&target_path) {
            Ok(u) => u,
            Err(_) => return Ok(None),
        };

        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range {
                start: Position {
                    line: start_line.saturating_sub(1) as u32,
                    character: 0,
                },
                end: Position {
                    line: start_line.saturating_sub(1) as u32,
                    character: 0,
                },
            },
        })))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        // Read file content to get the word at position
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }

        let line = lines[line_idx];
        let col = position.character as usize;
        let word = extract_word_at_position(line, col);
        if word.is_empty() {
            return Ok(None);
        }

        let index = self.index.lock().await;
        let root = self.root.lock().await;

        let (index, root) = match (index.as_ref(), root.as_ref()) {
            (Some(i), Some(r)) => (i, r.clone()),
            _ => return Ok(None),
        };

        let mut locations = Vec::new();

        // Include definition if requested
        if params.context.include_declaration {
            if let Ok(defs) = index.find_symbol(&word).await {
                for (file, _kind, start_line, _end_line) in defs {
                    let target_path = root.join(&file);
                    if let Ok(target_uri) = Url::from_file_path(&target_path) {
                        locations.push(Location {
                            uri: target_uri,
                            range: Range {
                                start: Position {
                                    line: start_line.saturating_sub(1) as u32,
                                    character: 0,
                                },
                                end: Position {
                                    line: start_line.saturating_sub(1) as u32,
                                    character: 0,
                                },
                            },
                        });
                    }
                }
            }
        }

        // Find callers (references)
        if let Ok(callers) = index.find_callers(&word).await {
            for (file, _caller_name, line) in callers {
                let target_path = root.join(&file);
                if let Ok(target_uri) = Url::from_file_path(&target_path) {
                    locations.push(Location {
                        uri: target_uri,
                        range: Range {
                            start: Position {
                                line: line.saturating_sub(1) as u32,
                                character: 0,
                            },
                            end: Position {
                                line: line.saturating_sub(1) as u32,
                                character: 0,
                            },
                        },
                    });
                }
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }

        let line = lines[line_idx];
        let col = position.character as usize;
        let word_info = extract_word_with_range(line, col);

        if word_info.word.is_empty() {
            return Ok(None);
        }

        // Verify this is a known symbol
        let index = self.index.lock().await;
        let index = match index.as_ref() {
            Some(i) => i,
            None => return Ok(None),
        };

        // Check if symbol exists in index
        if index
            .find_symbol(&word_info.word)
            .await
            .map(|m| m.is_empty())
            .unwrap_or(true)
        {
            // Also check if it's a caller (referenced symbol)
            if index
                .find_callers(&word_info.word)
                .await
                .map(|m| m.is_empty())
                .unwrap_or(true)
            {
                return Ok(None);
            }
        }

        Ok(Some(PrepareRenameResponse::Range(Range {
            start: Position {
                line: position.line,
                character: word_info.start_col as u32,
            },
            end: Position {
                line: position.line,
                character: word_info.end_col as u32,
            },
        })))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        if line_idx >= lines.len() {
            return Ok(None);
        }

        let line = lines[line_idx];
        let col = position.character as usize;
        let old_name = extract_word_with_range(line, col).word;

        if old_name.is_empty() {
            return Ok(None);
        }

        let index = self.index.lock().await;
        let root = self.root.lock().await;

        let (index, root) = match (index.as_ref(), root.as_ref()) {
            (Some(i), Some(r)) => (i, r.clone()),
            _ => return Ok(None),
        };

        // Collect all locations that need renaming
        let mut file_edits: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();

        // Find definition sites
        if let Ok(defs) = index.find_symbol(&old_name).await {
            for (file, _kind, start_line, _end_line) in defs {
                let target_path = root.join(&file);
                if let Ok(target_uri) = Url::from_file_path(&target_path)
                    && let Ok(file_content) = std::fs::read_to_string(&target_path)
                    && let Some(edit) =
                        find_rename_edit(&file_content, start_line, &old_name, &new_name)
                {
                    file_edits.entry(target_uri).or_default().push(edit);
                }
            }
        }

        // Find reference sites (callers)
        if let Ok(callers) = index.find_callers(&old_name).await {
            for (file, _caller_name, line) in callers {
                let target_path = root.join(&file);
                if let Ok(target_uri) = Url::from_file_path(&target_path)
                    && let Ok(file_content) = std::fs::read_to_string(&target_path)
                    && let Some(edit) = find_rename_edit(&file_content, line, &old_name, &new_name)
                {
                    file_edits.entry(target_uri).or_default().push(edit);
                }
            }
        }

        if file_edits.is_empty() {
            return Ok(None);
        }

        // Convert to WorkspaceEdit
        let changes: std::collections::HashMap<Url, Vec<TextEdit>> = file_edits;

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }
}

/// Word at a position with its range.
struct WordAtPosition {
    word: String,
    start_col: usize,
    end_col: usize,
}

/// Extract the word at a given column position in a line, with start/end positions.
fn extract_word_with_range(line: &str, col: usize) -> WordAtPosition {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return WordAtPosition {
            word: String::new(),
            start_col: 0,
            end_col: 0,
        };
    }

    // Find start of word
    let mut start = col;
    while start > 0 && is_identifier_char(chars[start - 1]) {
        start -= 1;
    }

    // Find end of word
    let mut end = col;
    while end < chars.len() && is_identifier_char(chars[end]) {
        end += 1;
    }

    WordAtPosition {
        word: chars[start..end].iter().collect(),
        start_col: start,
        end_col: end,
    }
}

/// Find a rename edit for a symbol at a given line.
fn find_rename_edit(
    content: &str,
    line_num: usize,
    old_name: &str,
    new_name: &str,
) -> Option<TextEdit> {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = line_num.saturating_sub(1);
    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // Find the symbol in this line (first occurrence)
    // Use word boundary matching to avoid partial matches
    let mut pos = 0;
    while let Some(idx) = line[pos..].find(old_name) {
        let abs_idx = pos + idx;
        let before_ok =
            abs_idx == 0 || !is_identifier_char(line.chars().nth(abs_idx - 1).unwrap_or(' '));
        let after_ok = abs_idx + old_name.len() >= line.len()
            || !is_identifier_char(line.chars().nth(abs_idx + old_name.len()).unwrap_or(' '));

        if before_ok && after_ok {
            return Some(TextEdit {
                range: Range {
                    start: Position {
                        line: line_idx as u32,
                        character: abs_idx as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: (abs_idx + old_name.len()) as u32,
                    },
                },
                new_text: new_name.to_string(),
            });
        }
        pos = abs_idx + old_name.len();
    }

    None
}

/// Extract the word at a given column position in a line.
fn extract_word_at_position(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if col >= chars.len() {
        return String::new();
    }

    // Find start of word
    let mut start = col;
    while start > 0 && is_identifier_char(chars[start - 1]) {
        start -= 1;
    }

    // Find end of word
    let mut end = col;
    while end < chars.len() && is_identifier_char(chars[end]) {
        end += 1;
    }

    chars[start..end].iter().collect()
}

/// Check if a character is valid in an identifier.
fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Start the LSP server on stdio.
pub async fn run_lsp_server(root: Option<&std::path::Path>) -> i32 {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(MossBackend::new);

    // If root is provided, initialize early (will be overridden by client's root)
    if let Some(_root) = root {
        // The client will provide the actual root during initialize
    }

    Server::new(stdin, stdout, socket).serve(service).await;
    0
}
