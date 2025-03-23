use std::str::FromStr;

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use solana_trading_balance::subscribe::{subscribe_balance_udpates_by_owner, GrpcConfig};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let endpoint = std::env::var("GRPC_ENDPOINT").unwrap();
    let x_token = std::env::var("GRPC_TOKEN").unwrap();
    let wallet = std::env::var("WALLET").unwrap();

    let mut rx = subscribe_balance_udpates_by_owner(
        GrpcConfig {
            endpoint,
            x_token: Some(x_token),
        },
        &Pubkey::from_str(wallet.as_str()).unwrap(),
        &vec![(
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            false,
        )],
    )
    .await
    .unwrap();

    while let Some(update) = rx.recv().await {
        println!("{:?}", update);
    }

    Ok(())
}
