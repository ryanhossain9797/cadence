use anyhow::Result;
use cadence_core::Player;
use clap::Parser;
use std::io::{self, BufRead, Write};

#[derive(Parser)]
#[command(name = "cadence", version, about = "Cadence CLI (MVP)")]
struct Cli {
    /// Audio file to play
    path: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let player = Player::new()?;

    // Play the file
    let info = player.load_and_play(&cli.path)?;
    println!(
        "Playing: {} ({} ms)",
        info.path,
        info.duration_ms.unwrap_or(0)
    );
    
    let commands_description = "Commands: pause, resume, stop, +/- <seconds> (advance or rewind by <seconds>), quit";
    
    println!("{}", commands_description);

    // REPL loop for commands
    let stdin = io::stdin();
    print!("> ");
    io::stdout().flush()?;

    for line in stdin.lock().lines() {
        let line = line?;
        let input = line.trim();
        let parts: Vec<&str> = input.split_whitespace().collect();

        if parts.is_empty() {
            print!("> ");
            io::stdout().flush()?;
            continue;
        }

        match parts[0] {
            "pause" => {
                player.pause();
                println!("Paused");
            }
            "resume" => {
                player.resume();
                println!("Resumed");
            }
            "stop" => {
                player.stop();
                println!("Stopped");
            }
            "+" | "-" => {
                if parts.len() < 2 {
                    println!("Usage: +/- <seconds>. enter a number after +/- !!>");
                } else {
                    match parts[1].parse::<u64>() {
                        Ok(secs) => {
                            let delta = if parts[0] == "+" { secs as i64 } else { -(secs as i64) };
                            if let Err(e) = player.advance_or_rewind(&cli.path, delta * 1000) {
                                println!("Error: {}", e);
                            }
                        }
                        Err(_) => println!("Invalid number: {}", parts[1]),
                    }
                }
            }
            "quit" | "q" | "exit" => {
                player.stop();
                break;
            }
            "help" | "h" => {
                println!("{}", commands_description);
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for commands.", parts[0]);
            }
        }

        print!("> ");
        io::stdout().flush()?;
    }

    Ok(())
}
