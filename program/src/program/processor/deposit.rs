use crate::{
    program::{
        dispatch_market::load_with_dispatch_mut, loaders::DepositContext, token_utils::try_deposit,
        MarketHeader, PhoenixError, PhoenixMarketContext, PhoenixVaultContext,
    },
    quantities::{BaseLots, QuoteLots, WrapperU64},
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{account_info::AccountInfo, entrypoint::ProgramResult, pubkey::Pubkey};
use std::mem::size_of;

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct DepositParams {
    pub quote_lots_to_deposit: u64,
    pub base_lots_to_deposit: u64,
}

pub(crate) fn process_deposit_funds<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
) -> ProgramResult {
    let DepositContext {
        seat: _,
        vault_context:
            PhoenixVaultContext {
                base_account,
                quote_account,
                base_vault,
                quote_vault,
                token_program,
            },
    } = DepositContext::load(market_context, accounts)?;
    let DepositParams {
        quote_lots_to_deposit,
        base_lots_to_deposit,
    } = DepositParams::try_from_slice(data)?;

    let quote_lots = QuoteLots::new(quote_lots_to_deposit);
    let base_lots = BaseLots::new(base_lots_to_deposit);

    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;

    {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        market
            .get_or_register_trader(trader.key)
            .ok_or(PhoenixError::TraderNotFound)?;
        let trader_state = market
            .get_trader_state_mut(trader.key)
            .ok_or(PhoenixError::TraderNotFound)?;
        trader_state.deposit_free_base_lots(base_lots);
        trader_state.deposit_free_quote_lots(quote_lots);
    }

    let header = market_info.get_header()?;

    try_deposit(
        token_program.as_ref(),
        quote_account,
        quote_vault,
        base_account,
        base_vault,
        quote_lots * header.get_quote_lot_size(),
        base_lots * header.get_base_lot_size(),
        trader,
    )?;

    Ok(())
}
