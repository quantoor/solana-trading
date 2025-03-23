use anyhow::Result;
use solana_sdk::{program_pack::Pack, pubkey::Pubkey, system_program};
use solana_trading_util::token::mints_to_associated_token_accounts;
use tokio::sync::mpsc;
use tracing::{error, info};
use yellowstone_grpc_proto::geyser::{SubscribeRequestFilterAccounts, SubscribeUpdateAccount};
use {
    futures::{sink::SinkExt, stream::StreamExt},
    tokio::time::{interval, Duration},
    tonic::transport::channel::ClientTlsConfig,
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::prelude::{
        subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest, SubscribeRequestPing,
        SubscribeUpdatePong,
    },
};

#[derive(Debug)]
pub struct BalanceUpdate {
    pub is_native: bool,
    pub pubkey: Pubkey,
    pub mint: Option<Pubkey>,
    pub amount: u64,
    pub slot: u64,
}

pub struct GrpcConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
}

/// Subscribe to the native balance and SPL balances belonging to an owner
pub async fn subscribe_balance_udpates_by_owner(
    grpc_config: GrpcConfig,
    owner: &Pubkey,
    mints: &Vec<(Pubkey, bool)>,
) -> Result<mpsc::Receiver<BalanceUpdate>> {
    let mut accounts: Vec<Pubkey> = vec![owner.clone()];
    let ata_accounts = mints_to_associated_token_accounts(owner, mints);
    accounts.extend(ata_accounts);

    subscribe_balance_udpates(grpc_config, &accounts).await
}

pub async fn subscribe_balance_udpates(
    grpc_config: GrpcConfig,
    accounts: &Vec<Pubkey>,
) -> Result<mpsc::Receiver<BalanceUpdate>> {
    let mut client = GeyserGrpcClient::build_from_shared(grpc_config.endpoint)?
        .x_token(grpc_config.x_token)?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;
    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    subscribe_tx
        .send(SubscribeRequest {
            accounts: maplit::hashmap! {
                "client".to_owned() => SubscribeRequestFilterAccounts {
                    nonempty_txn_signature: None,
                    account: accounts
                        .into_iter()
                        .map(|account| account.to_string())
                        .collect(),
                    owner: vec![],
                    filters: vec![],
                }
            },
            commitment: Some(CommitmentLevel::Processed as i32),
            ..Default::default()
        })
        .await?;

    let (tx, rx) = mpsc::channel::<BalanceUpdate>(1024 * 1024);

    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(3));
        let mut id = 0;
        loop {
            timer.tick().await;
            id += 1;
            if let Err(err) = subscribe_tx
                .send(SubscribeRequest {
                    ping: Some(SubscribeRequestPing { id }),
                    ..Default::default()
                })
                .await
            {
                error!(error = %err, "could not send ping");
            }
        }
    });

    tokio::spawn(async move {
        info!("start listening");
        while let Some(message) = stream.next().await {
            match message?.update_oneof.expect("valid message") {
                UpdateOneof::Ping(_msg) => {
                    info!("ping received");
                }
                UpdateOneof::Pong(SubscribeUpdatePong { id }) => {
                    info!("pong received: id#{id}");
                }
                UpdateOneof::Account(SubscribeUpdateAccount { account, slot, .. }) => {
                    if let Some(account) = account {
                        let account_pubkey = Pubkey::try_from(account.pubkey.clone()).unwrap();
                        let owner_pubkey = Pubkey::try_from(account.owner.clone()).unwrap();

                        let balance_update = if owner_pubkey == system_program::id() {
                            Some(BalanceUpdate {
                                is_native: true,
                                pubkey: account_pubkey,
                                mint: None,
                                amount: account.lamports,
                                slot,
                            })
                        } else if owner_pubkey == spl_token::id() {
                            let account_state = spl_token::state::Account::unpack_from_slice(
                                account.data.as_slice(),
                            )
                            .unwrap();
                            Some(BalanceUpdate {
                                is_native: false,
                                pubkey: account_pubkey,
                                mint: Some(account_state.mint),
                                amount: account_state.amount,
                                slot,
                            })
                        } else if owner_pubkey == spl_token_2022::id() {
                            let account_state = spl_token_2022::state::Account::unpack_from_slice(
                                account.data.as_slice(),
                            )
                            .unwrap();
                            Some(BalanceUpdate {
                                is_native: false,
                                pubkey: account_pubkey,
                                mint: Some(account_state.mint),
                                amount: account_state.amount,
                                slot,
                            })
                        } else {
                            tracing::warn!("ignore account update {:?}", account);
                            None
                        };

                        if let Some(balance_update) = balance_update {
                            if let Err(err) = tx.send(balance_update).await {
                                error!("send error: {}", err);
                            }
                        }
                    };
                }
                msg => anyhow::bail!("received unexpected message: {msg:?}"),
            }
        }
        Ok(())
    });

    Ok(rx)
}
