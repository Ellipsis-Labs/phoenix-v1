use crate::{
    program::{
        dispatch_market::load_with_dispatch_mut,
        error::{assert_with_msg, PhoenixError},
        loaders::NewOrderContext,
        status::MarketStatus,
        token_utils::{maybe_invoke_deposit, maybe_invoke_withdraw},
        MarketHeader, PhoenixMarketContext, PhoenixVaultContext,
    },
    quantities::{
        BaseAtoms, BaseAtomsPerBaseLot, BaseLots, QuoteAtoms, QuoteAtomsPerQuoteLot, QuoteLots,
        Ticks, WrapperU64,
    },
    state::{
        decode_order_packet,
        markets::{FIFOOrderId, FIFORestingOrder, MarketEvent, MarketWrapperMut},
        OrderPacket, OrderPacketMetadata, Side,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use itertools::Itertools;
use solana_program::{
    account_info::AccountInfo, clock::Clock, entrypoint::ProgramResult, log::sol_log_compute_units,
    program_error::ProgramError, pubkey::Pubkey, sysvar::Sysvar,
};
use std::mem::size_of;

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub enum FailedMultipleLimitOrderBehavior {
    /// Orders will never cross the spread. Instead they will be amended to the closest non-crossing price.
    /// The entire transaction will fail if matching engine returns None for any order, which indicates an error.
    ///
    /// If an order has insufficient funds, the entire transaction will fail.
    FailOnInsufficientFundsAndAmendOnCross,

    /// If any order crosses the spread or has insufficient funds, the entire transaction will fail.
    FailOnInsufficientFundsAndFailOnCross,

    /// Orders will be skipped if the user has insufficient funds.
    /// Crossing orders will be amended to the closest non-crossing price.
    SkipOnInsufficientFundsAndAmendOnCross,

    /// Orders will be skipped if the user has insufficient funds.
    /// If any order crosses the spread, the entire transaction will fail.
    SkipOnInsufficientFundsAndFailOnCross,
}

impl FailedMultipleLimitOrderBehavior {
    pub fn should_fail_on_cross(&self) -> bool {
        matches!(
            self,
            FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross
                | FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndFailOnCross
        )
    }

    pub fn should_skip_orders_with_insufficient_funds(&self) -> bool {
        matches!(
            self,
            FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndAmendOnCross
                | FailedMultipleLimitOrderBehavior::SkipOnInsufficientFundsAndFailOnCross
        )
    }
}

/// Struct to send a vector of bids and asks as PostOnly orders in a single packet.
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct MultipleOrderPacket {
    /// Bids and asks are in the format (price in ticks, size in base lots)
    pub bids: Vec<CondensedOrder>,
    pub asks: Vec<CondensedOrder>,
    pub client_order_id: Option<u128>,
    pub failed_multiple_limit_order_behavior: FailedMultipleLimitOrderBehavior,
}

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct CondensedOrder {
    pub price_in_ticks: u64,
    pub size_in_base_lots: u64,
    pub last_valid_slot: Option<u64>,
    pub last_valid_unix_timestamp_in_seconds: Option<u64>,
}

impl CondensedOrder {
    pub fn new_default(price_in_ticks: u64, size_in_base_lots: u64) -> Self {
        CondensedOrder {
            price_in_ticks,
            size_in_base_lots,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }
}

impl MultipleOrderPacket {
    pub fn new(
        bids: Vec<CondensedOrder>,
        asks: Vec<CondensedOrder>,
        client_order_id: Option<u128>,
        reject_post_only: bool,
    ) -> Self {
        MultipleOrderPacket {
            bids,
            asks,
            client_order_id,
            failed_multiple_limit_order_behavior: if reject_post_only {
                FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross
            } else {
                FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndAmendOnCross
            },
        }
    }

    pub fn new_default(bids: Vec<CondensedOrder>, asks: Vec<CondensedOrder>) -> Self {
        MultipleOrderPacket {
            bids,
            asks,
            client_order_id: None,
            failed_multiple_limit_order_behavior:
                FailedMultipleLimitOrderBehavior::FailOnInsufficientFundsAndFailOnCross,
        }
    }

    pub fn new_with_failure_behavior(
        bids: Vec<CondensedOrder>,
        asks: Vec<CondensedOrder>,
        client_order_id: Option<u128>,
        failed_multiple_limit_order_behavior: FailedMultipleLimitOrderBehavior,
    ) -> Self {
        MultipleOrderPacket {
            bids,
            asks,
            client_order_id,
            failed_multiple_limit_order_behavior,
        }
    }
}

/// This function performs an IOC or FOK order against the specified market.
pub(crate) fn process_swap<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    sol_log_compute_units();
    let new_order_context = NewOrderContext::load_cross_only(market_context, accounts, false)?;
    let mut order_packet = decode_order_packet(data).ok_or_else(|| {
        phoenix_log!("Failed to decode order packet");
        ProgramError::InvalidInstructionData
    })?;
    assert_with_msg(
        new_order_context.seat_option.is_none(),
        ProgramError::InvalidInstructionData,
        "Too many accounts",
    )?;
    assert_with_msg(
        order_packet.is_take_only(),
        ProgramError::InvalidInstructionData,
        "Order type must be IOC or FOK",
    )?;
    assert_with_msg(
        !order_packet.no_deposit_or_withdrawal(),
        ProgramError::InvalidInstructionData,
        "Instruction does not allow using deposited funds",
    )?;
    let mut order_ids = vec![];
    process_new_order(
        new_order_context,
        market_context,
        &mut order_packet,
        record_event_fn,
        &mut order_ids,
    )
}

/// This function performs an IOC or FOK order against the specified market
/// using only the funds already available to the trader.
/// Only users with sufficient funds and a "seat" on the market are authorized
/// to perform this action.
pub(crate) fn process_swap_with_free_funds<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let new_order_context = NewOrderContext::load_cross_only(market_context, accounts, true)?;
    let mut order_packet = decode_order_packet(data).ok_or_else(|| {
        phoenix_log!("Failed to decode order packet");
        ProgramError::InvalidInstructionData
    })?;
    assert_with_msg(
        new_order_context.seat_option.is_some(),
        ProgramError::InvalidInstructionData,
        "Missing seat for market maker",
    )?;
    assert_with_msg(
        order_packet.is_take_only(),
        ProgramError::InvalidInstructionData,
        "Order type must be IOC or FOK",
    )?;
    assert_with_msg(
        order_packet.no_deposit_or_withdrawal(),
        ProgramError::InvalidInstructionData,
        "Order must be set to use only deposited funds",
    )?;
    let mut order_ids = vec![];
    process_new_order(
        new_order_context,
        market_context,
        &mut order_packet,
        record_event_fn,
        &mut order_ids,
    )
}

/// This function performs a Post-Only or Limit order against the specified market.
/// Only users with a "seat" on the market are authorized to perform this action.
pub(crate) fn process_place_limit_order<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
) -> ProgramResult {
    let new_order_context = NewOrderContext::load_post_allowed(market_context, accounts, false)?;
    let mut order_packet = decode_order_packet(data).ok_or_else(|| {
        phoenix_log!("Failed to decode order packet");
        ProgramError::InvalidInstructionData
    })?;
    assert_with_msg(
        new_order_context.seat_option.is_some(),
        ProgramError::InvalidInstructionData,
        "Missing seat for market maker",
    )?;
    assert_with_msg(
        !order_packet.is_take_only(),
        ProgramError::InvalidInstructionData,
        "Order type must be Limit or PostOnly",
    )?;
    assert_with_msg(
        !order_packet.no_deposit_or_withdrawal(),
        ProgramError::InvalidInstructionData,
        "Instruction does not allow using deposited funds",
    )?;
    process_new_order(
        new_order_context,
        market_context,
        &mut order_packet,
        record_event_fn,
        order_ids,
    )
}

