//! Just a test

use solipr_network::SoliprPeer;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting peer!");
    let peer = SoliprPeer::start().await?;
    signal::ctrl_c().await?;
    println!("Stopping peer!");
    peer.stop().await?;
    Ok(())
}
