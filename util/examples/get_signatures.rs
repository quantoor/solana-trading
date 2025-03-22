use std::str::FromStr;

use solana_trading_util::{
    signatures::{
        get_signatures_since_time, get_transactions_from_signatures, GetSignaturesSinceTimeConfig,
        GetTransactionsFromSignaturesConfig,
    },
    time::{datetime_from_timestamp_sec, datetime_now},
};

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};

async fn get_signatures(rpc: &RpcClient) -> Vec<Signature> {
    let since = datetime_now() - chrono::Duration::seconds(45);

    println!("requesting signatures since {:?}", since);
    let signatures = get_signatures_since_time(
        rpc,
        solana_sdk::pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"),
        since.timestamp(),
        GetSignaturesSinceTimeConfig {
            ignore_failed: true,
            limit: 1000,
            commitment: CommitmentConfig::finalized(),
            log_progress: true,
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

    let signatures = signatures[..100]
        .to_vec()
        .iter()
        .map(|sig| Signature::from_str(&sig.signature).unwrap())
        .collect::<Vec<_>>();
    signatures
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

    let signatures = get_signatures(&rpc).await;
    let signatures = signatures[..10].to_vec();
    println!("{:?}", signatures);

    let txs = get_transactions_from_signatures(
        &rpc,
        signatures,
        GetTransactionsFromSignaturesConfig {
            batch_size: 2,
            encoding: solana_transaction_status::UiTransactionEncoding::JsonParsed,
            commitment: CommitmentConfig::finalized(),
            log_progress: true,
        },
    )
    .await;

    println!("{:?}", txs);
}