/// This function performs a Post-Only or Limit order against the specified market
/// using only the funds already available to the trader.
/// Only users with sufficient funds and a "seat" on the market are authorized
/// to perform this action.
pub(crate) fn process_place_limit_order_with_free_funds<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
) -> ProgramResult {
    let new_order_context = NewOrderContext::load_post_allowed(market_context, accounts, true)?;
    let mut order_packet = decode_order_packet(data).ok_or_else(|| {
        phoenix_log!("Failed to decode order packet");
        ProgramError::InvalidInstructionData
    })?;
    assert_with_msg(
        new_order_context.seat_option.is_some(),
        ProgramError::InvalidInstructionData,
        "Missing seat for market maker",
    )?;
    assert_with_msg(
        !order_packet.is_take_only(),
        ProgramError::InvalidInstructionData,
        "Order type must be Limit or PostOnly",
    )?;
    assert_with_msg(
        order_packet.no_deposit_or_withdrawal(),
        ProgramError::InvalidInstructionData,
        "Order must be set to use only deposited funds",
    )?;
    process_new_order(
        new_order_context,
        market_context,
        &mut order_packet,
        record_event_fn,
        order_ids,
    )
}

/// This function places multiple Post-Only orders against the specified market.
/// Only users with a "seat" on the market are authorized to perform this action.
///
/// Orders at the same price level will be merged.
pub(crate) fn process_place_multiple_post_only_orders<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
) -> ProgramResult {
    let new_order_context = NewOrderContext::load_post_allowed(market_context, accounts, false)?;
    let multiple_order_packet = MultipleOrderPacket::try_from_slice(data)?;
    assert_with_msg(
        new_order_context.seat_option.is_some(),
        ProgramError::InvalidInstructionData,
        "Missing seat for market maker",
    )?;

    process_multiple_new_orders(
        new_order_context,
        market_context,
        multiple_order_packet,
        record_event_fn,
        order_ids,
        false,
    )
}

