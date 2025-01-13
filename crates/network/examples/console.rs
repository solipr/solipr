//! Just a test

use solipr_network::SoliprPeer;
use tokio::io::{AsyncBufReadExt, BufReader, stdin};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting peer!");
    let mut peer = SoliprPeer::start().await?;
    let mut lines = BufReader::new(stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        match line.as_str() {
            "stop" => break,
            "status" => {
                println!("PeerID: {}", peer.id());
                let provided = peer.provided_keys();
                if provided.is_empty() {
                    println!("No keys provided");
                } else {
                    println!("Provided keys:");
                    for key in provided {
                        println!("- {}", String::from_utf8_lossy(key));
                    }
                }
            }
            line => match line.split_once(' ') {
                Some(("provide", key)) => {
                    if peer.provide_key(key).await? {
                        println!("Provided key: {key}");
                    } else {
                        println!("Failed to provide key: {key}");
                    }
                }
                Some(("unprovide", key)) => {
                    peer.stop_providing_key(key).await?;
                    println!("Unprovided key: {key}");
                }
                Some(("find", key)) => {
                    let peers = peer.find_providers(key).await?;
                    if peers.is_empty() {
                        println!("No peers found");
                    } else {
                        println!("Found peers:");
                        for peer in peers {
                            println!("- {}", peer);
                        }
                    }
                }
                _ => println!("Unknown command: {line}"),
            },
        }
    }
    println!("Stopping peer!");
    peer.stop().await?;
    Ok(())
}
