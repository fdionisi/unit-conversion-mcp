use std::{env, sync::Arc};

use anyhow::Result;
use context_server::{ContextServer, ContextServerRpcRequest, ContextServerRpcResponse};
use context_server_utils::tool_registry::ToolRegistry;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use unit_conversion_mcp_primitives::tools::UnitConversion;

struct ContextServerState {
    rpc: ContextServer,
}

impl ContextServerState {
    async fn new() -> Result<Self> {
        let tool_registry = Arc::new(ToolRegistry::default());

        tool_registry.register(Arc::new(UnitConversion));

        Ok(Self {
            rpc: ContextServer::builder()
                .with_server_info((env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
                .with_tools(tool_registry)
                .build()?,
        })
    }

    async fn process_request(
        &self,
        request: ContextServerRpcRequest,
    ) -> Result<Option<ContextServerRpcResponse>> {
        self.rpc.handle_incoming_message(request).await
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = ContextServerState::new().await?;

    let mut stdin = BufReader::new(io::stdin()).lines();
    let mut stdout = io::stdout();

    while let Some(line) = stdin.next_line().await? {
        let request: ContextServerRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Error parsing request: {}", e);
                continue;
            }
        };

        if let Some(response) = state.process_request(request).await? {
            let response_json = serde_json::to_string(&response)?;
            stdout.write_all(response_json.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}