/// This function plcaces multiple Post-Only orders against the specified market
/// using only the funds already available to the trader.
/// Only users with sufficient funds and a "seat" on the market are authorized
/// to perform this action.
///
/// Orders at the same price level will be merged.
pub(crate) fn process_place_multiple_post_only_orders_with_free_funds<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
) -> ProgramResult {
    let new_order_context = NewOrderContext::load_post_allowed(market_context, accounts, true)?;
    let multiple_order_packet = MultipleOrderPacket::try_from_slice(data)?;
    assert_with_msg(
        new_order_context.seat_option.is_some(),
        ProgramError::InvalidInstructionData,
        "Missing seat for market maker",
    )?;
    process_multiple_new_orders(
        new_order_context,
        market_context,
        multiple_order_packet,
        record_event_fn,
        order_ids,
        true,
    )
}

fn process_new_order<'a, 'info>(
    new_order_context: NewOrderContext<'a, 'info>,
    market_context: &PhoenixMarketContext<'a, 'info>,
    order_packet: &mut OrderPacket,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
) -> ProgramResult {
    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;
    let NewOrderContext { vault_context, .. } = new_order_context;
    let (quote_lot_size, base_lot_size) = {
        let header = market_info.get_header()?;
        (header.get_quote_lot_size(), header.get_base_lot_size())
    };

    let side = order_packet.side();
    let (
        quote_atoms_to_withdraw,
        quote_atoms_to_deposit,
        base_atoms_to_withdraw,
        base_atoms_to_deposit,
    ) = {
        let clock = Clock::get()?;
        let mut get_clock_fn = || (clock.slot, clock.unix_timestamp as u64);
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market_wrapper = load_with_dispatch_mut(&market_info.size_params, market_bytes)?;

        // If the order should fail silently on insufficient funds, and the trader does not have
        // sufficient funds for the order, return silently without modifying the book.
        if order_packet.fail_silently_on_insufficient_funds() {
            let (base_lots_available, quote_lots_available) = get_available_balances_for_trader(
                &market_wrapper,
                trader.key,
                vault_context.as_ref(),
                base_lot_size,
                quote_lot_size,
            )?;
            if !order_packet_has_sufficient_funds(
                &market_wrapper,
                order_packet,
                base_lots_available,
                quote_lots_available,
            ) {
                return Ok(());
            }
        }

        let (order_id, matching_engine_response) = market_wrapper
            .inner
            .place_order(
                trader.key,
                *order_packet,
                record_event_fn,
                &mut get_clock_fn,
            )
            .ok_or(PhoenixError::NewOrderError)?;

        if let Some(order_id) = order_id {
            order_ids.push(order_id);
        }

        (
            matching_engine_response.num_quote_lots_out * quote_lot_size,
            matching_engine_response.get_deposit_amount_bid_in_quote_lots() * quote_lot_size,
            matching_engine_response.num_base_lots_out * base_lot_size,
            matching_engine_response.get_deposit_amount_ask_in_base_lots() * base_lot_size,
        )
    };
    let header = market_info.get_header()?;
    let quote_params = &header.quote_params;
    let base_params = &header.base_params;

    if quote_atoms_to_withdraw > QuoteAtoms::ZERO || base_atoms_to_withdraw > BaseAtoms::ZERO {
        let status = MarketStatus::from(header.status);
        assert_with_msg(
            status.cross_allowed(),
            ProgramError::InvalidAccountData,
            &format!("Market is not active, market status is {}", status),
        )?;
    }
    if !order_packet.no_deposit_or_withdrawal() {
        if let Some(PhoenixVaultContext {
            base_account,
            quote_account,
            base_vault,
            quote_vault,
            token_program,
        }) = vault_context
        {
            match side {
                Side::Bid => {
                    maybe_invoke_withdraw(
                        market_info.key,
                        &base_params.mint_key,
                        base_params.vault_bump as u8,
                        base_atoms_to_withdraw.as_u64(),
                        &token_program,
                        &base_account,
                        &base_vault,
                    )?;
                    maybe_invoke_deposit(
                        quote_atoms_to_deposit.as_u64(),
                        &token_program,
                        &quote_account,
                        &quote_vault,
                        trader.as_ref(),
                    )?;
                }
                Side::Ask => {
                    maybe_invoke_withdraw(
                        market_info.key,
                        &quote_params.mint_key,
                        quote_params.vault_bump as u8,
                        quote_atoms_to_withdraw.as_u64(),
                        &token_program,
                        &quote_account,
                        &quote_vault,
                    )?;
                    maybe_invoke_deposit(
                        base_atoms_to_deposit.as_u64(),
                        &token_program,
                        &base_account,
                        &base_vault,
                        trader.as_ref(),
                    )?;
                }
            }
        } else {
            // Should never be reached as the account loading logic should fail
            phoenix_log!("WARNING: Vault context was not provided");
            return Err(PhoenixError::NewOrderError.into());
        }
    } else if quote_atoms_to_deposit > QuoteAtoms::ZERO || base_atoms_to_deposit > BaseAtoms::ZERO {
        // Should never execute as the matching engine should return None in this case
        phoenix_log!("WARNING: Deposited amount of funds were insufficient to execute the order");
        return Err(ProgramError::InsufficientFunds);
    }

    Ok(())
}

