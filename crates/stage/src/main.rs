mod console;
mod mirror;
mod ui;

use clap::{Parser, Subcommand};
use stage::proto;

#[derive(Parser)]
#[command(
    name = "stage",
    about = "Accrete dev gateway: mirror + console + YAML send"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the mirror and drop into the interactive console
    Serve {
        #[arg(long, default_value = "3002")]
        port: u16,
        /// trunk dev-server port; up -> reverse-proxy the UI from it
        #[arg(long, default_value = "8281")]
        trunk: u16,
        /// dist dir served when trunk is down (relative to cwd)
        #[arg(long, default_value = "crates/ui_leptos/dist")]
        dist: String,
    },
    /// Offline: parse a YAML frame file and print the wire JSON
    Tojson { file: String },
    /// Print the JSON Schema (draft 2020-12) of the Accrete wire shape
    Schema,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let default_serve = || Cmd::Serve {
        port: 3002,
        trunk: 8281,
        dist: "crates/ui_leptos/dist".into(),
    };
    match cli.cmd.unwrap_or_else(default_serve) {
        Cmd::Serve { port, trunk, dist } => {
            // mirror + console in one process: spawn the server, then run the
            // REPL on the main task. Ctrl-C / /quit exits the console; the
            // mirror dies with the process.
            let server = tokio::spawn(async move {
                if let Err(e) = mirror::serve(port, trunk, std::path::PathBuf::from(dist)).await {
                    eprintln!("mirror error: {e}");
                }
            });
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            console::run(port).await?;
            server.abort();
            Ok(())
        }
        Cmd::Tojson { file } => {
            let src = std::fs::read_to_string(&file)?;
            let value = serde_json::to_value(proto::parse_yaml_to_frame(&src)?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Schema => {
            let schema = schemars::schema_for!(accrete::Accrete);
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
    }
}
