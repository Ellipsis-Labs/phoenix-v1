use std::mem::size_of;

use crate::{
    program::{
        assert_with_msg, load_with_dispatch_mut,
        token_utils::{get_decimal_string, maybe_invoke_withdraw},
        ChangeFeeRecipientContext, CollectFeesContext, MarketHeader, PhoenixMarketContext,
    },
    quantities::{QuoteLots, WrapperU64},
    state::markets::MarketEvent,
};
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

pub(crate) fn process_collect_fees<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    _data: &[u8],
    record_event_fn: &mut dyn FnMut(MarketEvent<Pubkey>),
) -> ProgramResult {
    let CollectFeesContext {
        fee_recipient_token_account,
        quote_vault,
        token_program,
    } = CollectFeesContext::load(market_context, accounts)?;

    let PhoenixMarketContext {
        market_info,
        signer: _,
    } = market_context;

    let num_quote_lots_out = {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
        market.collect_fees(record_event_fn)
    };

    let header = market_info.get_header()?;
    let quote_atoms_collected = num_quote_lots_out * header.get_quote_lot_size();
    phoenix_log!(
        "Collected {} in fees",
        get_decimal_string(quote_atoms_collected.as_u64(), header.quote_params.decimals)
    );

    maybe_invoke_withdraw(
        market_info.key,
        &header.quote_params.mint_key,
        header.quote_params.vault_bump as u8,
        quote_atoms_collected.as_u64(),
        token_program.as_ref(),
        fee_recipient_token_account.as_ref(),
        &quote_vault,
    )?;
    Ok(())
}

pub(crate) fn process_change_fee_recipient<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    _data: &[u8],
) -> ProgramResult {
    let ChangeFeeRecipientContext {
        new_fee_recipient,
        previous_fee_recipient,
    } = ChangeFeeRecipientContext::load(market_context, accounts)?;
    let PhoenixMarketContext { market_info, .. } = market_context;

    let uncollected_fees = {
        let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
        load_with_dispatch_mut(&market_info.size_params, market_bytes)?
            .inner
            .get_uncollected_fee_amount()
    };

    let mut header = market_info.get_header_mut()?;
    if uncollected_fees > QuoteLots::ZERO {
        assert_with_msg(
            previous_fee_recipient.is_some(),
            ProgramError::MissingRequiredSignature,
            "Previous fee recipient must sign if there are uncollected fees",
        )?;
    }
    header.fee_recipient = *new_fee_recipient.key;
    Ok(())
}