fn process_multiple_new_orders<'a, 'info>(
    new_order_context: NewOrderContext<'a, 'info>,
    market_context: &PhoenixMarketContext<'a, 'info>,
    multiple_order_packet: MultipleOrderPacket,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
    order_ids: &mut Vec<FIFOOrderId>,
    no_deposit: bool,
) -> ProgramResult {
    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;
    let NewOrderContext { vault_context, .. } = new_order_context;

    let MultipleOrderPacket {
        bids,
        asks,
        client_order_id,
        failed_multiple_limit_order_behavior,
    } = multiple_order_packet;

    let highest_bid = bids
        .iter()
        .map(|bid| bid.price_in_ticks)
        .max_by(|bid1, bid2| bid1.cmp(&bid2))
        .unwrap_or(0);

    let lowest_ask = asks
        .iter()
        .map(|ask| ask.price_in_ticks)
        .min_by(|ask1, ask2| ask1.cmp(&ask2))
        .unwrap_or(u64::MAX);

    if highest_bid >= lowest_ask {
        phoenix_log!("Invalid input. MultipleOrderPacket contains crossing bids and asks");
        return Err(ProgramError::InvalidArgument.into());
    }

    let client_order_id = client_order_id.unwrap_or(0);
    let mut quote_lots_to_deposit = QuoteLots::ZERO;
    let mut base_lots_to_deposit = BaseLots::ZERO;
    let (quote_lot_size, base_lot_size) = {
        let header = market_info.get_header()?;
        (header.get_quote_lot_size(), header.get_base_lot_size())
    };

    {
        let clock = Clock::get()?;
        let mut get_clock_fn = || (clock.slot, clock.unix_timestamp as u64);
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market_wrapper = load_with_dispatch_mut(&market_info.size_params, market_bytes)?;

        let (mut base_lots_available, mut quote_lots_available) =
            get_available_balances_for_trader(
                &market_wrapper,
                trader.key,
                vault_context.as_ref(),
                base_lot_size,
                quote_lot_size,
            )?;

        for (book_orders, side) in [(&bids, Side::Bid), (&asks, Side::Ask)].iter() {
            for CondensedOrder {
                price_in_ticks,
                size_in_base_lots,
                last_valid_slot,
                last_valid_unix_timestamp_in_seconds,
            } in book_orders
                .iter()
                .sorted_by(|o1, o2| match side {
                    Side::Bid => o2.price_in_ticks.cmp(&o1.price_in_ticks),
                    Side::Ask => o1.price_in_ticks.cmp(&o2.price_in_ticks),
                })
                .group_by(|o| {
                    (
                        o.price_in_ticks,
                        o.last_valid_slot,
                        o.last_valid_unix_timestamp_in_seconds,
                    )
                })
                .into_iter()
                .map(
                    |(
                        (price_in_ticks, last_valid_slot, last_valid_unix_timestamp_in_seconds),
                        level,
                    )| CondensedOrder {
                        price_in_ticks,
                        size_in_base_lots: level.fold(0, |acc, o| acc + o.size_in_base_lots),
                        last_valid_slot,
                        last_valid_unix_timestamp_in_seconds,
                    },
                )
            {
                let order_packet = OrderPacket::PostOnly {
                    side: *side,
                    price_in_ticks: Ticks::new(price_in_ticks),
                    num_base_lots: BaseLots::new(size_in_base_lots),
                    client_order_id,
                    reject_post_only: failed_multiple_limit_order_behavior.should_fail_on_cross(),
                    use_only_deposited_funds: no_deposit,
                    last_valid_slot,
                    last_valid_unix_timestamp_in_seconds,
                    fail_silently_on_insufficient_funds: failed_multiple_limit_order_behavior
                        .should_skip_orders_with_insufficient_funds(),
                };

                let matching_engine_response = {
                    if failed_multiple_limit_order_behavior
                        .should_skip_orders_with_insufficient_funds()
                        && !order_packet_has_sufficient_funds(
                            &market_wrapper,
                            &order_packet,
                            base_lots_available,
                            quote_lots_available,
                        )
                    {
                        // Skip this order if the trader does not have sufficient funds
                        continue;
                    }
                    let (order_id, matching_engine_response) = market_wrapper
                        .inner
                        .place_order(trader.key, order_packet, record_event_fn, &mut get_clock_fn)
                        .ok_or(PhoenixError::NewOrderError)?;
                    if let Some(order_id) = order_id {
                        order_ids.push(order_id);
                    }
                    matching_engine_response
                };

                let quote_lots_deposited =
                    matching_engine_response.get_deposit_amount_bid_in_quote_lots();
                let base_lots_deposited =
                    matching_engine_response.get_deposit_amount_ask_in_base_lots();

                if failed_multiple_limit_order_behavior.should_skip_orders_with_insufficient_funds()
                {
                    // Decrement the available funds by the amount that was deposited after each iteration
                    // This should never underflow, but if it does, the program will panic and the transaction will fail
                    quote_lots_available -=
                        quote_lots_deposited + matching_engine_response.num_free_quote_lots_used;
                    base_lots_available -=
                        base_lots_deposited + matching_engine_response.num_free_base_lots_used;
                }

                quote_lots_to_deposit += quote_lots_deposited;
                base_lots_to_deposit += base_lots_deposited;
            }
        }
    }

    if !no_deposit {
        if let Some(PhoenixVaultContext {
            base_account,
            quote_account,
            base_vault,
            quote_vault,
            token_program,
        }) = vault_context
        {
            if !bids.is_empty() {
                maybe_invoke_deposit(
                    (quote_lots_to_deposit * quote_lot_size).as_u64(),
                    &token_program,
                    &quote_account,
                    &quote_vault,
                    trader.as_ref(),
                )?;
            } else {
                assert_with_msg(
                    quote_lots_to_deposit == QuoteLots::ZERO,
                    PhoenixError::NewOrderError,
                    "WARNING: Expected quote_lots_to_deposit to be zero",
                )?;
            }
            if !asks.is_empty() {
                maybe_invoke_deposit(
                    (base_lots_to_deposit * base_lot_size).as_u64(),
                    &token_program,
                    &base_account,
                    &base_vault,
                    trader.as_ref(),
                )?;
            } else {
                assert_with_msg(
                    base_lots_to_deposit == BaseLots::ZERO,
                    PhoenixError::NewOrderError,
                    "WARNING: Expected base_lots_to_deposit to be zero",
                )?;
            }
        } else {
            // Should never be reached as the account loading logic should fail
            phoenix_log!("WARNING: Vault context was not provided");
            return Err(PhoenixError::NewOrderError.into());
        }
    } else if base_lots_to_deposit > BaseLots::ZERO || quote_lots_to_deposit > QuoteLots::ZERO {
        phoenix_log!("Deposited amount of funds were insufficient to execute the order");
        return Err(ProgramError::InsufficientFunds);
    }

    Ok(())
}

