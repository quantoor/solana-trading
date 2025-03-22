use anyhow::{anyhow, Result};
use base64::{decode, encode};
use solana_sdk::{bs58, transaction::VersionedTransaction};

pub fn ui_number_to_units(ui_number: f64, decimals: u32) -> u64 {
    (ui_number * 10i32.pow(decimals) as f64) as u64
}

pub fn units_to_ui_number(units: u64, decimals: u32) -> f64 {
    units as f64 / 10i32.pow(decimals) as f64
}

pub fn base64_to_bytes(string_base64: &String) -> Result<Vec<u8>> {
    Ok(decode(string_base64).map_err(|err| anyhow!("base64_to_bytes {}", err))?)
}

pub fn bytes_to_base64(bytes: &Vec<u8>) -> String {
    encode(bytes)
}

pub fn tx_from_bytes(tx_bytes: &Vec<u8>) -> Result<VersionedTransaction> {
    Ok(bincode::deserialize(&tx_bytes).map_err(|err| anyhow!("tx_from_bytes {}", err))?)
}

pub fn tx_to_bytes(tx: &VersionedTransaction) -> Result<Vec<u8>> {
    Ok(bincode::serialize(&tx).map_err(|err| anyhow!("tx_to_bytes {}", err))?)
}

pub fn tx_from_base64(tx_base64: &String) -> Result<VersionedTransaction> {
    let tx_bytes = base64_to_bytes(tx_base64)?;
    Ok(tx_from_bytes(&tx_bytes)?)
}

pub fn tx_to_base64(tx: &VersionedTransaction) -> Result<String> {
    let tx_bytes = tx_to_bytes(tx)?;
    Ok(bytes_to_base64(&tx_bytes))
}

pub fn tx_from_base58(tx_base58: &String) -> Result<VersionedTransaction> {
    let tx_bytes = bs58::decode(tx_base58)
        .into_vec()
        .map_err(|err| anyhow!("tx_from_base58 {}", err))?;
    tx_from_bytes(&tx_bytes)
}

pub fn tx_to_base58(tx: &VersionedTransaction) -> Result<String> {
    let tx_bytes = tx_to_bytes(tx)?;
    Ok(bs58::encode(tx_bytes).into_string())
}
