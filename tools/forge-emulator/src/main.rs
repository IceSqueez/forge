use std::io::Write;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use forge_emulator::EmulatorError;
use forge_emulator::control::{
    ClientTimeouts, ControlClient, ControlEndpoint, EventFilter, Observation,
};
use forge_events::Event;

const TOKEN_VARIABLE: &str = "FORGE_EMULATOR_TOKEN";

#[derive(Parser)]
#[command(
    name = "forge-emulator",
    about = "Drive and observe a running forge over its control socket",
    after_help = "The server bearer token is read from FORGE_EMULATOR_TOKEN."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print every event forge publishes, one JSON object per line, until interrupted.
    Watch {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        port: u16,
        /// Print this many already-published events first; they may repeat as live events.
        #[arg(long, default_value_t = 0)]
        history: u32,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let Cli { command } = Cli::parse();
    let outcome = match command {
        Command::Watch {
            host,
            port,
            history,
        } => watch(&host, port, history).await,
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("forge-emulator: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn watch(host: &str, port: u16, history: u32) -> Result<(), EmulatorError> {
    let token = std::env::var(TOKEN_VARIABLE)
        .ok()
        .filter(|token| !token.is_empty())
        .ok_or(EmulatorError::MissingToken {
            variable: TOKEN_VARIABLE,
        })?;
    let endpoint = ControlEndpoint::loopback(host, port)?;
    let (client, mut observations) =
        ControlClient::connect(&endpoint, ClientTimeouts::default()).await?;
    client.authenticate(&token).await?;
    client.subscribe(&[EventFilter::default()]).await?;

    let interrupted = tokio::signal::ctrl_c();
    tokio::pin!(interrupted);
    let mut out = std::io::stdout();
    if history > 0 {
        for event in client.recent_events(history).await? {
            print_event(&mut out, &event)?;
        }
    }
    loop {
        tokio::select! {
            _ = &mut interrupted => return Ok(()),
            observation = observations.next() => match observation {
                Some(Observation::Event(event)) => print_event(&mut out, &event)?,
                Some(Observation::Dropped(count)) => {
                    eprintln!("forge-emulator: server dropped {count} events");
                }
                Some(Observation::Undecodable { frame, reason }) => {
                    eprintln!("forge-emulator: undecodable frame ({reason}): {frame}");
                }
                None => return Err(EmulatorError::ConnectionClosed),
            },
        }
    }
}

fn print_event(out: &mut impl Write, event: &Event) -> Result<(), EmulatorError> {
    let output_error = |reason: String| EmulatorError::Output { reason };
    let line = serde_json::to_string(event).map_err(|e| output_error(e.to_string()))?;
    writeln!(out, "{line}")
        .and_then(|()| out.flush())
        .map_err(|e| output_error(e.to_string()))
}
