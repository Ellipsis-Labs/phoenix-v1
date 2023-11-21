use borsh::BorshSerialize;
use ellipsis_client::program_test::*;
use ellipsis_client::EllipsisClient;
use itertools::Itertools;
use phoenix::phoenix_log_authority;
use phoenix::program::deposit::DepositParams;
use phoenix::program::instruction_builders::*;
use phoenix::program::new_order::CondensedOrder;
use phoenix::program::new_order::FailedMultipleLimitOrderBehavior;
use phoenix::program::new_order::MultipleOrderPacket;
use phoenix::program::MarketHeader;
use phoenix::quantities::Ticks;
use phoenix::quantities::WrapperU64;
use phoenix::quantities::{BaseLots, QuoteLots};
use phoenix_sdk::sdk_client::MarketEventDetails;
use phoenix_sdk::sdk_client::MarketMetadata;
use phoenix_sdk::sdk_client::Reduce;
use sokoban::ZeroCopy;
use solana_program::instruction::AccountMeta;
use solana_program::instruction::Instruction;
use solana_program::system_instruction::{self, transfer};
use solana_sdk::compute_budget::ComputeBudgetInstruction;
use spl_associated_token_account::get_associated_token_address;
use std::collections::HashSet;
use std::mem::size_of;

use phoenix::program::status::{MarketStatus, SeatApprovalStatus};
use phoenix::program::*;
use phoenix::state::*;
use phoenix_sdk::sdk_client::SDKClient;

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};

pub mod helpers;
use crate::helpers::*;

const BOOK_SIZE: usize = 4096;
const NUM_SEATS: usize = 8193;

pub struct PhoenixTestAccount {
    pub user: Keypair,
    pub base_ata: Pubkey,
    pub quote_ata: Pubkey,
}

pub struct PhoenixTestClient {
    ctx: ProgramTestContext,
    sdk: SDKClient,
    market: Pubkey,
    meta: MarketMetadata,
}

pub struct PhoenixTestContext {
    admin: Keypair,
    mint_authority: Keypair,
    default_maker: PhoenixTestAccount,
    default_taker: PhoenixTestAccount,
}

pub fn phoenix_test() -> ProgramTest {
    ProgramTest::new("phoenix", phoenix::id(), None)
}

async fn setup_account(
    client: &EllipsisClient,
    authority: &Keypair,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    base_amount: u64,
    quote_amount: u64,
) -> PhoenixTestAccount {
    // initialize user and ATAs
    let user = Keypair::new();
    let base_ata =
        create_associated_token_account(client, &user.pubkey(), &base_mint, &spl_token::id())
            .await
            .unwrap();
    let quote_ata =
        create_associated_token_account(client, &user.pubkey(), &quote_mint, &spl_token::id())
            .await
            .unwrap();

    // airdrop SOL to user
    airdrop(client, &user.pubkey(), sol(10.0)).await.unwrap();

    // airdrop base and quote tokens to user
    mint_tokens(
        client,
        authority,
        &base_mint,
        &base_ata,
        base_amount * 1e9 as u64,
        None,
    )
    .await
    .unwrap();

    mint_tokens(
        client,
        authority,
        &quote_mint,
        &quote_ata,
        quote_amount * 1e6 as u64,
        None,
    )
    .await
    .unwrap();

    PhoenixTestAccount {
        user,
        base_ata,
        quote_ata,
    }
}

async fn get_token_balance(client: &EllipsisClient, ata: Pubkey) -> u64 {
    get_token_account(client, &ata).await.unwrap().amount
}

async fn bootstrap_default(fees_bps: u16) -> (PhoenixTestClient, PhoenixTestContext) {
    bootstrap_with_parameters(100_000, 1_000, 1_000, 9, 6, fees_bps, None).await
}

async fn bootstrap_with_parameters(
    num_quote_lots_per_quote_unit: u64,
    num_base_lots_per_base_unit: u64,
    tick_size_in_quote_lots_per_base_unit: u64,
    base_decimals: u8,
    quote_decimals: u8,
    fee_bps: u16,
    raw_base_units_per_base_unit: Option<u32>,
) -> (PhoenixTestClient, PhoenixTestContext) {
    let context = phoenix_test().start_with_context().await;
    let mut ellipsis_client = EllipsisClient::from_banks(&context.banks_client, &context.payer)
        .await
        .unwrap();
    let authority = Keypair::new();
    ellipsis_client.add_keypair(&authority);
    airdrop(&ellipsis_client, &authority.pubkey(), sol(10.0))
        .await
        .unwrap();
    let market = Keypair::new();
    let params = MarketSizeParams {
        bids_size: BOOK_SIZE as u64,
        asks_size: BOOK_SIZE as u64,
        num_seats: NUM_SEATS as u64,
    };

    // create base and quote token mints
    let base_mint = Keypair::new();
    create_mint(
        &ellipsis_client,
        &authority.pubkey(),
        Some(&authority.pubkey()),
        base_decimals,
        Some(clone_keypair(&base_mint)),
    )
    .await
    .unwrap();

    let quote_mint = Keypair::new();
    create_mint(
        &ellipsis_client,
        &authority.pubkey(),
        Some(&authority.pubkey()),
        quote_decimals,
        Some(clone_keypair(&quote_mint)),
    )
    .await
    .unwrap();

    // initialize default maker and taker accounts
    let maker = setup_account(
        &ellipsis_client,
        &authority,
        base_mint.pubkey(),
        quote_mint.pubkey(),
        1_000_000,
        1_000_000,
    )
    .await;
    let taker = setup_account(
        &ellipsis_client,
        &authority,
        base_mint.pubkey(),
        quote_mint.pubkey(),
        1_000_000,
        1_000_000,
    )
    .await;

    ellipsis_client.add_keypair(&maker.user);
    ellipsis_client.add_keypair(&taker.user);
    let payer = Keypair::from_bytes(&ellipsis_client.payer.to_bytes()).unwrap();

    create_associated_token_account(
        &ellipsis_client,
        &payer.pubkey(),
        &quote_mint.pubkey(),
        &spl_token::id(),
    )
    .await
    .unwrap();

    let mut init_instructions = vec![];

    init_instructions.extend_from_slice(
        &create_initialize_market_instructions_default(
            &market.pubkey(),
            &base_mint.pubkey(),
            &quote_mint.pubkey(),
            &payer.pubkey(),
            params,
            num_quote_lots_per_quote_unit,
            num_base_lots_per_base_unit,
            tick_size_in_quote_lots_per_base_unit,
            fee_bps,
            raw_base_units_per_base_unit,
        )
        .unwrap(),
    );
    init_instructions.push(create_change_market_status_instruction(
        &payer.pubkey(),
        &market.pubkey(),
        MarketStatus::Active,
    ));

    ellipsis_client
        .sign_send_instructions_with_payer(init_instructions, vec![&market])
        .await
        .unwrap();

    // Request seat for maker (by authority)
    ellipsis_client
        .sign_send_instructions(
            vec![create_request_seat_authorized_instruction(
                &ellipsis_client.payer.pubkey(),
                &ellipsis_client.payer.pubkey(),
                &market.pubkey(),
                &maker.user.pubkey(),
            )],
            vec![&ellipsis_client.payer],
        )
        .await
        .unwrap();

    ellipsis_client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &ellipsis_client.payer.pubkey(),
                &market.pubkey(),
                &maker.user.pubkey(),
                SeatApprovalStatus::Approved,
            )],
            vec![&ellipsis_client.payer],
        )
        .await
        .unwrap();
    let mut sdk = SDKClient::new_from_ellipsis_client(ellipsis_client)
        .await
        .unwrap();
    sdk.add_market(&market.pubkey()).await.unwrap();
    let meta = *sdk
        .get_market_metadata_from_cache(&market.pubkey())
        .unwrap();
    (
        PhoenixTestClient {
            ctx: context,
            sdk,
            market: market.pubkey(),
            meta,
        },
        PhoenixTestContext {
            admin: payer,
            mint_authority: authority,
            default_maker: maker,
            default_taker: taker,
        },
    )
}

async fn get_new_maker(
    test_client: &PhoenixTestClient,
    context: &PhoenixTestContext,
    base_amount: u64,
    quote_amount: u64,
) -> PhoenixTestAccount {
    let meta = test_client.meta;

    let maker = setup_account(
        &test_client.sdk.client,
        &context.mint_authority,
        meta.base_mint,
        meta.quote_mint,
        base_amount,
        quote_amount,
    )
    .await;

    // Request seat for maker (by authority)
    test_client
        .sdk
        .client
        .sign_send_instructions(
            vec![
                system_instruction::transfer(
                    &test_client.sdk.client.payer.pubkey(),
                    &get_seat_address(&test_client.market, &maker.user.pubkey()).0,
                    5000,
                ),
                create_request_seat_authorized_instruction(
                    &test_client.sdk.client.payer.pubkey(),
                    &test_client.sdk.client.payer.pubkey(),
                    &test_client.market,
                    &maker.user.pubkey(),
                ),
            ],
            vec![&test_client.sdk.client.payer],
        )
        .await
        .unwrap();

    test_client
        .sdk
        .client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &test_client.sdk.client.payer.pubkey(),
                &test_client.market,
                &maker.user.pubkey(),
                SeatApprovalStatus::Approved,
            )],
            vec![&test_client.sdk.client.payer],
        )
        .await
        .unwrap();

    maker
}

