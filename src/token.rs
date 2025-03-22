use std::collections::HashMap;

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use spl_associated_token_account::get_associated_token_address_with_program_id;

pub fn mint_to_associated_token_account(
    owner: &Pubkey,
    mint: &Pubkey,
    is_token_2022: bool, // FIXME remove
) -> Pubkey {
    let program_id = if is_token_2022 {
        solana_sdk::pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
    } else {
        solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
    };
    get_associated_token_address_with_program_id(owner, &mint, &program_id)
}

pub fn mints_to_associated_token_accounts(
    owner: &Pubkey,
    mints: &Vec<(Pubkey, bool)>,
) -> Vec<Pubkey> {
    mints
        .into_iter()
        .map(|(mint, is_token_2022)| mint_to_associated_token_account(owner, mint, *is_token_2022))
        .collect()
}

pub async fn get_spl_balances(
    rpc: &RpcClient,
    owner: &Pubkey,
    mints: &Vec<(Pubkey, bool)>,
) -> HashMap<Pubkey, Result<f64>> {
    let atas = mints_to_associated_token_accounts(owner, mints);

    let requests: Vec<_> = atas
        .iter()
        .map(|i| rpc.get_token_account_balance_with_commitment(&i, CommitmentConfig::confirmed()))
        .collect();

    let responses: Vec<_> = futures::future::join_all(requests).await;

    let mut balances: HashMap<Pubkey, Result<f64>> = HashMap::new();
    for (idx, response) in responses.iter().enumerate() {
        let mint = mints[idx].0;
        let balance_result = match response {
            Ok(res) => res
                .value
                .ui_amount
                .ok_or(anyhow!("ui amount not found for {}", mint)),
            Err(err) => Err(anyhow!("error getting balance for {}: {}", mint, err)),
        };
        balances.insert(mint, balance_result);
    }

    balances
}
