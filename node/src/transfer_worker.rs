use anyhow::Result;
use tokio::sync::watch;

pub async fn run(mut shutdown: watch::Receiver<bool>) -> Result<()> {
    println!("Transfer worker is ready.");
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
    println!("Transfer worker stopped.");
    Ok(())
}