#[tokio::test]
async fn test_phoenix_request_seats() {
    let (phoenix_client, phoenix_ctx) = bootstrap_default(0).await;
    let PhoenixTestClient {
        mut ctx,
        sdk,
        meta,
        market,
    } = phoenix_client;
    let PhoenixTestContext { mint_authority, .. } = &phoenix_ctx;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    // Don't use the default_maker since we are testing the request_seats instruction
    let maker = Keypair::new();
    airdrop(&sdk.client, &maker.pubkey(), sol(20.0))
        .await
        .unwrap();

    let attacker = Keypair::new();
    airdrop(&sdk.client, &attacker.pubkey(), sol(20.0))
        .await
        .unwrap();

    let new_market = Keypair::new();

    let mut init_instructions = vec![];
    init_instructions.extend_from_slice(
        &create_initialize_market_instructions_default(
            &new_market.pubkey(),
            &meta.base_mint,
            &meta.quote_mint,
            &attacker.pubkey(),
            MarketSizeParams {
                bids_size: 512,
                asks_size: 512,
                num_seats: 128,
            },
            1_000_000,
            1000,
            1000,
            0,
            None,
        )
        .unwrap(),
    );

    sdk.client
        .sign_send_instructions_with_payer(init_instructions, vec![&attacker, &new_market])
        .await
        .unwrap();

    // Request seat for attacker
    sdk.client
        .sign_send_instructions(
            vec![create_request_seat_instruction(&attacker.pubkey(), &market)],
            vec![&attacker],
        )
        .await
        .unwrap();

    let mut malicious_claim_seat_instruction = create_change_seat_status_instruction(
        &attacker.pubkey(),
        &new_market.pubkey(),
        &attacker.pubkey(),
        SeatApprovalStatus::Approved,
    );

    malicious_claim_seat_instruction.accounts[4].pubkey =
        get_seat_address(&market, &attacker.pubkey()).0;

    assert!(
        sdk.client
            .sign_send_instructions(vec![malicious_claim_seat_instruction], vec![&attacker])
            .await
            .is_err(),
        "Attacker cannot claim seat for another market"
    );

    // Request seat for maker
    sdk.client
        .sign_send_instructions(
            vec![create_request_seat_instruction(&maker.pubkey(), &market)],
            vec![&maker],
        )
        .await
        .unwrap();

    // Maker cannot approve his own seat
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_seat_status_instruction(
                    &maker.pubkey(),
                    &market,
                    &maker.pubkey(),
                    SeatApprovalStatus::Approved,
                )],
                vec![&maker],
            )
            .await
            .is_err(),
        "Maker cannot approve his own seat"
    );

    // Approve seat for maker
    sdk.client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &sdk.get_trader(),
                &market,
                &maker.pubkey(),
                SeatApprovalStatus::Approved,
            )],
            vec![],
        )
        .await
        .unwrap();

    // Ban maker
    sdk.client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &sdk.get_trader(),
                &market,
                &maker.pubkey(),
                SeatApprovalStatus::Retired,
            )],
            vec![],
        )
        .await
        .unwrap();

    ctx.warp_to_slot(2).unwrap();
    // Maker cannot be unretired
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_seat_status_instruction(
                    &sdk.get_trader(),
                    &market,
                    &maker.pubkey(),
                    SeatApprovalStatus::Approved,
                )],
                vec![],
            )
            .await
            .is_err(),
        "Maker cannot be unretired"
    );

    // Request seat for maker1 (by authority)
    let PhoenixTestAccount { user: maker1, .. } = setup_account(
        &sdk.client,
        mint_authority,
        *base_mint,
        *quote_mint,
        1_000_000,
        1_000_000,
    )
    .await;
    sdk.client
        .sign_send_instructions(
            vec![create_request_seat_authorized_instruction(
                &sdk.client.payer.pubkey(),
                &sdk.client.payer.pubkey(),
                &market,
                &maker1.pubkey(),
            )],
            vec![],
        )
        .await
        .unwrap();

    // Approve seat for maker1
    sdk.client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &sdk.client.payer.pubkey(),
                &market,
                &maker1.pubkey(),
                SeatApprovalStatus::Approved,
            )],
            vec![],
        )
        .await
        .unwrap();

    // Make an order to get a seat
    let params = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(100.0),
        1,
    );
    sdk.client
        .sign_send_instructions(
            vec![create_new_order_instruction(
                &market,
                &maker1.pubkey(),
                base_mint,
                quote_mint,
                &params,
            )],
            vec![&maker1],
        )
        .await
        .unwrap();

    // Retire seat for maker1
    sdk.client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &sdk.client.payer.pubkey(),
                &market,
                &maker1.pubkey(),
                SeatApprovalStatus::Retired,
            )],
            vec![],
        )
        .await
        .unwrap();

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_evict_seat_instruction(
                    &sdk.client.payer.pubkey(),
                    &market,
                    &maker1.pubkey(),
                    base_mint,
                    quote_mint,
                )],
                vec![],
            )
            .await
            .is_err(),
        "Cannot evict seat with open orders"
    );

    // Cancel all existing orders for maker1
    sdk.client
        .sign_send_instructions(
            create_force_cancel_orders_instructions(
                &market,
                &maker1.pubkey(),
                &sdk.client.payer.pubkey(),
                base_mint,
                quote_mint,
            ),
            vec![],
        )
        .await
        .unwrap();

    // Evict maker1
    sdk.client
        .sign_send_instructions(
            vec![create_evict_seat_instruction(
                &sdk.client.payer.pubkey(),
                &market,
                &maker1.pubkey(),
                base_mint,
                quote_mint,
            )],
            vec![],
        )
        .await
        .unwrap();
}

async fn get_sequence_number(client: &EllipsisClient, market: &Pubkey) -> u64 {
    let market_data = client.get_account(market).await.unwrap().data;
    let (header_bytes, bytes) = market_data.split_at(size_of::<MarketHeader>());
    let header = Box::new(MarketHeader::load_bytes(header_bytes).unwrap());
    let full_market = load_with_dispatch(&header.market_size_params, bytes).unwrap();
    full_market.inner.get_sequence_number()
}

#[tokio::test]
async fn test_phoenix_orders() {
    let (phoenix_client, ctx) = bootstrap_default(0).await;
    let PhoenixTestClient {
        ctx: _,
        sdk,
        meta,
        market,
    } = &phoenix_client;

    let PhoenixTestContext { default_maker, .. } = &ctx;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;
    let mut orders = vec![];

    // Place a bid at 100
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(100.0),
        1,
    );

    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));
    // Place a bid at 99
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(99.0),
        1,
    );

    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));
    // Place an ask at 101
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(101.0),
        1,
    );
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));

    // Place an ask at 102
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(102.0),
        1,
    );
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));

    // Minimum tick price is 1
    let limit_order = OrderPacket::new_limit_order_default(Side::Bid, 0, 1);
    assert!(sdk
        .client
        .sign_send_instructions(
            vec![create_new_order_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &limit_order
            )],
            vec![]
        )
        .await
        .is_err());

    // Minimum tick price is 1
    let limit_order = OrderPacket::new_limit_order_default(Side::Bid, 1, 1);
    assert!(sdk
        .client
        .sign_send_instructions(
            vec![create_new_order_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &limit_order
            )],
            vec![]
        )
        .await
        .is_ok());

    let cancel_orders = vec![CancelOrderParams {
        side: Side::Bid,
        price_in_ticks: 1,
        order_sequence_number: !1, // Cancel the first bid
    }];

    sdk.client
        .sign_send_instructions(
            vec![create_cancel_multiple_orders_by_id_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &CancelMultipleOrdersByIdParams {
                    orders: cancel_orders,
                },
            )],
            vec![],
        )
        .await
        .unwrap();

    sdk.client
        .sign_send_instructions(orders, vec![])
        .await
        .unwrap();

    let base_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    assert_eq!(base_start, 999999998000000);
    assert_eq!(quote_start, 999999801000);

    let sequence_number = get_sequence_number(&sdk.client, market).await;

    let cancel_orders = vec![
        CancelOrderParams {
            side: Side::Ask,
            price_in_ticks: 10200,
            order_sequence_number: sequence_number - 1,
        },
        CancelOrderParams {
            side: Side::Ask,
            price_in_ticks: 10100,
            order_sequence_number: sequence_number - 2,
        },
    ];

    sdk.client
        .sign_send_instructions(
            vec![create_cancel_multiple_orders_by_id_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &CancelMultipleOrdersByIdParams {
                    orders: cancel_orders,
                },
            )],
            vec![],
        )
        .await
        .unwrap();

    let mut base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let mut quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    let new_sequence_number = get_sequence_number(&sdk.client, market).await;

    // maker receives base tokens
    assert_eq!(base_end, 1000000000000000);
    assert_eq!(quote_end, quote_start);

    // sequence number bumped only once
    assert_eq!(new_sequence_number, sequence_number);

    // try to cancel already cancelled orders
    sdk.client
        .sign_send_instructions(
            vec![create_cancel_multiple_orders_by_id_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &CancelMultipleOrdersByIdParams {
                    orders: vec![
                        CancelOrderParams {
                            side: Side::Ask,
                            price_in_ticks: 10200,
                            order_sequence_number: sequence_number - 1,
                        },
                        CancelOrderParams {
                            side: Side::Ask,
                            price_in_ticks: 10100,
                            order_sequence_number: sequence_number - 2,
                        },
                    ],
                },
            )],
            vec![],
        )
        .await
        .unwrap();

    base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    // nothing should be affected
    assert_eq!(base_end, 1000000000000000);
    assert_eq!(quote_end, quote_start);

    sdk.client
        .sign_send_instructions(
            vec![create_cancel_multiple_orders_by_id_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
                &CancelMultipleOrdersByIdParams {
                    orders: vec![
                        // order already cancelled
                        CancelOrderParams {
                            side: Side::Ask,
                            price_in_ticks: 10200,
                            order_sequence_number: sequence_number - 1,
                        },
                        // order does not exist
                        CancelOrderParams {
                            side: Side::Ask,
                            price_in_ticks: 10800,
                            order_sequence_number: sequence_number - 2,
                        },
                        // order on bid
                        CancelOrderParams {
                            side: Side::Bid,
                            price_in_ticks: 9900,
                            order_sequence_number: !(sequence_number - 3),
                        },
                    ],
                },
            )],
            vec![],
        )
        .await
        .unwrap();

    base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    // maker receives quote tokens
    assert_eq!(base_end, 1000000000000000);
    assert_eq!(
        quote_end,
        quote_start + meta.quote_lots_to_quote_atoms(9900)
    );
}

#[tokio::test]
async fn test_phoenix_cancel_all_orders() {
    let (mut phoenix_test_client, phoenix_ctx) = bootstrap_default(0).await;
    let PhoenixTestContext { default_maker, .. } = &phoenix_ctx;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut phoenix_test_client;
    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    let mut orders = vec![];
    let payer_key = sdk.client.payer.pubkey();
    sdk.set_payer(clone_keypair(&default_maker.user));
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(100.0),
        1,
    );
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));
    // Place a bid at 99
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(99.0),
        1,
    );

    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));
    // Place an ask at 101
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(101.0),
        1,
    );
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));

    // Place an ask at 102
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(102.0),
        1,
    );
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    ));

    sdk.client.set_payer(&payer_key).unwrap();
    let sequence_number = get_sequence_number(&sdk.client, market).await;

    sdk.client
        .sign_send_instructions(
            vec![create_cancel_all_orders_instruction(
                market,
                &default_maker.user.pubkey(),
                base_mint,
                quote_mint,
            )],
            vec![],
        )
        .await
        .unwrap();

    let base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    let new_sequence_number = get_sequence_number(&sdk.client, market).await;

    // maker receives base tokens
    assert_eq!(base_end, 1000000000000000);
    assert_eq!(quote_end, 1000000000000);

    // sequence number bumped only once
    assert_eq!(new_sequence_number, sequence_number);
}

