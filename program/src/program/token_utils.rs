use std::{
    fmt::Display,
    ops::{Div, Rem},
};

use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    pubkey::Pubkey,
};

use crate::quantities::{BaseAtoms, QuoteAtoms, WrapperU64};

use super::{checkers::TokenAccountInfo, TokenParams};

#[allow(clippy::too_many_arguments)]
pub(crate) fn try_withdraw<'a, 'info>(
    market_key: &Pubkey,
    base_params: &TokenParams,
    quote_params: &TokenParams,
    token_program: &AccountInfo<'info>,
    quote_account: &AccountInfo<'info>,
    quote_vault: TokenAccountInfo<'a, 'info>,
    base_account: &AccountInfo<'info>,
    base_vault: TokenAccountInfo<'a, 'info>,
    quote_atoms_to_withdraw: QuoteAtoms,
    base_atoms_to_withdraw: BaseAtoms,
) -> ProgramResult {
    for (withdraw_vault, withdraw_account, withdraw_amount, params) in [
        (
            quote_vault,
            quote_account,
            quote_atoms_to_withdraw.as_u64(),
            quote_params,
        ),
        (
            base_vault,
            base_account,
            base_atoms_to_withdraw.as_u64(),
            base_params,
        ),
    ] {
        maybe_invoke_withdraw(
            market_key,
            &params.mint_key,
            params.vault_bump as u8,
            withdraw_amount,
            token_program,
            withdraw_account,
            &withdraw_vault,
        )?;
    }
    Ok(())
}

pub(crate) fn maybe_invoke_withdraw<'a, 'info>(
    market_key: &Pubkey,
    mint_key: &Pubkey,
    bump: u8,
    withdraw_amount: u64,
    token_program: &AccountInfo<'info>,
    withdraw_account: &AccountInfo<'info>,
    withdraw_vault: &'a TokenAccountInfo<'a, 'info>,
) -> ProgramResult {
    if withdraw_amount != 0 {
        invoke_signed(
            &spl_token::instruction::transfer(
                token_program.key,
                withdraw_vault.key,
                withdraw_account.key,
                withdraw_vault.key,
                &[],
                withdraw_amount,
            )?,
            &[
                token_program.clone(),
                withdraw_vault.as_ref().clone(),
                withdraw_account.clone(),
            ],
            &[&[b"vault", market_key.as_ref(), mint_key.as_ref(), &[bump]]],
        )?;
    }
    Ok(())
}

pub(crate) fn maybe_invoke_deposit<'a, 'info>(
    deposit_amount: u64,
    token_program: &AccountInfo<'info>,
    deposit_account: &'a TokenAccountInfo<'a, 'info>,
    deposit_vault: &'a TokenAccountInfo<'a, 'info>,
    trader: &AccountInfo<'info>,
) -> ProgramResult {
    if deposit_amount > 0 {
        invoke(
            &spl_token::instruction::transfer(
                token_program.key,
                deposit_account.key,
                deposit_vault.key,
                trader.key,
                &[],
                deposit_amount,
            )?,
            &[
                token_program.as_ref().clone(),
                deposit_account.as_ref().clone(),
                deposit_vault.as_ref().clone(),
                trader.as_ref().clone(),
            ],
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn try_deposit<'a, 'info>(
    token_program: &AccountInfo<'info>,
    quote_account: TokenAccountInfo<'a, 'info>,
    quote_vault: TokenAccountInfo<'a, 'info>,
    base_account: TokenAccountInfo<'a, 'info>,
    base_vault: TokenAccountInfo<'a, 'info>,
    quote_amount: QuoteAtoms,
    base_amount: BaseAtoms,
    trader: &AccountInfo<'info>,
) -> ProgramResult {
    for (deposit_vault, deposit_account, deposit_amount) in [
        (quote_vault, quote_account, quote_amount.as_u64()),
        (base_vault, base_account, base_amount.as_u64()),
    ] {
        maybe_invoke_deposit(
            deposit_amount,
            token_program,
            &deposit_account,
            &deposit_vault,
            trader,
        )?;
    }
    Ok(())
}

pub fn get_decimal_string<N: Display + Div + Rem + Copy + TryFrom<u64>>(
    amount: N,
    decimals: u32,
) -> String
where
    <N as Rem>::Output: std::fmt::Display,
    <N as Div>::Output: std::fmt::Display,
    <N as TryFrom<u64>>::Error: std::fmt::Debug,
{
    let scale = N::try_from(10_u64.pow(decimals)).unwrap();
    let lhs = amount / scale;
    let rhs = format!("{:0width$}", (amount % scale), width = decimals as usize).replace('-', ""); // remove negative sign from rhs
    format!("{}.{}", lhs, rhs.trim_end_matches('0'))
}
