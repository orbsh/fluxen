mod console;
mod mirror;

use clap::{Parser, Subcommand};
use stage::proto;

#[derive(Parser)]
#[command(name = "stage", about = "Brick dev gateway: mirror + console + KDL send")]
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
    },
    /// Offline: parse KDL and print the Brick JSON tree
    Tojson { file: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Serve { port: 3002 }) {
        Cmd::Serve { port } => {
            // mirror + console in one process: spawn the server, then run the
            // REPL on the main task. Ctrl-C / /quit exits the console; the
            // mirror dies with the process.
            let server = tokio::spawn(async move {
                if let Err(e) = mirror::serve(port).await {
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
            let bricks = proto::parse_kdl_to_bricks(&src)?;
            for b in &bricks {
                println!("{}", serde_json::to_string_pretty(b)?);
            }
            Ok(())
        }
    }
}