#[tokio::test]
async fn test_phoenix_admin() {
    let (
        mut phoenix_test_client,
        PhoenixTestContext {
            admin,
            default_maker,
            default_taker,
            ..
        },
    ) = bootstrap_default(5).await;

    let PhoenixTestClient {
        ctx,
        sdk,
        market,
        meta,
        ..
    } = &mut phoenix_test_client;
    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    let mut orders = vec![];

    let payer_key = sdk.client.payer.pubkey();
    sdk.set_payer(clone_keypair(&default_maker.user));
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &OrderPacket::new_limit_order_default(
            Side::Bid,
            meta.float_price_to_ticks_rounded_down(100.0),
            1,
        ),
    ));
    // Place a bid at 99
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &OrderPacket::new_limit_order_default(
            Side::Bid,
            meta.float_price_to_ticks_rounded_down(99.0),
            1,
        ),
    ));
    // Place an ask at 101
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &OrderPacket::new_limit_order_default(
            Side::Ask,
            meta.float_price_to_ticks_rounded_down(101.0),
            1,
        ),
    ));

    // Place an ask at 102
    orders.push(create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &OrderPacket::new_limit_order_default(
            Side::Ask,
            meta.float_price_to_ticks_rounded_down(102.0),
            1,
        ),
    ));

    sdk.client
        .sign_send_instructions(orders, vec![])
        .await
        .unwrap();

    sdk.client.set_payer(&payer_key).unwrap();

    let successor = Keypair::new();
    airdrop(&sdk.client, &successor.pubkey(), sol(10.0))
        .await
        .unwrap();

    // Attempt to transfer ownership as a non-admin
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_name_successor_instruction(
                    &successor.pubkey(),
                    &market,
                    &successor.pubkey()
                )],
                vec![&successor],
            )
            .await
            .is_err(),
        "Should not be able to transfer ownership as a non-admin"
    );

    //Attempt to transfer ownership as an admin
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_name_successor_instruction(
                    &admin.pubkey(),
                    &market,
                    &successor.pubkey()
                )],
                vec![&admin],
            )
            .await
            .is_ok(),
        "Should be able to transfer ownership as an admin"
    );

    // Attempt to claim authority as a non-admin
    let attacker = Keypair::new();
    airdrop(&sdk.client, &attacker.pubkey(), sol(10.0))
        .await
        .unwrap();

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_claim_authority_instruction(
                    &attacker.pubkey(),
                    &market
                )],
                vec![&attacker],
            )
            .await
            .is_err(),
        "Should not be able to claim authority if you are not the successor"
    );

    // Attempt to claim authority as the successor
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_claim_authority_instruction(
                    &successor.pubkey(),
                    &market
                )],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Should be able to claim authority if you are the successor"
    );
    let params = OrderPacket::new_ioc_by_lots(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(102.0),
        1,
        SelfTradeBehavior::DecrementTake,
        None,
        0,
        false,
    );
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_new_order_instruction(
                    market,
                    &default_taker.user.pubkey(),
                    base_mint,
                    quote_mint,
                    &params,
                )],
                vec![],
            )
            .await
            .is_ok(),
        "Should be able to trade when market is active"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Closed
                )],
                vec![&successor],
            )
            .await
            .is_err(),
        "Can't close an active market"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &admin.pubkey(),
                    market,
                    MarketStatus::Paused
                )],
                vec![&admin],
            )
            .await
            .is_err(),
        "Previous admin cannot pause an active market"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Paused
                )],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Current admin can pause an active market"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_cancel_up_to_instruction(
                    market,
                    &default_maker.user.pubkey(),
                    base_mint,
                    quote_mint,
                    &CancelUpToParams {
                        side: Side::Bid,
                        tick_limit: None,
                        num_orders_to_cancel: Some(1),
                        num_orders_to_search: None
                    },
                )],
                vec![&default_maker.user],
            )
            .await
            .is_ok(),
        "Should be able to cancel when market is paused",
    );
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Active
                )],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Current admin can reactivate paused market"
    );
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Paused
                )],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Current admin can pause an active market"
    );

    // We need to increment the slot because you cannot send duplicated transactions (same blockhash and same instruction)
    ctx.warp_to_slot(2).unwrap();

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_new_order_instruction(
                    market,
                    &default_taker.user.pubkey(),
                    base_mint,
                    quote_mint,
                    &params,
                )],
                vec![&default_taker.user],
            )
            .await
            .is_err(),
        "Should not be able to trade when market is paused"
    );
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Closed,
                )],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Can close paused market"
    );
    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Tombstoned,
                )],
                vec![&successor],
            )
            .await
            .is_err(),
        "Can't tombstone market with orders"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![
                    create_cancel_up_to_instruction(
                        market,
                        &default_maker.user.pubkey(),
                        base_mint,
                        quote_mint,
                        &CancelUpToParams {
                            side: Side::Bid,
                            tick_limit: None,
                            num_orders_to_cancel: None,
                            num_orders_to_search: None
                        },
                    ),
                    create_cancel_up_to_instruction(
                        market,
                        &default_maker.user.pubkey(),
                        base_mint,
                        quote_mint,
                        &CancelUpToParams {
                            side: Side::Ask,
                            tick_limit: None,
                            num_orders_to_cancel: None,
                            num_orders_to_search: None
                        },
                    ),
                ],
                vec![&default_maker.user],
            )
            .await
            .is_ok(),
        "Should be able to cancel when market is closed",
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![
                    // We need to add this now because to remove a trader, you must explicily
                    // call withdraw
                    create_evict_seat_instruction(
                        &successor.pubkey(),
                        market,
                        &default_maker.user.pubkey(),
                        base_mint,
                        quote_mint,
                    )
                ],
                vec![&successor],
            )
            .await
            .is_err(),
        "Cannot evict seat if the trader's seat is still approved"
    );

    sdk.client
        .sign_send_instructions(
            vec![create_change_seat_status_instruction(
                &successor.pubkey(),
                market,
                &default_maker.user.pubkey(),
                SeatApprovalStatus::NotApproved,
            )],
            vec![&successor],
        )
        .await
        .unwrap();

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![
                    // We need to add this now because to remove a trader, you must explicily
                    // call withdraw
                    create_evict_seat_instruction(
                        &successor.pubkey(),
                        market,
                        &default_maker.user.pubkey(),
                        base_mint,
                        quote_mint,
                    )
                ],
                vec![&successor],
            )
            .await
            .is_ok(),
        "Can evict seat"
    );

    assert!(
        sdk.client
            .sign_send_instructions(
                vec![create_change_market_status_instruction(
                    &successor.pubkey(),
                    market,
                    MarketStatus::Tombstoned
                )],
                vec![&successor],
            )
            .await
            .is_err(),
        "Cannot tombstone closed market with uncollected fees"
    );

    // Collect fees from the market
    sdk.client
        .sign_send_instructions(
            vec![create_collect_fees_instruction_default(
                market,
                &sdk.client.payer.pubkey(),
                &sdk.client.payer.pubkey(), // Fee collector is the market creator in this case
                quote_mint,
            )],
            vec![],
        )
        .await
        .unwrap();

    sdk.client
        .sign_send_instructions(
            vec![create_change_market_status_instruction(
                &successor.pubkey(),
                market,
                MarketStatus::Tombstoned,
            )],
            vec![&successor],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_phoenix_basic() {
    let (mut client, ctx) = bootstrap_default(0).await;
    let PhoenixTestContext {
        default_maker,
        default_taker,
        ..
    } = &ctx;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;
    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));

    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(40.0),
        meta.float_price_to_ticks_rounded_down(36.0),
        meta.float_price_to_ticks_rounded_down(0.05),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(0.5),
        Side::Bid,
    )
    .await;

    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(40.01),
        meta.float_price_to_ticks_rounded_down(45.5),
        meta.float_price_to_ticks_rounded_down(0.05),
        meta.raw_base_units_to_base_lots_rounded_down(1.2),
        meta.raw_base_units_to_base_lots_rounded_down(0.3),
        Side::Ask,
    )
    .await;
    sdk.set_payer(clone_keypair(&default_taker.user));

    let params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(39.7),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::Abort,
        None,
        0,
        false,
    );

    let base_start = get_token_balance(&sdk.client, default_taker.base_ata).await;
    let quote_start = get_token_balance(&sdk.client, default_taker.quote_ata).await;

    let new_order_ix = create_new_order_instruction(
        market,
        &default_taker.user.pubkey(),
        base_mint,
        quote_mint,
        &params,
    );
    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![])
        .await
        .unwrap();
    let base_end = get_token_balance(&sdk.client, default_taker.base_ata).await;
    let quote_end = get_token_balance(&sdk.client, default_taker.quote_ata).await;
    println!("Base start: {}", base_start);
    println!("Quote start: {}", quote_start);
    println!("Base end: {}", base_end);
    println!("Quote end: {}", quote_end);
    assert_eq!(quote_end - quote_start, 398750000);
    assert_eq!(base_start - base_end, 10000000000);

    let base_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    let withdraw_ix = create_withdraw_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
    );
    sdk.client
        .sign_send_instructions(vec![withdraw_ix], vec![])
        .await
        .unwrap();
    let base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    assert_eq!(quote_end - quote_start, 0);
    assert_eq!(base_end - base_start, 10000000000);
    let params = CancelUpToParams {
        side: Side::Bid,
        tick_limit: None,
        num_orders_to_search: None,
        num_orders_to_cancel: None,
    };

    let cancel_multiple_ix = create_cancel_up_to_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &params,
    );
    let market_state = sdk.get_market_state(market).await.unwrap();
    let mut orders = [&market_state.orderbook.bids]
        .iter()
        .flat_map(|ob| {
            ob.iter()
                .map(|(k, v)| (k.order_sequence_number, v.num_base_lots))
        })
        .collect::<HashSet<(u64, u64)>>();

    let sig = sdk
        .client
        .sign_send_instructions(vec![cancel_multiple_ix], vec![])
        .await
        .unwrap();

    let tx_events = sdk.parse_events_from_transaction(&sig).await.unwrap();
    for event in tx_events {
        if let MarketEventDetails::Reduce(Reduce {
            order_sequence_number,
            maker,
            base_lots_removed,
            ..
        }) = event.details
        {
            assert!(orders.remove(&(order_sequence_number, base_lots_removed)));
            assert_eq!(maker, default_maker.user.pubkey());
        } else {
            panic!("Unexpected event: {:?}", event);
        }
    }
    assert!(orders.is_empty());

    let quote_after_cancel = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert!(quote_after_cancel == 1_000_000_000_000 - 398750000);
    let deposit_ix = create_deposit_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 1,
            base_lots_to_deposit: 1,
        },
    );
    sdk.client
        .sign_send_instructions(vec![deposit_ix], vec![])
        .await
        .unwrap();

    let base_after_deposit = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_after_deposit = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert_eq!(
        quote_after_deposit,
        quote_after_cancel - meta.quote_atoms_per_quote_lot
    );
    assert_eq!(base_after_deposit, base_end - meta.base_atoms_per_base_lot);

    let base_before_withdraw = base_after_deposit;
    let quote_before_withdraw = quote_after_deposit;
    let withdraw_ix = create_withdraw_funds_with_custom_amounts_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        1,
        1,
    );
    sdk.client
        .sign_send_instructions(vec![withdraw_ix], vec![])
        .await
        .unwrap();
    let base_after_withdraw = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_after_withdraw = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert_eq!(
        quote_after_withdraw,
        quote_before_withdraw + meta.quote_atoms_per_quote_lot
    );
    assert_eq!(
        base_after_withdraw,
        base_before_withdraw + meta.base_atoms_per_base_lot
    );
}