fn get_available_balances_for_trader<'a>(
    market_wrapper: &MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
    trader: &Pubkey,
    vault_context: Option<&PhoenixVaultContext>,
    base_lot_size: BaseAtomsPerBaseLot,
    quote_lot_size: QuoteAtomsPerQuoteLot,
) -> Result<(BaseLots, QuoteLots), ProgramError> {
    let (base_lots_free, quote_lots_free) = {
        let trader_index = market_wrapper
            .inner
            .get_trader_index(trader)
            .ok_or(PhoenixError::TraderNotFound)?;
        let trader_state = market_wrapper
            .inner
            .get_trader_state_from_index(trader_index);
        (trader_state.base_lots_free, trader_state.quote_lots_free)
    };
    let (base_lots_available, quote_lots_available) = match vault_context.as_ref() {
        None => (base_lots_free, quote_lots_free),
        Some(ctx) => {
            let quote_account_atoms = ctx.quote_account.amount().map(QuoteAtoms::new)?;
            let base_account_atoms = ctx.base_account.amount().map(BaseAtoms::new)?;
            (
                base_lots_free + base_account_atoms.unchecked_div(base_lot_size),
                quote_lots_free + quote_account_atoms.unchecked_div(quote_lot_size),
            )
        }
    };
    Ok((base_lots_available, quote_lots_available))
}

fn order_packet_has_sufficient_funds<'a>(
    market_wrapper: &MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
    order_packet: &OrderPacket,
    base_lots_available: BaseLots,
    quote_lots_available: QuoteLots,
) -> bool {
    match order_packet.side() {
        Side::Ask => {
            if base_lots_available < order_packet.num_base_lots() {
                phoenix_log!(
                    "Insufficient funds to place order: {} base lots available, {} base lots required",
                    base_lots_available,
                    order_packet.num_base_lots()
                );
                return false;
            }
        }
        Side::Bid => {
            let quote_lots_required = order_packet.get_price_in_ticks()
                * market_wrapper.inner.get_tick_size()
                * order_packet.num_base_lots()
                / market_wrapper.inner.get_base_lots_per_base_unit();

            if quote_lots_available < quote_lots_required {
                phoenix_log!(
                    "Insufficient funds to place order: {} quote lots available, {} quote lots required",
                    quote_lots_available,
                    quote_lots_required
                );
                return false;
            }
        }
    }
    true
}
