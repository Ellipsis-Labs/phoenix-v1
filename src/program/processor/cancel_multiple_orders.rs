use crate::{
    program::{
        dispatch_market::load_with_dispatch_mut, loaders::CancelOrWithdrawContext as Cancel,
        token_utils::try_withdraw, validation::checkers::phoenix_checkers::MarketAccountInfo,
        MarketHeader, PhoenixMarketContext, PhoenixVaultContext,
    },
    quantities::{Ticks, WrapperU64},
    state::{
        markets::{FIFOOrderId, MarketEvent},
        MatchingEngineResponse, Side,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, log::sol_log_compute_units,
    pubkey::Pubkey,
};
use std::mem::size_of;

use super::CancelOrderParams;

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct CancelUpToParams {
    pub side: Side,
    pub tick_limit: Option<u64>,
    pub num_orders_to_search: Option<u32>,
    pub num_orders_to_cancel: Option<u32>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct CancelMultipleOrdersByIdParams {
    pub orders: Vec<CancelOrderParams>,
}

pub(crate) fn process_cancel_all_orders<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    _data: &[u8],
    withdraw_funds: bool,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let vault_context_option = if withdraw_funds {
        let Cancel { vault_context } = Cancel::load(market_context, accounts)?;
        Some(vault_context)
    } else {
        None
    };

    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;

    let claim_funds = vault_context_option.is_some();
    let MatchingEngineResponse {
        num_base_lots_out,
        num_quote_lots_out,
        ..
    } = {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        sol_log_compute_units();
        market
            .cancel_all_orders(trader.key, claim_funds, record_event_fn)
            .unwrap_or_default()
    };
    sol_log_compute_units();

    let header = market_info.get_header()?;

    if let Some(PhoenixVaultContext {
        base_account,
        quote_account,
        base_vault,
        quote_vault,
        token_program,
    }) = vault_context_option
    {
        try_withdraw(
            market_info.key,
            &header.base_params,
            &header.quote_params,
            &token_program,
            quote_account.as_ref(),
            quote_vault,
            base_account.as_ref(),
            base_vault,
            num_quote_lots_out * header.get_quote_lot_size(),
            num_base_lots_out * header.get_base_lot_size(),
        )?;
    }

    drop(header);
    Ok(())
}

pub(crate) fn process_cancel_up_to<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    withdraw_funds: bool,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let vault_context_option = if withdraw_funds {
        let Cancel { vault_context } = Cancel::load(market_context, accounts)?;
        Some(vault_context)
    } else {
        None
    };

    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;

    let params = CancelUpToParams::try_from_slice(data)?;
    process_cancel_orders(
        market_info,
        trader.key,
        vault_context_option,
        params,
        record_event_fn,
    )
}

pub(crate) fn process_cancel_multiple_orders_by_id<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    withdraw_funds: bool,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let vault_context_option = if withdraw_funds {
        let Cancel { vault_context } = Cancel::load(market_context, accounts)?;
        Some(vault_context)
    } else {
        None
    };

    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;

    let cancel_params = CancelMultipleOrdersByIdParams::try_from_slice(data)?;
    if cancel_params.orders.is_empty() {
        phoenix_log!("No orders to cancel");
        return Ok(());
    }

    let MatchingEngineResponse {
        num_quote_lots_out,
        num_base_lots_out,
        ..
    } = {
        sol_log_compute_units();
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        let orders_to_cancel = cancel_params
            .orders
            .iter()
            .filter_map(
                |CancelOrderParams {
                     side,
                     price_in_ticks,
                     order_sequence_number,
                 }| {
                    if *side == Side::from_order_sequence_number(*order_sequence_number) {
                        Some(FIFOOrderId::new(
                            Ticks::new(*price_in_ticks),
                            *order_sequence_number,
                        ))
                    } else {
                        None
                    }
                },
            )
            .collect::<Vec<_>>();

        market
            .cancel_multiple_orders_by_id(
                trader.key,
                &orders_to_cancel,
                vault_context_option.is_some(),
                record_event_fn,
            )
            .unwrap_or_default()
    };
    sol_log_compute_units();

    let header = market_info.get_header()?;

    if let Some(PhoenixVaultContext {
        base_account,
        quote_account,
        base_vault,
        quote_vault,
        token_program,
    }) = vault_context_option
    {
        try_withdraw(
            market_info.key,
            &header.base_params,
            &header.quote_params,
            &token_program,
            quote_account.as_ref(),
            quote_vault,
            base_account.as_ref(),
            base_vault,
            num_quote_lots_out * header.get_quote_lot_size(),
            num_base_lots_out * header.get_base_lot_size(),
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_cancel_orders<'a, 'info>(
    market_info: &MarketAccountInfo<'a, 'info>,
    trader_key: &Pubkey,
    vault_context_option: Option<PhoenixVaultContext<'a, 'info>>,
    cancel_params: CancelUpToParams,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let CancelUpToParams {
        side,
        tick_limit,
        num_orders_to_search,
        num_orders_to_cancel,
    } = cancel_params;

    let claim_funds = vault_context_option.is_some();
    let released = {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        sol_log_compute_units();
        market
            .cancel_up_to(
                trader_key,
                side,
                num_orders_to_search.map(|x| x as usize),
                num_orders_to_cancel.map(|x| x as usize),
                tick_limit.map(Ticks::new),
                claim_funds,
                record_event_fn,
            )
            .unwrap_or_default()
    };
    sol_log_compute_units();

    let header = market_info.get_header()?;

    let MatchingEngineResponse {
        num_quote_lots_out,
        num_base_lots_out,
        ..
    } = released;
    if let Some(PhoenixVaultContext {
        base_account,
        quote_account,
        base_vault,
        quote_vault,
        token_program,
    }) = vault_context_option
    {
        try_withdraw(
            market_info.key,
            &header.base_params,
            &header.quote_params,
            &token_program,
            quote_account.as_ref(),
            quote_vault,
            base_account.as_ref(),
            base_vault,
            num_quote_lots_out * header.get_quote_lot_size(),
            num_base_lots_out * header.get_base_lot_size(),
        )?;
    }

    Ok(())
}