#[tokio::test]
async fn test_phoenix_fees() {
    let (mut client, ctx) = bootstrap_default(5).await;
    let PhoenixTestContext {
        default_maker,
        default_taker,
        admin,
        mint_authority,
    } = &ctx;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;
    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));
    // Place a bid at 100
    let limit_order = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(100.0),
        1000,
    );
    let make_ix = create_new_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &limit_order,
    );

    sdk.client
        .sign_send_instructions(vec![make_ix], vec![])
        .await
        .unwrap();

    sdk.client.set_payer(&default_taker.user.pubkey()).unwrap();
    let taker_order = OrderPacket::new_ioc_sell_with_limit_price(
        meta.float_price_to_ticks_rounded_down(100.0),
        1000,
        SelfTradeBehavior::Abort,
        None,
        0,
        false,
    );
    let take_ix = create_new_order_instruction(
        market,
        &default_taker.user.pubkey(),
        base_mint,
        quote_mint,
        &taker_order,
    );

    let taker_ata = get_associated_token_address(&default_taker.user.pubkey(), quote_mint);
    let taker_balance_start = get_token_balance(&sdk.client, taker_ata).await;
    sdk.client
        .sign_send_instructions(vec![take_ix], vec![])
        .await
        .unwrap();
    let taker_balance_end = get_token_balance(&sdk.client, taker_ata).await;
    let taker_diff = taker_balance_end - taker_balance_start;
    println!("taker balance change {}", taker_diff);
    sdk.client.set_payer(&admin.pubkey()).unwrap();

    let new_fee_recipient = setup_account(
        &sdk.client,
        mint_authority,
        meta.base_mint,
        meta.quote_mint,
        0,
        0,
    )
    .await;

    let change_fee_recipient_ix = create_change_fee_recipient_instruction(
        &admin.pubkey(),
        market,
        &new_fee_recipient.user.pubkey(),
    );

    assert!(
        sdk.client
            .sign_send_instructions(vec![change_fee_recipient_ix], vec![admin])
            .await
            .is_err(),
        "Cannot change fee recipient if there are unclaimed fees and current fee recipient does not sign"
    );

    let change_fee_recipient_ix = create_change_fee_recipient_with_unclaimed_fees_instruction(
        &admin.pubkey(),
        market,
        &new_fee_recipient.user.pubkey(),
        &admin.pubkey(),
    );

    assert!(
        sdk.client
            .sign_send_instructions(vec![change_fee_recipient_ix], vec![admin])
            .await
            .is_ok(),
        "Fee recipient can be changed if there are unclaimed fees and current fee recipient signs"
    );

    let collect_fees_ix = create_collect_fees_instruction_default(
        market,
        &admin.pubkey(),
        &new_fee_recipient.user.pubkey(),
        quote_mint,
    );
    let fee_ata = get_associated_token_address(&new_fee_recipient.user.pubkey(), quote_mint);
    let fee_dest_start = get_token_balance(&sdk.client, fee_ata).await;
    let quote_vault = get_vault_address(market, quote_mint).0;
    let quote_balance_start = get_token_balance(&sdk.client, quote_vault).await;

    sdk.client
        .sign_send_instructions(vec![collect_fees_ix], vec![])
        .await
        .unwrap();

    let quote_balance_end = get_token_balance(&sdk.client, quote_vault).await;

    let fee_dest_balance = get_token_balance(&sdk.client, fee_ata).await;

    // Verify that the fee is 5 bps of the taker's order
    assert_eq!((50000 + taker_diff) / 50000, 2000);

    assert_eq!(quote_balance_start - quote_balance_end, 50000);
    assert_eq!(quote_balance_end, 0);
    assert_eq!(fee_dest_balance - fee_dest_start, 50000);

    let market_account_data = (sdk.client.get_account_data(market)).await.unwrap();
    let (header_bytes, bytes) = market_account_data.split_at(size_of::<MarketHeader>());
    let header = MarketHeader::load_bytes(header_bytes).unwrap();
    let market_obj = load_with_dispatch(&header.market_size_params, bytes)
        .unwrap()
        .inner;
    assert_eq!(
        market_obj
            .get_registered_traders()
            .get(&default_maker.user.pubkey())
            .unwrap()
            .base_lots_free,
        BaseLots::new(1000)
    );

    let change_fee_recipient_ix =
        create_change_fee_recipient_instruction(&admin.pubkey(), market, &Keypair::new().pubkey());

    assert!(
        sdk.client
            .sign_send_instructions(vec![change_fee_recipient_ix], vec![])
            .await
            .is_ok(),
        "Can change fee recipient if there are no unclaimed fees"
    );
}

