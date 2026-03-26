use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    println!("{} omnilens in {}", "Initializing".green().bold(), cwd.display());

    let _engine = super::create_engine()?;

    // Create .omnilens directory.
    let omnilens_dir = cwd.join(".omnilens");
    std::fs::create_dir_all(&omnilens_dir)?;

    println!(
        "{} Run {} to build the semantic index.",
        "Done.".green().bold(),
        "omnilens index".cyan()
    );
    Ok(())
}
