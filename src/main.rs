use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, MessageType, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client as LspClient, LanguageServer, LspService, Server};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

mod cache;
mod completion;
mod pypi;
mod types;

#[derive(Debug)]
struct Backend {
    client: LspClient,
    cache: Arc<cache::Cache>,

    documents: Arc<Mutex<HashMap<Url, String>>>, // Open documents
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    //
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        info!("Initializing PyPI versions language server");

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "\"".to_string(),
                        "=".to_string(),
                        ">".to_string(),
                        "<".to_string(),
                    ]),
                    ..Default::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),

                // definition_provider: Some(OneOf::Left(true)),
                // document_symbol_provider: Some(OneOf::Left(true)),
                // hover_provider: Some(HoverProviderCapability::Simple(true)),
                // references_provider: Some(OneOf::Left(true)),
                // workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: String::from("pyproject-ls"),
                version: Some(String::from("0.1.0")),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "server initialized!").await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        //
        let mut documents = self.documents.lock().await;
        let uri = params.text_document.uri;

        documents.insert(uri.clone(), params.text_document.text);

        self.client
            .log_message(MessageType::INFO, format!("file opened: {uri}"))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        //
        let mut documents = self.documents.lock().await;
        let uri = params.text_document.uri;

        if let Some(changes) = params.content_changes.first() {
            documents.insert(uri.clone(), changes.text.clone());
        }

        self.client
            .log_message(MessageType::INFO, format!("file changed: {uri}"))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut documents = self.documents.lock().await;
        documents.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        //
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let document = self.documents.lock().await;
        let text = document.get(&uri).unwrap();

        let result = completion::get_completions(text.as_str(), position, self.cache.clone()).await;

        Ok(result)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let logs_dir = PathBuf::from("/Users/dsully/.local/state/nvim");

    // Create logs directory if it doesn't exist
    std::fs::create_dir_all(&logs_dir).expect("Failed to create logs directory");

    let log_file = logs_dir.join("pyproject-ls.log");

    // Create non-blocking file appender
    let (non_blocking, _guard) =
        tracing_appender::non_blocking(std::fs::OpenOptions::new().create(true).append(true).open(log_file)?);

    // Create logging layers
    let file_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_target(false)
        .with_writer(non_blocking)
        .with_ansi(false);

    // Setup tracing subscriber with environment filter
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(file_layer)
        .init();

    info!("Starting PyPI versions language server");

    let cache = Arc::new(cache::Cache::new());

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        cache: cache.clone(),
        documents: Arc::new(Mutex::new(HashMap::new())),
    });

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