#[tokio::test]
async fn test_phoenix_cancel_with_free_funds() {
    let (mut client, ctx) = bootstrap_default(0).await;
    let PhoenixTestContext { default_maker, .. } = &ctx;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;
    sdk.client.set_payer(&default_maker.user.pubkey()).unwrap();
    let quote_lots_to_deposit = meta.quote_units_to_quote_lots(10000.0);
    let base_lots_to_deposit = meta.raw_base_units_to_base_lots_rounded_down(100.0);
    let params = DepositParams {
        quote_lots_to_deposit,
        base_lots_to_deposit,
    };

    let quote_lots = QuoteLots::new(quote_lots_to_deposit);
    let base_lots = BaseLots::new(base_lots_to_deposit);

    let trader = default_maker.user.pubkey();

    sdk.client
        .sign_send_instructions(
            vec![create_deposit_funds_instruction(
                &market,
                &trader,
                &meta.base_mint,
                &meta.quote_mint,
                &params,
            )],
            vec![],
        )
        .await
        .unwrap();

    let market_state = sdk.get_market_state(market).await.unwrap();
    assert!(market_state.traders[&trader].base_lots_free == base_lots.as_u64());
    assert!(market_state.traders[&trader].quote_lots_free == quote_lots.as_u64());

    let order_packet = OrderPacket::new_limit_order(
        Side::Bid,
        100,
        10,
        SelfTradeBehavior::DecrementTake,
        None,
        0,
        true,
    );

    sdk.client
        .sign_send_instructions(
            vec![create_new_order_with_free_funds_instruction(
                &market,
                &trader,
                &order_packet,
            )],
            vec![],
        )
        .await
        .unwrap();

    let market_state = sdk.get_market_state(market).await.unwrap();
    assert!(market_state.traders[&trader].base_lots_free == base_lots.as_u64());
    assert!(!market_state.orderbook.bids.is_empty());
    assert!(
        market_state.traders[&trader].quote_lots_free
            == quote_lots.as_u64()
                - (100 * 10 * meta.tick_size_in_quote_atoms_per_base_unit
                    / (meta.num_base_lots_per_base_unit * meta.quote_atoms_per_quote_lot))
    );

    let mut orders = [&market_state.orderbook.bids, &market_state.orderbook.asks]
        .iter()
        .flat_map(|ob| {
            ob.iter()
                .map(|(k, v)| (k.order_sequence_number, v.num_base_lots))
        })
        .collect::<HashSet<(u64, u64)>>();

    let sig = sdk
        .client
        .sign_send_instructions(
            vec![create_cancel_all_order_with_free_funds_instruction(
                &market, &trader,
            )],
            vec![],
        )
        .await
        .unwrap();

    let tx_events = sdk.parse_events_from_transaction(&sig).await.unwrap();
    for event in tx_events {
        if let MarketEventDetails::Reduce(Reduce {
            order_sequence_number,
            maker,
            base_lots_removed,
            ..
        }) = event.details
        {
            assert!(orders.remove(&(order_sequence_number, base_lots_removed)));
            assert_eq!(maker, trader);
        } else {
            panic!("Unexpected event: {:?}", event);
        }
    }
    assert!(orders.is_empty());

    let market_state = sdk.get_market_state(market).await.unwrap();
    assert!(market_state.orderbook.bids.is_empty());
    assert!(market_state.traders[&trader].base_lots_free == base_lots.as_u64());
    assert!(market_state.traders[&trader].quote_lots_free == quote_lots.as_u64());

    sdk.client
        .sign_send_instructions(
            vec![
                create_new_order_with_free_funds_instruction(&market, &trader, &order_packet),
                create_new_order_with_free_funds_instruction(&market, &trader, &order_packet),
                create_cancel_multiple_orders_by_id_with_free_funds_instruction(
                    &market,
                    &trader,
                    &CancelMultipleOrdersByIdParams {
                        orders: vec![CancelOrderParams {
                            side: Side::Bid,
                            price_in_ticks: 100,
                            order_sequence_number: !2,
                        }],
                    },
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

    let market_state = sdk.get_market_state(market).await.unwrap();
    assert!(!market_state.orderbook.bids.is_empty());
    assert!(market_state.traders[&trader].base_lots_free == base_lots.as_u64());
    assert!(
        market_state.traders[&trader].quote_lots_free
            == quote_lots.as_u64()
                - (100 * 10 * meta.tick_size_in_quote_atoms_per_base_unit
                    / (meta.quote_atoms_per_quote_lot * meta.num_base_lots_per_base_unit))
    );
    sdk.client
        .sign_send_instructions(
            vec![
                create_new_order_with_free_funds_instruction(&market, &trader, &order_packet),
                create_cancel_up_to_with_free_funds_instruction(
                    &market,
                    &trader,
                    &CancelUpToParams {
                        side: Side::Bid,
                        tick_limit: None,
                        num_orders_to_cancel: None,
                        num_orders_to_search: None,
                    },
                ),
            ],
            vec![],
        )
        .await
        .unwrap();

    let market_state = sdk.get_market_state(market).await.unwrap();
    assert!(market_state.orderbook.bids.is_empty());
    assert!(market_state.traders[&trader].base_lots_free == base_lots.as_u64());
    assert!(market_state.traders[&trader].quote_lots_free == quote_lots.as_u64());
}

#[tokio::test]
async fn test_phoenix_orders_with_free_funds() {
    let (mut client, ctx) = bootstrap_default(0).await;
    let PhoenixTestContext {
        default_maker,
        default_taker,
        ..
    } = &ctx;
    let second_maker = get_new_maker(&client, &ctx, 1_000_000, 1_000_000).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));

    let base_balance_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(40.0),
        meta.float_price_to_ticks_rounded_down(30.0),
        meta.float_price_to_ticks_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        Side::Bid,
    )
    .await;

    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(50.0),
        meta.float_price_to_ticks_rounded_down(60.0),
        meta.float_price_to_ticks_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        Side::Ask,
    )
    .await;
    sdk.set_payer(clone_keypair(&default_taker.user));

    //Attempt to use free funds to trade, will reject because the taker has no seat
    let sell_params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(31.0),
        meta.raw_base_units_to_base_lots_rounded_down(55.0),
        SelfTradeBehavior::Abort,
        None,
        0,
        true,
    );

    let new_order_ix = create_new_order_with_free_funds_instruction(
        market,
        &default_taker.user.pubkey(),
        &sell_params,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_taker.user])
        .await
        .is_err());

    //Trade through the first 10 levels of the book and self trade the last level on each side
    let sell_params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(31.0),
        meta.raw_base_units_to_base_lots_rounded_down(55.0),
        SelfTradeBehavior::Abort,
        None,
        0,
        false,
    );

    let buy_params = OrderPacket::new_ioc_by_lots(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(59.0),
        meta.raw_base_units_to_base_lots_rounded_down(55.0),
        SelfTradeBehavior::Abort,
        None,
        0,
        false,
    );

    let self_trade_bid_params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(30.0),
        meta.raw_base_units_to_base_lots_rounded_down(11.0),
        SelfTradeBehavior::DecrementTake,
        None,
        0,
        false,
    );

    let self_trade_offer_params = OrderPacket::new_ioc_by_lots(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(60.0),
        meta.raw_base_units_to_base_lots_rounded_down(11.0),
        SelfTradeBehavior::DecrementTake,
        None,
        0,
        false,
    );

    let taker_params = vec![sell_params, buy_params];
    let maker_params = vec![self_trade_bid_params, self_trade_offer_params];

    for param in taker_params {
        let new_order_ix = create_new_order_instruction(
            market,
            &default_taker.user.pubkey(),
            base_mint,
            quote_mint,
            &param,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&default_taker.user])
            .await
            .unwrap();
    }

    for param in maker_params {
        let new_order_ix = create_new_order_instruction(
            market,
            &default_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &param,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
            .await
            .unwrap();
    }

    let base_balance_new = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_new = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    println!("Base balance start: {}", base_balance_start);
    println!("Quote balance start: {}", quote_balance_start);
    println!("Base balance new: {}", base_balance_new);
    println!("Quote balance new: {}", quote_balance_new);
    assert_eq!(quote_balance_start - quote_balance_new, 2200000000);
    assert_eq!(base_balance_start - base_balance_new, 66000000000);

    //Attempt to send a LimitOrderWithFreeFunds with the second maker that will fail due to insufficient funds
    sdk.client.payer = clone_keypair(&second_maker.user);
    let new_order_ix = create_new_order_with_free_funds_instruction(
        market,
        &second_maker.user.pubkey(),
        &OrderPacket::new_post_only_default(
            Side::Bid,
            meta.float_price_to_ticks_rounded_down(100.0),
            meta.raw_base_units_to_base_lots_rounded_down(10.0),
        ),
    );
    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
        .await
        .is_err());

    //Add limit orders using the second maker, then use only free lots from the original maker to buy/sell via IOC
    let limit_buy_params = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(30.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
    );

    let limit_sell_params = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
    );

    let ioc_buy_params = OrderPacket::new_ioc_by_lots(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );

    let ioc_sell_params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(30.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );
    let second_maker_params = vec![limit_buy_params, limit_sell_params];
    let maker_ioc_params = vec![ioc_buy_params, ioc_sell_params];
    for param in second_maker_params {
        let new_order_ix = create_new_order_instruction(
            market,
            &second_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &param,
        );

        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .unwrap();
    }
    sdk.set_payer(clone_keypair(&default_maker.user));
    for param in maker_ioc_params {
        let new_order_ix = create_new_order_with_free_funds_instruction(
            market,
            &default_maker.user.pubkey(),
            &param,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
            .await
            .unwrap();
    }

    let base_balance_after_ioc = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_after_ioc = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    // No deposits/withdraws, keep same amount of base lots free, lose 41000000 quote lots free
    assert_eq!(quote_balance_after_ioc - quote_balance_new, 0);
    assert_eq!(base_balance_after_ioc - base_balance_new, 0);

    //Place a new buy and sell order using all remaining free lots + 1 extra unit
    let limit_buy_params = OrderPacket::new_limit_order_default(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(33.69),
        meta.raw_base_units_to_base_lots_rounded_down(101.0),
    );

    let limit_sell_params = OrderPacket::new_limit_order_default(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(50.0),
        meta.raw_base_units_to_base_lots_rounded_down(67.0),
    );

    let maker_params = vec![limit_buy_params, limit_sell_params];

    for param in maker_params {
        let new_order_ix = create_new_order_instruction(
            market,
            &default_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &param,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
            .await
            .unwrap();
    }

    //Check we only used 1 unit worth of new deposits
    let base_balance_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert_eq!(quote_balance_after_ioc - quote_balance_end, 33690000);
    assert_eq!(base_balance_after_ioc - base_balance_end, 1000000000);

    //Attempt to send a SwapWithFreeFunds with the second maker that will fail due to insufficient funds
    sdk.client.payer = clone_keypair(&second_maker.user);
    let second_maker_base_balance_start =
        get_token_balance(&sdk.client, second_maker.base_ata).await;
    let second_maker_quote_balance_start =
        get_token_balance(&sdk.client, second_maker.quote_ata).await;
    let new_order_ix = create_new_order_with_free_funds_instruction(
        market,
        &second_maker.user.pubkey(),
        &OrderPacket::new_ioc_by_lots(
            Side::Bid,
            meta.float_price_to_ticks_rounded_down(250.0),
            meta.raw_base_units_to_base_lots_rounded_down(10.0),
            SelfTradeBehavior::CancelProvide,
            None,
            0,
            true,
        ),
    );
    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
        .await
        .is_err());

    //Add limit orders using the second maker using only free funds
    let limit_buy_params = OrderPacket::new_limit_order(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );

    let limit_sell_params = OrderPacket::new_limit_order(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(35.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );

    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_with_free_funds_instruction(
            market,
            &second_maker.user.pubkey(),
            &params,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .unwrap();
    }

    //Check that the second maker has used only free funds
    let second_maker_base_balance_new = get_token_balance(&sdk.client, second_maker.base_ata).await;
    let second_maker_quote_balance_new =
        get_token_balance(&sdk.client, second_maker.quote_ata).await;
    assert_eq!(
        second_maker_base_balance_new - second_maker_base_balance_start,
        0
    );
    assert_eq!(
        second_maker_quote_balance_new - second_maker_quote_balance_start,
        0
    );

    //Check that internal free funds are now zero, so a new order uses new deposits
    let limit_buy_params = OrderPacket::new_limit_order(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    let limit_sell_params = OrderPacket::new_limit_order(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(35.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_instruction(
            market,
            &second_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &params,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .unwrap();
    }

    let second_maker_base_balance_end = get_token_balance(&sdk.client, second_maker.base_ata).await;
    let second_maker_quote_balance_end =
        get_token_balance(&sdk.client, second_maker.quote_ata).await;
    assert_eq!(
        second_maker_base_balance_new - second_maker_base_balance_end,
        10000000000
    );
    assert_eq!(
        second_maker_quote_balance_new - second_maker_quote_balance_end,
        341000000
    );

    // Cancel all to free up some funds
    let cancel_all_ix =
        create_cancel_all_order_with_free_funds_instruction(market, &second_maker.user.pubkey());

    sdk.client
        .sign_send_instructions(vec![cancel_all_ix], vec![&second_maker.user])
        .await
        .unwrap();

    let limit_buy_params = OrderPacket::new_limit_order(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(5.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );

    let limit_sell_params = OrderPacket::new_limit_order(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(35.0),
        meta.raw_base_units_to_base_lots_rounded_down(5.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        true,
    );

    //Check that sending an orderpacket with free funds set to true fails if we send via the wrong instruction type
    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_instruction(
            market,
            &second_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &params,
        );
        assert!(sdk
            .client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .is_err());
    }

    // Free funds order packet succeeds with correct instruction type
    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_with_free_funds_instruction(
            market,
            &second_maker.user.pubkey(),
            &params,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .unwrap();
    }

    let limit_buy_params = OrderPacket::new_limit_order(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(34.1),
        meta.raw_base_units_to_base_lots_rounded_down(5.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    let limit_sell_params = OrderPacket::new_limit_order(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(35.0),
        meta.raw_base_units_to_base_lots_rounded_down(5.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    // Order packet with free funds set to false fails if we send via the free funds instruction type
    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_with_free_funds_instruction(
            market,
            &second_maker.user.pubkey(),
            &params,
        );
        assert!(sdk
            .client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .is_err());
    }
}

#[tokio::test]
async fn test_phoenix_place_multiple_limit_orders() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let PhoenixTestContext { default_maker, .. } = &phoenix_ctx;

    let second_maker = get_new_maker(&client, &phoenix_ctx, 1_000_000, 1_000_000).await;
    let PhoenixTestClient {
        ctx,
        sdk,
        market,
        meta,
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));

    let base_balance_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    // Place multiple post only orders successfully
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(8.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(9.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(10.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(11.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    {
        let mut adversarial_ix = new_order_ix.clone();
        adversarial_ix.accounts = adversarial_ix.accounts[..5].to_vec();

        assert!(sdk
            .client
            .sign_send_instructions(vec![adversarial_ix], vec![&default_maker.user])
            .await
            .is_err());
    }

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    let base_balance_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert_eq!(base_balance_start - base_balance_end, 20000000000);
    assert_eq!(quote_balance_start - quote_balance_end, 170000000);

    let cancel_order_ix =
        create_cancel_all_order_with_free_funds_instruction(market, &default_maker.user.pubkey());

    sdk.client
        .sign_send_instructions(vec![cancel_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    // Ensure free funds order doesnt place if not enough base lots but enough quote lots
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(8.0),
                meta.raw_base_units_to_base_lots_rounded_down(9.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(11.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(10.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(11.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(12.0),
                meta.raw_base_units_to_base_lots_rounded_down(4.0),
            ),
        ],
    );

    let new_order_ix = create_new_multiple_order_with_free_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    // Ensure free funds order doesnt place if not enough quote lots but enough base lots

    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(8.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(9.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(3.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(1.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(10.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(11.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
    );

    let new_order_ix = create_new_multiple_order_with_free_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    // place multiple post only orders successfully with free funds
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(8.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(9.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
        vec![
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(17.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(17.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(5.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
            CondensedOrder {
                price_in_ticks: meta.float_price_to_ticks_rounded_down(12.0),
                size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(5.0),
                last_valid_slot: None,
                last_valid_unix_timestamp_in_seconds: None,
            },
        ],
    );
    let new_order_ix = create_new_multiple_order_with_free_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        &multiple_order_packet,
    );

    let base_balance_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    // Assert that no new funds were used
    let base_balance_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    assert_eq!(base_balance_start - base_balance_end, 0);
    assert_eq!(quote_balance_start - quote_balance_end, 0);

    // We need to increment the slot because you cannot send duplicated transactions (same blockhash and same instruction)
    ctx.warp_to_slot(2).unwrap();

    // Cancel orders to return the orderbook to empty
    let cancel_order_ix =
        create_cancel_all_order_with_free_funds_instruction(market, &default_maker.user.pubkey());

    sdk.client
        .sign_send_instructions(vec![cancel_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    // Ensure we can't place orders in cross against themselves
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(8.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(9.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(9.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(11.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    // Ensure we can't place orders in cross against themselves, different variation
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(29.0),
                meta.raw_base_units_to_base_lots_rounded_down(1.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(9.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(19.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(30.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(25.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    // Add limit orders to the book from the second maker
    let limit_buy_params = OrderPacket::new_limit_order(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(10.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    let limit_sell_params = OrderPacket::new_limit_order(
        Side::Ask,
        meta.float_price_to_ticks_rounded_down(20.0),
        meta.raw_base_units_to_base_lots_rounded_down(10.0),
        SelfTradeBehavior::CancelProvide,
        None,
        0,
        false,
    );

    for params in [limit_buy_params, limit_sell_params] {
        let new_order_ix = create_new_order_instruction(
            market,
            &second_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &params,
        );
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&second_maker.user])
            .await
            .unwrap();
    }

    // Ensure we can't place orders in cross against the existing book
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(8.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(9.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(10.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(11.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(20.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(9.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .is_err());

    // Check that we use all our free funds first on a normal place multiple
    // Currently have 20 base units and 170 quote units available
    let multiple_order_packet = MultipleOrderPacket::new_default(
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(5.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(4.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(3.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(5.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
            CondensedOrder::new_default(
                //this order is all of the extra quote lots we need to deposit
                meta.float_price_to_ticks_rounded_down(4.0),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            ),
        ],
        vec![
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0),
                meta.raw_base_units_to_base_lots_rounded_down(5.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(105.0),
                meta.raw_base_units_to_base_lots_rounded_down(5.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0),
                meta.raw_base_units_to_base_lots_rounded_down(5.0),
            ),
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(103.0),
                meta.raw_base_units_to_base_lots_rounded_down(5.0),
            ),
            CondensedOrder::new_default(
                //this order is all of the extra base lots we need to deposit
                meta.float_price_to_ticks_rounded_down(102.0),
                meta.raw_base_units_to_base_lots_rounded_down(5.0),
            ),
        ],
    );

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    let base_balance_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    let base_balance_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_balance_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;
    // Check that we only used an extra 40 quote units and 5 base units
    assert_eq!(base_balance_start - base_balance_end, 5000000000);
    assert_eq!(quote_balance_start - quote_balance_end, 40000000);

    ctx.warp_to_slot(3).unwrap();

    // Cancel orders for both makers to return the orderbook to empty
    let cancel_order_ix =
        create_cancel_all_order_with_free_funds_instruction(market, &default_maker.user.pubkey());

    sdk.client
        .sign_send_instructions(vec![cancel_order_ix], vec![&default_maker.user])
        .await
        .unwrap();

    let cancel_order_ix =
        create_cancel_all_order_with_free_funds_instruction(market, &second_maker.user.pubkey());

    sdk.client
        .sign_send_instructions(vec![cancel_order_ix], vec![&second_maker.user])
        .await
        .unwrap();

    // Send 21 orders on each side to verify there is enough compute to do so (this is the upper bound due to the transaction size)
    let bids = (1..22)
        .map(|i| {
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0 - (i as f64 * 0.1)),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            )
        })
        .collect::<Vec<_>>();
    let asks = (1..22)
        .map(|i| {
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0 + (i as f64 * 0.1)),
                meta.raw_base_units_to_base_lots_rounded_down(10.0),
            )
        })
        .collect::<Vec<_>>();

    let multiple_order_packet = MultipleOrderPacket::new_default(bids, asks);

    let byte_len = multiple_order_packet.try_to_vec().unwrap().len();
    assert_eq!(byte_len, 766);

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
        &multiple_order_packet,
    );

    sdk.client
        .sign_send_instructions(
            vec![
                ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
                new_order_ix,
            ],
            vec![&default_maker.user],
        )
        .await
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn layer_orders(
    meta: &MarketMetadata,
    market: &Pubkey,
    sdk: &SDKClient,
    start_price: u64,
    end_price: u64,
    price_step: u64,
    start_size: u64,
    size_step: u64,
    side: Side,
) {
    assert!(price_step > 0);
    let mut prices = vec![];
    let mut sizes = vec![];
    match side {
        Side::Bid => {
            assert!(start_price >= end_price);
            let mut price = start_price;
            let mut size = start_size;
            while price >= end_price && price > 0 {
                prices.push(price);
                sizes.push(size);
                price -= price_step;
                size += size_step;
            }
        }
        Side::Ask => {
            assert!(start_price <= end_price);
            let mut price = start_price;
            let mut size = start_size;
            while price <= end_price {
                prices.push(price);
                sizes.push(size);
                price += price_step;
                size += size_step;
            }
        }
    }
    let mut ixs = vec![];
    for (p, s) in prices.iter().zip(sizes.iter()) {
        let params = OrderPacket::new_limit_order_default(side, *p, *s);
        let new_order_ix = create_new_order_instruction(
            market,
            &sdk.get_trader(),
            &meta.base_mint,
            &meta.quote_mint,
            &params,
        );
        ixs.push(new_order_ix);
    }

    let chunk_size = 12;
    for chunk in ixs.chunks(chunk_size) {
        sdk.client
            .sign_send_instructions(chunk.to_vec(), vec![])
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn test_phoenix_log_authorization() {
    let context = phoenix_test().start_with_context().await;
    let ellipsis_client = EllipsisClient::from_banks(&context.banks_client, &context.payer)
        .await
        .unwrap();
    let log_instruction = Instruction {
        program_id: phoenix::id(),
        accounts: vec![AccountMeta::new_readonly(
            ellipsis_client.payer.pubkey(),
            true,
        )],
        data: PhoenixInstruction::Log.to_vec(),
    };
    assert!(
        ellipsis_client
            .sign_send_instructions(vec![log_instruction], vec![])
            .await
            .is_err(),
        "Arbitrary signer should not be able to log"
    );
    let log_instruction = Instruction {
        program_id: phoenix::id(),
        accounts: vec![AccountMeta::new_readonly(
            phoenix_log_authority::id(),
            false,
        )],
        data: PhoenixInstruction::Log.to_vec(),
    };
    assert!(
        ellipsis_client
            .sign_send_instructions(vec![log_instruction], vec![])
            .await
            .is_err(),
        "Account is not signer"
    );
    let log_instruction = Instruction {
        program_id: phoenix::id(),
        accounts: vec![AccountMeta::new_readonly(phoenix_log_authority::id(), true)],
        data: PhoenixInstruction::Log.to_vec(),
    };
    assert!(
        ellipsis_client
            .sign_send_instructions(vec![log_instruction], vec![])
            .await
            .is_err(),
        "PDA cannot sign outside of the program"
    );
}

#[tokio::test]
async fn test_phoenix_cancel_all_memory_management() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;
    let PhoenixTestContext { default_maker, .. } = &phoenix_ctx;

    sdk.set_payer(clone_keypair(&default_maker.user));
    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(40.0),
        meta.float_price_to_ticks_rounded_down(38.0),
        meta.float_price_to_ticks_rounded_down(0.01),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(0.0),
        Side::Bid,
    )
    .await;

    layer_orders(
        meta,
        market,
        &sdk,
        meta.float_price_to_ticks_rounded_down(40.01),
        meta.float_price_to_ticks_rounded_down(42.0),
        meta.float_price_to_ticks_rounded_down(0.01),
        meta.raw_base_units_to_base_lots_rounded_down(1.0),
        meta.raw_base_units_to_base_lots_rounded_down(0.0),
        Side::Ask,
    )
    .await;

    let ix = sdk.get_cancel_all_ix(market).unwrap();
    sdk.client
        .sign_send_instructions(
            vec![
                ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
                ix,
            ],
            vec![&default_maker.user],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_phoenix_place_multiple_memory_management() {
    let (client, phoenix_ctx) = bootstrap_default(0).await;

    let PhoenixTestContext {
        default_maker,
        default_taker,
        ..
    } = &phoenix_ctx;

    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &client;

    // Send 21 orders on each side to verify there is enough compute to do so (this is the upper bound due to the transaction size)
    let bids = (1..22)
        .map(|i| {
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0 - (i as f64 * 0.1)),
                meta.raw_base_units_to_base_lots_rounded_down(1.0),
            )
        })
        .collect::<Vec<_>>();
    let asks = (1..22)
        .map(|i| {
            CondensedOrder::new_default(
                meta.float_price_to_ticks_rounded_down(100.0 + (i as f64 * 0.1)),
                meta.raw_base_units_to_base_lots_rounded_down(1.0),
            )
        })
        .collect::<Vec<_>>();

    let multiple_order_packet = MultipleOrderPacket::new_default(bids, asks);

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &default_maker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &multiple_order_packet,
    );

    sdk.client
        .sign_send_instructions(
            vec![
                ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
                new_order_ix.clone(),
            ],
            vec![&default_maker.user],
        )
        .await
        .unwrap();

    sdk.client
        .sign_send_instructions(
            vec![
                ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
                create_new_order_instruction(
                    market,
                    &default_taker.user.pubkey(),
                    &meta.base_mint,
                    &meta.quote_mint,
                    &OrderPacket::new_ioc_by_lots(
                        Side::Ask,
                        0,
                        u64::MAX,
                        SelfTradeBehavior::DecrementTake,
                        None,
                        0,
                        false,
                    ),
                ),
            ],
            vec![&default_taker.user],
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_phoenix_place_multiple_limit_orders_adversarial() {
    let (mut phoenix_test_client, phoenix_ctx) = bootstrap_default(0).await;

    let PhoenixTestContext {
        default_maker,
        default_taker,
        ..
    } = &phoenix_ctx;

    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut phoenix_test_client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));

    let mut start = 0;
    let mut size = 0;
    // Stuff the book with 1 lots
    loop {
        let bids = (start..start + 30)
            .map(|_| CondensedOrder::new_default(meta.float_price_to_ticks_rounded_down(99.0), 1))
            .collect::<Vec<_>>();
        let asks = (start..start + 30)
            .map(|_| CondensedOrder::new_default(meta.float_price_to_ticks_rounded_down(100.0), 1))
            .collect::<Vec<_>>();

        let multiple_order_packet = MultipleOrderPacket::new_default(bids, asks);

        let new_order_ix = create_new_multiple_order_instruction(
            market,
            &default_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &multiple_order_packet,
        );
        // Add noise for blockhash
        let self_transfer = transfer(
            &default_maker.user.pubkey(),
            &default_maker.user.pubkey(),
            start,
        );
        start += 1;
        size += 30;
        if size > BOOK_SIZE {
            break;
        }
        sdk.client
            .sign_send_instructions(vec![new_order_ix, self_transfer], vec![&default_maker.user])
            .await
            .unwrap();
    }

    // Normally this would crash due to compute usage, but we now coalesce the orders
    // at the same price in place multiple orders
    sdk.set_payer(clone_keypair(&default_taker.user));
    let order_packet = OrderPacket::new_ioc_by_lots(
        Side::Bid,
        meta.float_price_to_ticks_rounded_down(101.0),
        700,
        SelfTradeBehavior::Abort,
        None,
        0,
        false,
    );
    let ix = create_new_order_instruction(
        market,
        &default_taker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &order_packet,
    );

    let request_compute_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);
    sdk.client
        .sign_send_instructions(vec![request_compute_ix, ix], vec![])
        .await
        .unwrap();
}

#[tokio::test]
async fn test_phoenix_basic_with_raw_base_unit_adjustment() {
    // For tokens whose raw base unit is worth less than one USDC atom, we need to adjust the Phoenix BaseUnit by a multiplicative factor
    // such that the BaseUnit can be represented by a positive integer of USDC atoms.
    let raw_base_units_per_base_unit: u64 = 1_000;
    let tick_size_in_quote_lots_per_base_unit = 10; // base_unit is BaseUnit (adjusted)
    let base_lot_per_base_unit = 10; // base_unit is BaseUnit (adjusted)

    let (mut client, ctx) = bootstrap_with_parameters(
        1_000_000,
        base_lot_per_base_unit,
        tick_size_in_quote_lots_per_base_unit,
        5,
        6,
        0,
        Some(raw_base_units_per_base_unit as u32),
    )
    .await;
    let PhoenixTestContext {
        default_maker,
        default_taker,
        ..
    } = &ctx;

    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    mint_tokens(
        &sdk.client,
        &ctx.mint_authority,
        &meta.base_mint,
        &default_maker.base_ata,
        1_000_000 * 1e12 as u64,
        None,
    )
    .await
    .unwrap();

    mint_tokens(
        &sdk.client,
        &ctx.mint_authority,
        &meta.quote_mint,
        &default_maker.quote_ata,
        1_000_000 * 1e9 as u64,
        None,
    )
    .await
    .unwrap();

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&default_maker.user));

    // Add two layers of bids and two layers of asks
    let mut layer_ixs: Vec<Instruction> = vec![];
    let bid_price_range: Vec<f64> = vec![0.000001358, 0.000001359];
    let ask_price_range: Vec<f64> = vec![0.000001361, 0.000001362];

    for (bid_price, ask_price) in bid_price_range.iter().zip(ask_price_range.iter()) {
        let bid_params = OrderPacket::new_limit_order(
            Side::Bid,
            meta.float_price_to_ticks_rounded_down(*bid_price),
            meta.raw_base_units_to_base_lots_rounded_down(1000_f64), // 1_000 tokens, or 1_000 raw_base_units
            SelfTradeBehavior::Abort,
            None,
            0,
            false,
        );

        let ask_params = OrderPacket::new_limit_order(
            Side::Ask,
            meta.float_price_to_ticks_rounded_down(*ask_price),
            meta.raw_base_units_to_base_lots_rounded_down(1000_f64), // 1_000 tokens, or 1_000 raw_base_units
            SelfTradeBehavior::Abort,
            None,
            0,
            false,
        );

        let bid_ix = create_new_order_instruction(
            market,
            &default_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &bid_params,
        );

        let ask_ix = create_new_order_instruction(
            market,
            &default_maker.user.pubkey(),
            base_mint,
            quote_mint,
            &ask_params,
        );

        layer_ixs.push(bid_ix);
        layer_ixs.push(ask_ix);
    }

    sdk.client
        .sign_send_instructions(layer_ixs, vec![])
        .await
        .unwrap();

    let first_cross_price =
        meta.float_price_to_ticks_rounded_down(*bid_price_range.last().unwrap());
    let first_cross_size = meta.raw_base_units_to_base_lots_rounded_down(1000_f64);
    let second_cross_price =
        meta.float_price_to_ticks_rounded_down(*bid_price_range.first().unwrap()); // Takes the last price in the bid price_range (40.0)
    let second_cross_size = meta.raw_base_units_to_base_lots_rounded_down(1000_f64);

    let params = OrderPacket::new_ioc_by_lots(
        Side::Ask,
        second_cross_price,
        first_cross_size + second_cross_size,
        SelfTradeBehavior::Abort,
        None,
        19082332,
        false,
    );

    sdk.set_payer(clone_keypair(&default_taker.user));
    let base_start = get_token_balance(&sdk.client, default_taker.base_ata).await;
    let quote_start = get_token_balance(&sdk.client, default_taker.quote_ata).await;
    let base_lot_size = &meta.base_atoms_per_base_lot;
    println!("base_lot_size: {}", base_lot_size);
    let quote_lot_size = &meta.quote_atoms_per_quote_lot;
    println!("quote_lot_size: {}", quote_lot_size);
    println!(
        "base_lots per base_unit: {}",
        &meta.num_base_lots_per_base_unit
    );

    let new_order_ix = create_new_order_instruction(
        market,
        &default_taker.user.pubkey(),
        base_mint,
        quote_mint,
        &params,
    );
    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![])
        .await
        .unwrap();
    let base_end = get_token_balance(&sdk.client, default_taker.base_ata).await;
    let quote_end = get_token_balance(&sdk.client, default_taker.quote_ata).await;
    println!("Base start: {}", base_start);
    println!("Quote start: {}", quote_start);
    println!("Base end: {}", base_end);
    println!("Quote end: {}", quote_end);
    assert_eq!(
        quote_end - quote_start,
        first_cross_price * first_cross_size * quote_lot_size
            + second_cross_price * second_cross_size * quote_lot_size
    );
    assert_eq!(
        base_start - base_end,
        first_cross_size * base_lot_size + second_cross_size * base_lot_size
    );

    let base_start = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_start = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    let withdraw_ix = create_withdraw_funds_instruction(
        market,
        &default_maker.user.pubkey(),
        base_mint,
        quote_mint,
    );
    sdk.client
        .sign_send_instructions(vec![withdraw_ix], vec![])
        .await
        .unwrap();
    let base_end = get_token_balance(&sdk.client, default_maker.base_ata).await;
    let quote_end = get_token_balance(&sdk.client, default_maker.quote_ata).await;

    assert_eq!(quote_end - quote_start, 0);
    assert_eq!(
        base_end - base_start,
        first_cross_size * base_lot_size + second_cross_size * base_lot_size
    );
}

#[tokio::test]
async fn test_phoenix_place_order_quiet_failure() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    // 100 SOL, 1_000 USDC
    let maker = get_new_maker(&client, &phoenix_ctx, 100, 1_000).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    let base_balance_start = get_token_balance(&sdk.client, maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, maker.quote_ata).await;
    println!("Base balance start: {}", base_balance_start);
    println!("Quote balance start: {}", quote_balance_start);

    println!("Depositing 3 SOL and 3 USDC");
    let deposit_ix = create_deposit_funds_instruction(
        market,
        &maker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: meta.quote_units_to_quote_lots(3.0),
            base_lots_to_deposit: meta.raw_base_units_to_base_lots_rounded_down(3.0),
        },
    );
    sdk.client
        .sign_send_instructions(vec![deposit_ix], vec![&maker.user])
        .await
        .unwrap();

    println!("Placing ask order for 97 SOL (deposited funds + tokens)");
    let params = OrderPacket::Limit {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(97_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };

    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    let base_balance = get_token_balance(&sdk.client, maker.base_ata).await;
    assert_eq!(base_balance, 3e9 as u64, "Order failed to deposit 97 SOL");

    println!("Placing ask order for 1 SOL");
    let params = OrderPacket::Limit {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };
    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    println!("Placing ask order (using only deposited funds) for 1 SOL");

    let deposit_ix = create_deposit_funds_instruction(
        market,
        &maker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: 0,
            base_lots_to_deposit: meta.raw_base_units_to_base_lots_rounded_down(1.0),
        },
    );

    let params = OrderPacket::Limit {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: true,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };
    let new_order_ix =
        create_new_order_with_free_funds_instruction(market, &maker.user.pubkey(), &params);

    sdk.client
        .sign_send_instructions(vec![deposit_ix, new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    let market_start = sdk.get_market_orderbook(market).await.unwrap();
    assert_eq!(market_start.asks.len(), 3);

    // This order should fail silently
    let params = OrderPacket::Limit {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(2_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };

    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    let market_end = sdk.get_market_orderbook(market).await.unwrap();
    assert_eq!(
        market_start.asks.len(),
        market_end.asks.len(),
        "Order should have failed silently"
    );

    // This order should fail
    let params = OrderPacket::Limit {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(2_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    assert!(
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
            .await
            .is_err(),
        "Order should have failed"
    );

    let market_end = sdk.get_market_orderbook(market).await.unwrap();
    assert_eq!(
        market_start.asks.len(),
        market_end.asks.len(),
        "Order count should be the same"
    );

    println!("Cancelling all orders");
    sdk.client
        .sign_send_instructions(
            vec![sdk.get_cancel_all_ix(market).unwrap()],
            vec![&maker.user],
        )
        .await
        .unwrap();

    let base_balance_end = get_token_balance(&sdk.client, maker.base_ata).await;
    assert_eq!(
        base_balance_start, base_balance_end as u64,
        "Balances should not change"
    );

    println!("Placing bid order for 997 USDC (deposited funds + tokens)");

    let params = OrderPacket::Limit {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(99.7_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };

    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    let quote_balance = get_token_balance(&sdk.client, maker.quote_ata).await;
    assert_eq!(
        quote_balance, 3e6 as u64,
        "Order failed to deposit 997 USDC"
    );

    println!("Placing bid order for 1 USDC");

    let params = OrderPacket::Limit {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(1.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };

    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    println!("Placing bid order (using only deposited funds) for 1 USDC");
    let deposit_ix = create_deposit_funds_instruction(
        market,
        &maker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: meta.quote_units_to_quote_lots(1.0),
            base_lots_to_deposit: 0,
        },
    );

    let params = OrderPacket::Limit {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(1.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: true,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };
    let new_order_ix =
        create_new_order_with_free_funds_instruction(market, &maker.user.pubkey(), &params);

    sdk.client
        .sign_send_instructions(vec![deposit_ix, new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    let market_start = sdk.get_market_orderbook(market).await.unwrap();
    assert_eq!(market_start.bids.len(), 3);

    // This order should fail silently
    let params = OrderPacket::Limit {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(2_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: true,
    };

    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    sdk.client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .unwrap();

    // This order should fail
    let params = OrderPacket::Limit {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.0)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(2_f64)),
        self_trade_behavior: SelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let new_order_ix =
        create_new_order_instruction(market, &maker.user.pubkey(), base_mint, quote_mint, &params);

    assert!(
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
            .await
            .is_err(),
        "Order should have failed"
    );

    let market_end = sdk.get_market_orderbook(market).await.unwrap();

    assert_eq!(
        market_start.bids.len(),
        market_end.bids.len(),
        "Order should have failed silently"
    );

    println!("Cancelling all orders");
    sdk.client
        .sign_send_instructions(
            vec![sdk.get_cancel_all_ix(market).unwrap()],
            vec![&maker.user],
        )
        .await
        .unwrap();

    let quote_balance_end = get_token_balance(&sdk.client, maker.quote_ata).await;
    assert_eq!(
        quote_balance_start, quote_balance_end,
        "Balances should not change"
    );
}

/// This tests that a user can place multiple orders that fail silently even if the user
/// is out of funds.
#[tokio::test]
async fn test_phoenix_multiple_orders_fail_silently_basic() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let maker = get_new_maker(&client, &phoenix_ctx, 99, 901).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    let deposit_ix = create_deposit_funds_instruction(
        market,
        &maker.user.pubkey(),
        &meta.base_mint,
        &meta.quote_mint,
        &DepositParams {
            quote_lots_to_deposit: meta.quote_units_to_quote_lots(312.0),
            base_lots_to_deposit: meta.raw_base_units_to_base_lots_rounded_down(39.0),
        },
    );
    sdk.client
        .sign_send_instructions(vec![deposit_ix], vec![&maker.user])
        .await
        .unwrap();

    let mut bids = vec![];
    for i in 0..10 {
        bids.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_down(10.0 - 0.01 * i as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let mut asks = vec![];

    for i in 0..10 {
        asks.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_down(10.0 + 0.01 * (i + 1) as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_ok());

    let market = sdk.get_market_orderbook(market).await.unwrap();

    // Unflip the bits of the bid order_sequence_numbers to get the true order of placement
    let bid_sequence_numbers = market
        .bids
        .iter()
        .sorted_by(|a, b| a.0.price_in_ticks.cmp(&b.0.price_in_ticks))
        .map(|order| !order.0.order_sequence_number)
        .collect::<Vec<u64>>();

    assert!(
        bid_sequence_numbers
            .iter()
            .zip(bid_sequence_numbers.iter().skip(1))
            .all(|(a, b)| a > b),
        "Bids with higher prices should have lower sequence numbers"
    );

    let ask_sequence_numbers = market
        .asks
        .iter()
        .sorted_by(|a, b| a.0.price_in_ticks.cmp(&b.0.price_in_ticks))
        .map(|order| order.0.order_sequence_number)
        .collect::<Vec<u64>>();

    assert!(
        ask_sequence_numbers
            .iter()
            .zip(ask_sequence_numbers.iter().skip(1))
            .all(|(a, b)| a < b),
        "Asks with lower prices should have lower sequence numbers"
    );
    assert_eq!(market.bids.len(), 9);
    assert_eq!(market.asks.len(), 9);
}

/// This tests that placing multiple orders will fail if the input orders cross
#[tokio::test]
async fn test_phoenix_multiple_orders_crossing_order_input() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    // 100 SOL, 1_000 USDC
    let maker = get_new_maker(&client, &phoenix_ctx, 10, 100).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    let base_balance_start = get_token_balance(&sdk.client, maker.base_ata).await;
    let quote_balance_start = get_token_balance(&sdk.client, maker.quote_ata).await;
    println!("Base balance start: {}", base_balance_start);
    println!("Quote balance start: {}", quote_balance_start);

    let bids = vec![CondensedOrder {
        price_in_ticks: meta.float_price_to_ticks_rounded_down(10.0),
        size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
    }];

    let asks = vec![CondensedOrder {
        price_in_ticks: meta.float_price_to_ticks_rounded_down(9.99),
        size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
    }];

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());
}

/// This tests that placing multiple orders will still succeed if one of the orders crosses the bid-ask spread
#[tokio::test]
async fn test_phoenix_multiple_orders_crossing_existing_book_ignore_crossing_bid() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let maker = get_new_maker(&client, &phoenix_ctx, 101, 1010).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    // Create limit orders at 9.96 and 10.01
    let bid_order_packet = OrderPacket::PostOnly {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(9.96)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let bid_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &bid_order_packet,
    );

    let ask_order_packet = OrderPacket::PostOnly {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.01)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let ask_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &ask_order_packet,
    );

    sdk.client
        .sign_send_instructions(vec![bid_ix, ask_ix], vec![&maker.user])
        .await
        .unwrap();

    let mut bids = vec![];

    for i in 0..10 {
        bids.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_down(10.01 - 0.01 * i as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let mut asks = vec![];

    for i in 0..10 {
        asks.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_up(10.02 + 0.01 * i as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
            .await
            .is_err(),
        "Order should fail on cross"
    );
}

/// This tests that placing multiple orders will still succeed if one of the orders crosses the bid-ask spread
#[tokio::test]
async fn test_phoenix_multiple_orders_crossing_existing_book_amend_bid() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let maker = get_new_maker(&client, &phoenix_ctx, 101, 1010).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    // Create limit orders at 9.96 and 10.01
    let bid_order_packet = OrderPacket::PostOnly {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(9.96)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let bid_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &bid_order_packet,
    );

    let ask_order_packet = OrderPacket::PostOnly {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.01)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let ask_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &ask_order_packet,
    );

    sdk.client
        .sign_send_instructions(vec![bid_ix, ask_ix], vec![&maker.user])
        .await
        .unwrap();

    let mut bids = vec![];

    for i in 0..10 {
        bids.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_down(10.01 - 0.01 * i as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let mut asks = vec![];

    for i in 0..10 {
        asks.push(CondensedOrder {
            price_in_ticks: meta.float_price_to_ticks_rounded_up(10.02 + 0.01 * i as f64),
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndAmendOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_ok());

    let market = sdk.get_market_orderbook(market).await.unwrap();
    let market_bids = market
        .bids
        .iter()
        .map(|(o, _)| o.price_in_ticks.into())
        .collect::<Vec<_>>();
    for bid in bids {
        if bid.price_in_ticks >= 1001 {
            assert!(!market_bids.contains(&bid.price_in_ticks));
        } else {
            assert!(market_bids.contains(&bid.price_in_ticks));
        }
    }

    assert_eq!(market_bids.len(), 11);
    assert!(market_bids.iter().filter(|&x| *x == 1000).count() == 2);

    let market_asks = market
        .asks
        .iter()
        .map(|(o, _)| o.price_in_ticks.into())
        .collect::<Vec<_>>();

    for ask in asks {
        println!("{:?}", ask);
        assert!(market_asks.contains(&ask.price_in_ticks));
    }
    assert_eq!(market_asks.len(), 11);

    assert_eq!(market_asks[0], 1001);
    assert_eq!(market_bids[0], 1000);
}

/// This tests that placing multiple orders will still succeed if one of the orders crosses the bid-ask spread
#[tokio::test]
async fn test_phoenix_multiple_orders_crossing_existing_book_ignore_crossing_ask() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let maker = get_new_maker(&client, &phoenix_ctx, 101, 1010).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    // Create limit orders at 9.96 and 10.01
    let bid_order_packet = OrderPacket::PostOnly {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(9.96)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let bid_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &bid_order_packet,
    );

    let ask_order_packet = OrderPacket::PostOnly {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.01)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let ask_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &ask_order_packet,
    );

    sdk.client
        .sign_send_instructions(vec![bid_ix, ask_ix], vec![&maker.user])
        .await
        .unwrap();

    let mut bids = vec![];

    for i in 0..10 {
        bids.push(CondensedOrder {
            price_in_ticks: 994 - i,
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let mut asks = vec![];

    for i in 0..10 {
        asks.push(CondensedOrder {
            price_in_ticks: 995 + i,
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(
        sdk.client
            .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
            .await
            .is_err(),
        "Order should fail on cross"
    );
}

/// This tests that placing multiple orders will still succeed if one of the orders crosses the bid-ask spread
#[tokio::test]
async fn test_phoenix_multiple_orders_crossing_existing_book_amend_ask() {
    let (mut client, phoenix_ctx) = bootstrap_default(0).await;

    let maker = get_new_maker(&client, &phoenix_ctx, 101, 1010).await;
    let PhoenixTestClient {
        sdk, market, meta, ..
    } = &mut client;

    let quote_mint = &meta.quote_mint;
    let base_mint = &meta.base_mint;

    sdk.set_payer(clone_keypair(&maker.user));

    // Create limit orders at 9.96 and 10.01
    let bid_order_packet = OrderPacket::PostOnly {
        side: Side::Bid,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(9.96)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let bid_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &bid_order_packet,
    );

    let ask_order_packet = OrderPacket::PostOnly {
        side: Side::Ask,
        price_in_ticks: Ticks::new(meta.float_price_to_ticks_rounded_down(10.01)),
        num_base_lots: BaseLots::new(meta.raw_base_units_to_base_lots_rounded_down(1_f64)),
        client_order_id: 0,
        reject_post_only: true,
        use_only_deposited_funds: false,
        last_valid_slot: None,
        last_valid_unix_timestamp_in_seconds: None,
        fail_silently_on_insufficient_funds: false,
    };
    let ask_ix = create_new_order_instruction(
        market,
        &maker.user.pubkey(),
        &base_mint,
        &quote_mint,
        &ask_order_packet,
    );

    sdk.client
        .sign_send_instructions(vec![bid_ix, ask_ix], vec![&maker.user])
        .await
        .unwrap();

    let mut bids = vec![];

    for i in 0..10 {
        bids.push(CondensedOrder {
            price_in_ticks: 994 - i,
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    println!("bids: {:?}", bids);

    let mut asks = vec![];

    for i in 0..10 {
        asks.push(CondensedOrder {
            price_in_ticks: 995 + i,
            size_in_base_lots: meta.raw_base_units_to_base_lots_rounded_down(10_f64),
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        });
    }

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_err());

    let order_packet = MultipleOrderPacket {
        asks: asks.clone(),
        bids: bids.clone(),
        client_order_id: None,
        failed_multiple_limit_order_behavior:
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndAmendOnCross,
    };

    let new_order_ix = create_new_multiple_order_instruction(
        market,
        &maker.user.pubkey(),
        base_mint,
        quote_mint,
        &order_packet,
    );

    assert!(sdk
        .client
        .sign_send_instructions(vec![new_order_ix], vec![&maker.user])
        .await
        .is_ok());

    let market = sdk.get_market_orderbook(market).await.unwrap();
    let market_bids = market
        .bids
        .iter()
        .map(|(o, _)| o.price_in_ticks.into())
        .collect::<Vec<_>>();
    for bid in bids {
        assert!(market_bids.contains(&bid.price_in_ticks));
    }

    assert_eq!(market_bids.len(), 11);

    let market_asks = market
        .asks
        .iter()
        .map(|(o, _)| o.price_in_ticks.into())
        .collect::<Vec<_>>();

    for ask in asks {
        if ask.price_in_ticks > 996 {
            assert!(market_asks.contains(&ask.price_in_ticks));
        }
    }

    assert!(market_asks.iter().filter(|&x| *x == 997).count() == 3);
    assert_eq!(market_asks.len(), 11);

    assert_eq!(market_asks[0], 997);
    assert_eq!(market_bids[0], 996);
}
