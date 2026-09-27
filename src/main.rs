use clap::Parser;
use redisui::cli::Args;
use redisui::store;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let session = store::Session::new(&args)?;

    let mut terminal = ratatui::init();
    let result = redisui::app::run(&mut terminal, session).await;
    ratatui::restore();

    result
}
