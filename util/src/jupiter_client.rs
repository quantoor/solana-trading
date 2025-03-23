use anyhow::{anyhow, Result};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    Client,
};
use solana_sdk::pubkey::Pubkey;
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum SwapMode {
    ExactIn,
    ExactOut,
}

impl ToString for SwapMode {
    fn to_string(&self) -> String {
        match self {
            SwapMode::ExactIn => "ExactIn".to_string(),
            SwapMode::ExactOut => "ExactOut".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GetQuoteParams {
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub amount_in: u64,
    pub slippage_bps: u64,
    pub timeout: Duration,
    pub swap_mode: SwapMode,
    pub dexes: Option<String>,
    pub exclude_dexes: Option<String>,
    pub restrict_intermediate_tokens: bool,
    pub only_direct_routes: bool,
}

impl Default for GetQuoteParams {
    fn default() -> Self {
        Self {
            input_mint: Pubkey::default(),
            output_mint: Pubkey::default(),
            amount_in: 0,
            slippage_bps: 0,
            timeout: Duration::from_secs(3),
            swap_mode: SwapMode::ExactIn,
            dexes: None,
            exclude_dexes: None,
            restrict_intermediate_tokens: false,
            only_direct_routes: false,
        }
    }
}

#[derive(serde::Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetSwapParams {
    pub user_public_key: String,
    pub wrap_and_unwrap_sol: bool,
    pub use_shared_accounts: bool,
    pub compute_unit_price_micro_lamports: u64,
    pub as_legacy_transaction: bool,
    pub use_token_ledger: bool,
    pub dynamic_compute_unit_limit: bool,
    pub skip_user_accounts_rpc_calls: bool,
    pub quote_response: QuoteResponse,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponse {
    pub input_mint: String,
    pub in_amount: String,
    pub output_mint: String,
    pub out_amount: String,
    pub other_amount_threshold: String,
    pub swap_mode: String,
    pub slippage_bps: u64,
    pub platform_fee: Option<f64>,
    pub price_impact_pct: String,
    pub route_plan: Vec<RoutePlanStep>,
    pub context_slot: u64,
    pub time_taken: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RoutePlanStep {
    pub swap_info: SwapInfo,
    pub percent: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    pub amm_key: String,
    pub label: String,
    pub input_mint: String,
    pub output_mint: String,
    pub in_amount: String,
    pub out_amount: String,
    pub fee_amount: String,
    pub fee_mint: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    pub swap_transaction: String,
    pub last_valid_block_height: u64,
    pub prioritization_fee_lamports: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SwapInstructionsResponse {
    pub token_ledger_instruction: Option<JupiterInstruction>,
    pub compute_budget_instructions: Vec<JupiterInstruction>,
    pub setup_instructions: Vec<JupiterInstruction>,
    pub swap_instruction: JupiterInstruction,
    pub cleanup_instruction: Option<Vec<JupiterInstruction>>,
    pub other_instructions: Vec<JupiterInstruction>,
    pub address_lookup_table_addresses: Vec<String>,
    pub prioritization_fee_lamports: u64,
    pub compute_unit_limit: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JupiterInstruction {
    pub program_id: String,
    pub accounts: Vec<JupiterAccountMeta>,
    pub data: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct JupiterAccountMeta {
    pub pubkey: String,
    pub is_signer: bool,
    pub is_writable: bool,
}

pub struct JupiterClient {
    url: String,
    client: Client,
}

impl JupiterClient {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: Client::new(),
        }
    }

    pub async fn get_quote(&self, params: GetQuoteParams) -> Result<QuoteResponse> {
        let mut query_params = vec![
            ("inputMint", params.input_mint.to_string()),
            ("outputMint", params.output_mint.to_string()),
            ("amount", params.amount_in.to_string()),
            ("slippageBps", params.slippage_bps.to_string()),
            ("swapMode", params.swap_mode.to_string()),
            (
                "restrictIntermediateTokens",
                params.restrict_intermediate_tokens.to_string(),
            ),
            ("onlyDirectRoutes", params.only_direct_routes.to_string()),
        ];
        if let Some(dexes) = params.dexes {
            query_params.push(("dexes", dexes));
        }
        if let Some(exclude_dexes) = params.exclude_dexes {
            query_params.push(("excludeDexes", exclude_dexes));
        }

        let url = format!("{}/quote", self.url);
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .query(&query_params)
            .timeout(params.timeout)
            .send()
            .await?;
        let response_string = response.text().await?;
        let quote_response: QuoteResponse =
            serde_json::from_str(&response_string).map_err(|err| {
                anyhow!("get_amount_out error parsing response {response_string}: {err}")
            })?;
        Ok(quote_response)
    }

    pub async fn get_swap_transaction(&self, params: GetSwapParams) -> Result<SwapResponse> {
        let response = self
            .client
            .post(&format!("{}/swap", self.url))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&params)
            .send()
            .await?;
        let response_string = response.text().await?;
        let swap_response: SwapResponse =
            serde_json::from_str(&response_string).map_err(|err| {
                anyhow!("get_swap_transaction error parsing response {response_string}: {err}")
            })?;
        Ok(swap_response)
    }

    pub async fn get_swap_instructions(
        &self,
        params: GetSwapParams,
    ) -> Result<SwapInstructionsResponse> {
        let response = self
            .client
            .post(&format!("{}/swap-instructions", self.url))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&params)
            .send()
            .await?;
        let response_string = response.text().await?;
        let swap_response: SwapInstructionsResponse = serde_json::from_str(&response_string)
            .map_err(|err| {
                anyhow!("get_swap_instructions error parsing response {response_string}: {err}")
            })?;
        Ok(swap_response)
    }
}
