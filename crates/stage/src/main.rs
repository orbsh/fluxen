mod console;
mod mirror;
mod ui;

use clap::{Parser, Subcommand};
use stage::proto;

#[derive(Parser)]
#[command(
    name = "stage",
    about = "Accrete dev gateway: mirror + console + KDL send"
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
    /// Offline: parse KDL and print the Accrete JSON tree
    Tojson { file: String },
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
            // format is an explicit choice by extension — same rule as /send?fmt=
            let is_yaml = matches!(
                std::path::Path::new(&file)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or(""),
                "yaml" | "yml"
            );
            let value = if is_yaml {
                serde_json::to_value(proto::parse_yaml_to_frame(&src)?)?
            } else {
                let accretes = proto::parse_kdl_to_accretes(&src)?;
                serde_json::to_value(&accretes)?
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
    }
}
