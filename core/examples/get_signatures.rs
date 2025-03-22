use core::{
    signatures::{get_signatures_since_time, GetSignaturesSinceTimeConfig},
    time::{datetime_from_timestamp_sec, datetime_now},
};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;

#[tokio::main]
async fn main() {
    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
    let since = datetime_now() - chrono::Duration::seconds(45);

    println!("requesting signatures since {:?}", since);
    let signatures = get_signatures_since_time(
        &rpc,
        GetSignaturesSinceTimeConfig {
            target: solana_sdk::pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"),
            since_timestamp_sec: since.timestamp(),
            ignore_failed: true,
            commitment: CommitmentConfig::finalized(),
        },
    )
    .await
    .unwrap();

    println!("found {}", signatures.len());
    let oldest_block_time = signatures[signatures.len() - 1].block_time.unwrap();
    println!(
        "oldest signature time: {:?}",
        datetime_from_timestamp_sec(oldest_block_time).unwrap()
    );
}
