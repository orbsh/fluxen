mod mirror;
mod repl;
mod send;

use clap::{Parser, Subcommand};
use stage::proto;

#[derive(Parser)]
#[command(name = "stage", about = "Brick dev gateway: KDL preview & debugging")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the mirror server (default port 3002)
    Serve {
        #[arg(long, default_value = "3002")]
        port: u16,
    },
    /// Parse a KDL file and send it as a brick frame
    Send {
        file: String,
        #[arg(long, default_value = "ws://127.0.0.1:3002/cli")]
        url: String,
    },
    /// Interactive REPL
    Repl {
        #[arg(long, default_value = "ws://127.0.0.1:3002/cli")]
        url: String,
    },
    /// Offline: parse KDL and print the Brick JSON tree
    Tojson { file: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { port } => mirror::serve(port).await,
        Cmd::Send { file, url } => send::send(&file, &url).await,
        Cmd::Repl { url } => repl::run(&url).await,
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
