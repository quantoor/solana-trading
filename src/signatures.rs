use std::str::FromStr;

use anyhow::Result;
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::RpcTransactionConfig, rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};

use crate::time::datetime_from_timestamp_sec;

pub struct GetSignaturesSinceTimeConfig {
    pub target: Pubkey,
    pub since_timestamp_sec: i64,
    pub ignore_failed: bool,
    pub commitment: CommitmentConfig,
    pub log_progress: bool,
}

/// Returns all the signatures for a given address since a timestamp in seconds.
/// Signatures are returned in descending order, from the newest to the oldest.
pub async fn get_signatures_since_time(
    rpc: &RpcClient,
    config: GetSignaturesSinceTimeConfig,
) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
    let mut signatures = rpc
        .get_signatures_for_address_with_config(
            &config.target,
            GetConfirmedSignaturesForAddress2Config {
                limit: Some(1000),
                commitment: Some(config.commitment),
                ..Default::default()
            },
        )
        .await?;

    if signatures.is_empty() {
        return Ok(vec![]);
    }

    let mut oldest_signature = &signatures[signatures.len() - 1];
    let mut oldest_blocktime = oldest_signature.block_time.unwrap();

    while oldest_blocktime > config.since_timestamp_sec {
        if config.log_progress {
            tracing::info!(
                "get_signatures before {:?} {}",
                datetime_from_timestamp_sec(oldest_blocktime),
                oldest_signature.signature
            );
        }

        let prev_signatures = rpc
            .get_signatures_for_address_with_config(
                &config.target,
                GetConfirmedSignaturesForAddress2Config {
                    before: Some(Signature::from_str(&oldest_signature.signature).unwrap()),
                    limit: Some(1000),
                    commitment: Some(config.commitment),
                    ..Default::default()
                },
            )
            .await?;

        signatures.extend(prev_signatures);
        oldest_signature = &signatures[signatures.len() - 1];
        oldest_blocktime = oldest_signature.block_time.unwrap();
    }

    signatures.retain(|s| s.block_time.unwrap() >= config.since_timestamp_sec);
    if config.ignore_failed {
        signatures.retain(|s| s.err.is_none());
    }

    Ok(signatures)
}

pub struct GetTransactionsFromSignaturesConfig {
    pub batch_size: usize,
    pub signatures: Vec<Signature>,
    pub commitment: CommitmentConfig,
    pub log_progress: bool,
}

pub async fn get_transactions_from_signatures(
    rpc: &RpcClient,
    config: GetTransactionsFromSignaturesConfig,
) -> Result<Vec<EncodedConfirmedTransactionWithStatusMeta>> {
    let n = config.signatures.len();
    let mut current_idx_min = 0;
    let mut current_idx_max = std::cmp::min(config.batch_size, n);

    let mut transactions: Vec<EncodedConfirmedTransactionWithStatusMeta> = vec![];

    while current_idx_max <= n {
        if config.log_progress {
            tracing::info!(
                "current_idx_max {}/{} ({:.2}%)",
                current_idx_max,
                n,
                current_idx_max as f64 / n as f64 * 100.0
            );
        }

        let signatures_batch = config.signatures[current_idx_min..current_idx_max].to_vec();

        let requests = signatures_batch
            .iter()
            .map(|sig| {
                rpc.get_transaction_with_config(
                    sig,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::JsonParsed),
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                )
            })
            .collect::<Vec<_>>();

        for res in futures::future::join_all(requests).await {
            match res {
                Ok(tx) => transactions.push(tx),
                Err(err) => tracing::error!("{}", err),
            }
        }

        if current_idx_max == n {
            break;
        }

        current_idx_min = current_idx_max;
        current_idx_max = std::cmp::min(current_idx_max + config.batch_size, n);
    }

    Ok(transactions)
}
