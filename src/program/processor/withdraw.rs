use crate::{
    program::{
        dispatch_market::load_with_dispatch_mut,
        error::{assert_with_msg, PhoenixError},
        loaders::CancelOrWithdrawContext as Withdraw,
        token_utils::try_withdraw,
        validation::checkers::phoenix_checkers::MarketAccountInfo,
        MarketHeader, PhoenixMarketContext, PhoenixVaultContext,
    },
    quantities::{BaseLots, QuoteLots, WrapperU64},
    state::MatchingEngineResponse,
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, log::sol_log_compute_units,
    pubkey::Pubkey,
};
use std::mem::size_of;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct WithdrawParams {
    pub quote_lots_to_withdraw: Option<u64>,
    pub base_lots_to_withdraw: Option<u64>,
}

pub(crate) fn process_withdraw_funds<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
) -> ProgramResult {
    let Withdraw { vault_context } = Withdraw::load(market_context, accounts)?;
    let WithdrawParams {
        quote_lots_to_withdraw,
        base_lots_to_withdraw,
    } = WithdrawParams::try_from_slice(data)?;
    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;
    process_withdraw(
        market_info,
        trader.as_ref().clone(),
        vault_context,
        quote_lots_to_withdraw,
        base_lots_to_withdraw,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_withdraw<'a, 'info>(
    market_info: &MarketAccountInfo<'a, 'info>,
    trader: AccountInfo<'info>,
    vault_context: PhoenixVaultContext<'a, 'info>,
    quote_lots_to_withdraw: Option<u64>,
    base_lots_to_withdraw: Option<u64>,
    evict_seat: bool,
) -> ProgramResult {
    sol_log_compute_units();

    let PhoenixVaultContext {
        base_account,
        quote_account,
        base_vault,
        quote_vault,
        token_program,
    } = vault_context;
    let MatchingEngineResponse {
        num_quote_lots_out,
        num_base_lots_out,
        ..
    } = {
        sol_log_compute_units();
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        let response = market
            .claim_funds(
                trader.key,
                quote_lots_to_withdraw.map(QuoteLots::new),
                base_lots_to_withdraw.map(BaseLots::new),
                evict_seat,
            )
            .ok_or(PhoenixError::ReduceOrderError)?;
        sol_log_compute_units();
        if evict_seat {
            assert_with_msg(
                market.get_trader_index(trader.key).is_none(),
                PhoenixError::EvictionError,
                "Trader was not evicted, there are still orders on the book",
            )?;
        }
        response
    };
    sol_log_compute_units();
    let header = market_info.get_header()?;

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

    Ok(())
}
