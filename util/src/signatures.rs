use std::str::FromStr;

use anyhow::Result;
use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::RpcTransactionConfig, rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_trading_core::time::datetime_from_timestamp_sec;
use solana_transaction_status::{EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding};

pub struct GetSignaturesSinceTimeConfig {
    pub ignore_failed: bool,
    pub limit: usize,
    pub commitment: CommitmentConfig,
    pub log_progress: bool,
}

impl Default for GetSignaturesSinceTimeConfig {
    fn default() -> Self {
        Self {
            ignore_failed: false,
            commitment: CommitmentConfig::finalized(),
            log_progress: false,
            limit: 1000,
        }
    }
}

pub struct GetTransactionsFromSignaturesConfig {
    pub batch_size: usize,
    pub encoding: UiTransactionEncoding,
    pub commitment: CommitmentConfig,
    pub log_progress: bool,
}

impl Default for GetTransactionsFromSignaturesConfig {
    fn default() -> Self {
        Self {
            batch_size: 1,
            encoding: UiTransactionEncoding::JsonParsed,
            commitment: CommitmentConfig::finalized(),
            log_progress: false,
        }
    }
}

/// Returns all the signatures for a given address since a timestamp in seconds.
/// Signatures are returned in descending order, from the newest to the oldest.
pub async fn get_signatures_since_time(
    rpc: &RpcClient,
    target: Pubkey,
    since_timestamp_sec: i64,
    config: GetSignaturesSinceTimeConfig,
) -> Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
    let mut signatures = rpc
        .get_signatures_for_address_with_config(
            &target,
            GetConfirmedSignaturesForAddress2Config {
                limit: Some(config.limit),
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

    while oldest_blocktime > since_timestamp_sec {
        if config.log_progress {
            tracing::info!(
                "get_signatures before {:?} {}",
                datetime_from_timestamp_sec(oldest_blocktime),
                oldest_signature.signature
            );
        }

        let prev_signatures = rpc
            .get_signatures_for_address_with_config(
                &target,
                GetConfirmedSignaturesForAddress2Config {
                    before: Some(Signature::from_str(&oldest_signature.signature).unwrap()),
                    limit: Some(config.limit),
                    commitment: Some(config.commitment),
                    ..Default::default()
                },
            )
            .await?;

        signatures.extend(prev_signatures);
        oldest_signature = &signatures[signatures.len() - 1];
        oldest_blocktime = oldest_signature.block_time.unwrap();
    }

    signatures.retain(|s| s.block_time.unwrap() >= since_timestamp_sec);
    if config.ignore_failed {
        signatures.retain(|s| s.err.is_none());
    }

    Ok(signatures)
}

pub async fn get_transactions_from_signatures(
    rpc: &RpcClient,
    signatures: Vec<Signature>,
    config: GetTransactionsFromSignaturesConfig,
) -> Result<Vec<EncodedConfirmedTransactionWithStatusMeta>> {
    let n = signatures.len();
    let mut current_idx_min = 0;
    let mut current_idx_max = std::cmp::min(config.batch_size, n);

    let mut transactions: Vec<EncodedConfirmedTransactionWithStatusMeta> =
        Vec::with_capacity(signatures.len());

    while current_idx_max <= n {
        if config.log_progress {
            tracing::info!(
                "current_idx_max {}/{} ({:.2}%)",
                current_idx_max,
                n,
                current_idx_max as f64 / n as f64 * 100.0
            );
        }

        let signatures_batch = signatures[current_idx_min..current_idx_max].to_vec();

        let requests = signatures_batch
            .iter()
            .map(|sig| {
                rpc.get_transaction_with_config(
                    sig,
                    RpcTransactionConfig {
                        encoding: Some(config.encoding),
                        commitment: Some(config.commitment),
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
