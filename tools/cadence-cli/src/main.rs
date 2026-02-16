use anyhow::Result;
use cadence_core::Player;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cadence", version, about = "Cadence CLI (MVP)")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Play a file (FLAC/WAV/etc.)
    Play { path: String },
    /// Pause playback
    Pause,
    /// Resume playback
    Resume,
    /// Stop playback
    Stop,
    /// Seek approximately to ms in the same file
    Seek { path: String, to_ms: u64 },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let player = Player::new_default()?;

    match cli.cmd {
        Commands::Play { path } => {
            match player.load_and_play(&path) {
                Ok(info) => {
                    println!("Playing: {}", info.path);
                    if let Some(d) = info.duration_ms {
                        println!("Duration ~ {} ms", d);
                    }
                }
                Err(e) => {
                    eprintln!("Rodio failed ({e}). Trying Symphonia...");
                    let info = player.load_and_play_symphonia(&path)?;
                    println!("Playing (symphonia): {}", info.path);
                    if let Some(d) = info.duration_ms {
                        println!("Duration ~ {} ms", d);
                    }
                }
            }
            player.sleep_until_end();
        }
        Commands::Pause => player.pause(),
        Commands::Resume => player.resume(),
        Commands::Stop => player.stop(),
        Commands::Seek { path, to_ms } => {
            player.seek_approx(&path, to_ms)?;
            println!("Seeked to {} ms", to_ms);
        }
    }

    Ok(())
}
