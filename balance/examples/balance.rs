use std::{collections::HashMap, str::FromStr};

use anyhow::Result;
use solana_sdk::{
    native_token::LAMPORTS_PER_SOL, program_pack::Pack, pubkey::Pubkey, system_program,
};
use solana_trading_util::token::mints_to_associated_token_accounts;
use tracing::info;
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

pub struct GrpcConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
}

pub async fn subscribe_balance_udpates(
    grpc_config: GrpcConfig,
    owner: &Pubkey,
    mints: &Vec<(Pubkey, bool)>,
) -> Result<()> {
    let mut accounts: Vec<Pubkey> = vec![owner.clone()];
    let ata_accounts = mints_to_associated_token_accounts(owner, mints);
    accounts.extend(ata_accounts);

    subscribe_account_udpates(grpc_config, &accounts).await
}

pub async fn subscribe_account_udpates(
    grpc_config: GrpcConfig,
    accounts: &Vec<Pubkey>,
) -> Result<()> {
    let mut client = GeyserGrpcClient::build_from_shared(grpc_config.endpoint)?
        .x_token(grpc_config.x_token)?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;
    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    let mut subscribe_accounts = HashMap::new();
    // filters: vec![SubscribeRequestFilterAccountsFilter {
    //     filter: Some(subscribe_request_filter_accounts_filter::Filter::Datasize(
    //         165,
    //     )),
    // }],
    subscribe_accounts.insert(
        "client".to_owned(),
        SubscribeRequestFilterAccounts {
            nonempty_txn_signature: None,
            account: accounts
                .into_iter()
                .map(|account| account.to_string())
                .collect(),
            owner: vec![],
            filters: vec![],
        },
    );

    subscribe_tx
        .send(SubscribeRequest {
            accounts: subscribe_accounts,
            commitment: Some(CommitmentLevel::Processed as i32),
            ..Default::default()
        })
        .await?;

    futures::try_join!(
        async move {
            let mut timer = interval(Duration::from_secs(3));
            let mut id = 0;
            loop {
                timer.tick().await;
                id += 1;
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id }),
                        ..Default::default()
                    })
                    .await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        },
        async move {
            info!("start listening");
            while let Some(message) = stream.next().await {
                match message?.update_oneof.expect("valid message") {
                    UpdateOneof::Ping(_msg) => {
                        info!("ping received");
                    }
                    UpdateOneof::Pong(SubscribeUpdatePong { id }) => {
                        info!("pong received: id#{id}");
                    }
                    UpdateOneof::Account(SubscribeUpdateAccount { account, .. }) => {
                        if let Some(account) = account {
                            let owner_pubkey = Pubkey::try_from(account.owner.clone()).unwrap();
                            // info!("{:?}", owner_pubkey);

                            if owner_pubkey == system_program::id() {
                                info!(
                                    "native balance update: {:.2}",
                                    account.lamports as f64 / LAMPORTS_PER_SOL as f64
                                );
                            } else if owner_pubkey == spl_token::id() {
                                let account_state = spl_token::state::Account::unpack_from_slice(
                                    account.data.as_slice(),
                                )
                                .unwrap();
                                info!("spl balance update {:?}", account_state);
                            } else if owner_pubkey == spl_token_2022::id() {
                                info!("spl2022 balance update");
                            } else {
                                tracing::warn!("ignore account update {:?}", account);
                            }
                        }
                    }
                    msg => anyhow::bail!("received unexpected message: {msg:?}"),
                }
            }
            Ok::<(), anyhow::Error>(())
        }
    )?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let endpoint = std::env::var("GRPC_ENDPOINT").unwrap();
    let x_token = std::env::var("GRPC_TOKEN").unwrap();
    let wallet = std::env::var("WALLET").unwrap();

    subscribe_balance_udpates(
        GrpcConfig {
            endpoint,
            x_token: Some(x_token),
        },
        &Pubkey::from_str(wallet.as_str()).unwrap(),
        &vec![(
            Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(),
            false,
        )],
    )
    .await
    .unwrap();

    tokio::signal::ctrl_c().await.unwrap();

    Ok(())
}
