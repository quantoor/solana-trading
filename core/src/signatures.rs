use std::str::FromStr;

use solana_client::{
    nonblocking::rpc_client::RpcClient, rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_response::RpcConfirmedTransactionStatusWithSignature,
};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};

use crate::time::datetime_from_timestamp_sec;

pub struct GetSignaturesSinceTimeConfig {
    pub target: Pubkey,
    pub since_timestamp_sec: i64,
    pub ignore_failed: bool,
    pub commitment: CommitmentConfig,
}

/// Returns all the signatures for a given address since a timestamp in seconds.
/// Signatures are returned in descending order, from the newest to the oldest.
pub async fn get_signatures_since_time(
    rpc_client: &RpcClient,
    config: GetSignaturesSinceTimeConfig,
) -> anyhow::Result<Vec<RpcConfirmedTransactionStatusWithSignature>> {
    let mut signatures = rpc_client
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
        tracing::debug!(
            "get_signatures before {:?} {}",
            datetime_from_timestamp_sec(oldest_blocktime),
            oldest_signature.signature
        );

        let prev_signatures = rpc_client
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
