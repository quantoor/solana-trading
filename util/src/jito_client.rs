use anyhow::{anyhow, Result};
use rand::Rng;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::system_instruction::transfer;
use solana_sdk::transaction::VersionedTransaction;
use solana_trading_core::conversions::tx_to_base58;
use std::time::Duration;
use tracing::{error, info};

#[derive(Serialize)]
struct JitoRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<Vec<String>>,
}

impl JitoRequest {
    pub fn new(method: String, params: Vec<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: 1,
            method,
            params: vec![params],
        }
    }
}

#[derive(Serialize)]
struct JitoRequestTx {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<String>,
}

impl JitoRequestTx {
    pub fn new(method: String, params: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: 1,
            method,
            params: vec![params],
        }
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct JitoResponse<T> {
    jsonrpc: String,
    id: u64,
    result: Option<T>,
    error: Option<JitoResponseError>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct JitoResponseError {
    code: i64,
    message: String,
    // data: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct JitoResponseContextValue<T> {
    context: Context,
    value: Vec<T>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct Context {
    slot: u64,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct GetInflightBundleStatusesResponse {
    bundle_id: String,
    status: String,
    landed_slot: Option<u64>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct GetBundleStatusesResponse {
    bundle_id: String,
    transactions: Vec<String>,
    slot: u64,
    confirmation_status: String,
}

pub struct JitoClient {
    url: String,
    uuid: Option<String>,
    client: Client,
}

impl JitoClient {
    pub fn new(url: &String, uuid: Option<String>) -> Result<Self> {
        Ok(Self {
            url: url.clone(),
            uuid,
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    pub async fn send_bundle(&self, bundle: &Vec<VersionedTransaction>) -> Result<String> {
        let bundle_base_58: Vec<Result<String>> =
            bundle.into_iter().map(|tx| tx_to_base58(tx)).collect();
        let bundle_base_58: Result<Vec<String>> = bundle_base_58.into_iter().collect();

        let data = JitoRequest::new("sendBundle".into(), bundle_base_58?);
        let mut url = format!("{}/api/v1/bundles", self.url);
        if let Some(uuid) = self.uuid.clone() {
            url = format!("{}?uuid={}", url, uuid);
        }
        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&data)
            .send()
            .await?;
        let response_string = response.text().await?;
        let swap_response: JitoResponse<String> =
            serde_json::from_str(&response_string).map_err(|err| {
                anyhow!("send_bundle_base_58 error parsing response {response_string}: {err}")
            })?;
        match swap_response.error {
            Some(err) => return Err(anyhow!("send_bundle_base_58 {:?}", err)),
            None => {
                return Ok(swap_response.result.unwrap());
            }
        };
    }

    pub async fn send_transaction(&self, tx: &VersionedTransaction) -> Result<String> {
        let encoded_tx_base58 = tx_to_base58(tx)?;
        let data = JitoRequestTx::new("sendTransaction".into(), encoded_tx_base58);
        let mut url = format!("{}/api/v1/transactions?bundleOnly=true", self.url);
        if let Some(uuid) = self.uuid.clone() {
            url = format!("{}&uuid={}", url, uuid);
        }
        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&data)
            .send()
            .await?;
        let headers = response.headers().clone();
        let bundle_id = headers
            .get("x-bundle-id")
            .ok_or(anyhow!("x-bundle-id header not found"))?;
        let response_string = response.text().await?;
        let swap_response: JitoResponse<String> =
            serde_json::from_str(&response_string).map_err(|err| {
                anyhow!("send_transaction error parsing response {response_string}: {err}")
            })?;
        match swap_response.error {
            Some(err) => return Err(anyhow!("send_transaction {:?}", err)),
            None => {
                return Ok(bundle_id.to_str().unwrap_or_default().to_string());
            }
        };
    }

    pub async fn confirm_bundle_id(
        &self,
        rpc_client: &RpcClient,
        bundle_id: &String,
        last_valid_block_height: u64,
        poll_period: Duration,
    ) -> Result<()> {
        loop {
            let current_block_height = rpc_client
                .get_block_height()
                .await
                .map_err(|err| anyhow!("get_block_height: {}", err))?;
            if current_block_height > last_valid_block_height {
                return Err(anyhow!("bundle expired"));
            }

            match self.get_bundle_status_with_retry(bundle_id).await {
                Ok(response_string) => {
                    let status_response: serde_json::Result<
                        JitoResponse<JitoResponseContextValue<GetInflightBundleStatusesResponse>>,
                    > = serde_json::from_str(&response_string);
                    match status_response {
                        Ok(status_response) => {
                            if let Some(err) = status_response.error {
                                error!("status_response: {:?}", err);
                                break;
                            };

                            let status = status_response
                                .result
                                .unwrap()
                                .value
                                .get(0)
                                .unwrap()
                                .status
                                .clone();
                            match status.as_str() {
                                "Invalid" => {
                                    info!("Bundle {} Invalid", bundle_id);
                                }
                                "Pending" => {
                                    info!("Bundle {} Pending", bundle_id);
                                }
                                "Failed" => {
                                    anyhow::bail!("Bundle {} Failed", bundle_id)
                                }
                                "Landed" => {
                                    info!("Bundle {} Landed", bundle_id);
                                    break;
                                }
                                _ => {
                                    anyhow::bail!(
                                        "Unrecognized bundle status {status} for bundle id {bundle_id}"
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            anyhow::bail!(
                                "confirm_bundle error parsing response {response_string}: {err}"
                            );
                        }
                    }
                }
                Err(err) => {
                    anyhow::bail!("get_bundle_status_with_retry: {}", err);
                }
            }

            tokio::time::sleep(poll_period).await;
        }

        Ok(())
    }

    async fn get_bundle_status_with_retry(&self, bundle_id: &String) -> Result<String> {
        let request = JitoRequest::new("getInflightBundleStatuses".into(), vec![bundle_id.clone()]);
        let mut url = format!("{}/api/v1/bundles", self.url);
        if let Some(uuid) = self.uuid.clone() {
            url = format!("{}&uuid={}", url, uuid);
        }

        let mut retry_count = 1;
        loop {
            match self
                .client
                .post(&url)
                .header(CONTENT_TYPE, "application/json")
                .header(ACCEPT, "application/json")
                .json(&request)
                .send()
                .await
            {
                Ok(res) => match res.text().await {
                    Ok(res_text) => {
                        return Ok(res_text);
                    }
                    Err(err) => {
                        error!("getInflightBundleStatuses res.text(): {err}");
                    }
                },
                Err(err) => {
                    error!("getInflightBundleStatuses: {err}");
                }
            }

            retry_count += 1;
            if retry_count == 3 {
                anyhow::bail!("getInflightBundleStatuses: max retry reached");
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    pub fn get_jito_tip_instruction(payer: &Pubkey, lamports: u64) -> Instruction {
        let tip_account = get_random_tip_account();
        transfer(payer, &tip_account, lamports)
    }
}

fn get_random_tip_account() -> Pubkey {
    let tip_accounts = vec![
        pubkey!("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5"),
        pubkey!("HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe"),
        pubkey!("Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY"),
        pubkey!("ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49"),
        pubkey!("DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh"),
        pubkey!("ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt"),
        pubkey!("DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL"),
        pubkey!("3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT"),
    ];
    let idx = random_in_range(0, tip_accounts.len() as i32 - 1);
    tip_accounts.get(idx as usize).unwrap().clone()
}

fn random_in_range(min: i32, max: i32) -> i32 {
    if min >= max {
        panic!("min should be lower than max")
    }
    let mut rng = rand::thread_rng();
    rng.gen_range(min..=max)
}
