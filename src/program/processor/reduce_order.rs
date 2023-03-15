use crate::{
    program::{
        assert_with_msg, dispatch_market::load_with_dispatch_mut, error::PhoenixError,
        loaders::CancelOrWithdrawContext as Cancel, token_utils::try_withdraw, MarketHeader,
        PhoenixMarketContext, PhoenixVaultContext,
    },
    quantities::{BaseLots, Ticks, WrapperU64},
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

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct CancelOrderParams {
    pub side: Side,
    pub price_in_ticks: u64,
    pub order_sequence_number: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy)]
pub struct ReduceOrderParams {
    pub base_params: CancelOrderParams,
    /// Size of the order to reduce in base lots
    pub size: u64,
}

pub(crate) fn process_reduce_order<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
    withdraw_funds: bool,
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    sol_log_compute_units();
    let ReduceOrderParams { base_params, size } = ReduceOrderParams::try_from_slice(data)?;
    let CancelOrderParams {
        side,
        price_in_ticks,
        order_sequence_number,
    } = base_params;
    let order_id = FIFOOrderId::new(Ticks::new(price_in_ticks), order_sequence_number);

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

    let MatchingEngineResponse {
        num_quote_lots_out,
        num_base_lots_out,
        ..
    } = {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        sol_log_compute_units();
        market
            .reduce_order(
                trader.key,
                &order_id,
                side,
                Some(BaseLots::new(size)),
                vault_context_option.is_some(),
                record_event_fn,
            )
            .ok_or(PhoenixError::ReduceOrderError)?
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
            token_program.as_ref(),
            quote_account.as_ref(),
            quote_vault,
            base_account.as_ref(),
            base_vault,
            num_quote_lots_out * header.get_quote_lot_size(),
            num_base_lots_out * header.get_base_lot_size(),
        )?;
    } else {
        // This case is only reached if the user is reducing orders with free funds
        // In this case, there should be no funds to claim
        assert_with_msg(
            num_quote_lots_out == 0,
            PhoenixError::ReduceOrderError,
            "WARNING: num_quote_lots_out must be 0",
        )?;
        assert_with_msg(
            num_base_lots_out == 0,
            PhoenixError::ReduceOrderError,
            "WARNING: num_base_lots_out must be 0",
        )?;
    }
    Ok(())
}
