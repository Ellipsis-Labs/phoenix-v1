use crate::phoenix_log_authority;
use crate::program::new_order::MultipleOrderPacket;
use crate::program::withdraw::WithdrawParams;
use crate::program::{processor::*, PhoenixInstruction};
use crate::state::{OrderPacket, OrderPacketMetadata};
use borsh::BorshSerialize;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use spl_associated_token_account::get_associated_token_address;

use crate::program::loaders::get_vault_address;
use crate::program::processor::deposit::DepositParams;
use crate::program::validation::loaders::get_seat_address;

pub fn create_new_order_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    order_packet: &OrderPacket,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_new_order_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        order_packet,
    )
}

pub fn create_new_order_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    order_packet: &OrderPacket,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    if order_packet.is_take_only() {
        Instruction {
            program_id: crate::id(),
            accounts: vec![
                AccountMeta::new_readonly(crate::id(), false),
                AccountMeta::new_readonly(phoenix_log_authority::id(), false),
                AccountMeta::new(*market, false),
                AccountMeta::new(*trader, true),
                AccountMeta::new(*base_account, false),
                AccountMeta::new(*quote_account, false),
                AccountMeta::new(base_vault, false),
                AccountMeta::new(quote_vault, false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
            data: [
                PhoenixInstruction::Swap.to_vec(),
                order_packet.try_to_vec().unwrap(),
            ]
            .concat(),
        }
    } else {
        let (seat, _) = get_seat_address(market, trader);
        Instruction {
            program_id: crate::id(),
            accounts: vec![
                AccountMeta::new_readonly(crate::id(), false),
                AccountMeta::new_readonly(phoenix_log_authority::id(), false),
                AccountMeta::new(*market, false),
                AccountMeta::new(*trader, true),
                AccountMeta::new_readonly(seat, false),
                AccountMeta::new(*base_account, false),
                AccountMeta::new(*quote_account, false),
                AccountMeta::new(base_vault, false),
                AccountMeta::new(quote_vault, false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
            data: [
                PhoenixInstruction::PlaceLimitOrder.to_vec(),
                order_packet.try_to_vec().unwrap(),
            ]
            .concat(),
        }
    }
}

pub fn create_new_order_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    order_packet: &OrderPacket,
) -> Instruction {
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new_readonly(seat, false),
        ],
        data: [
            if order_packet.is_take_only() {
                PhoenixInstruction::SwapWithFreeFunds.to_vec()
            } else {
                PhoenixInstruction::PlaceLimitOrderWithFreeFunds.to_vec()
            },
            order_packet.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_new_multiple_order_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    multiple_order_packet: &MultipleOrderPacket,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_new_multiple_order_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        multiple_order_packet,
    )
}

pub fn create_new_multiple_order_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    multiple_order_packet: &MultipleOrderPacket,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new_readonly(seat, false),
            AccountMeta::new(*base_account, false),
            AccountMeta::new(*quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [
            PhoenixInstruction::PlaceMultiplePostOnlyOrders.to_vec(),
            multiple_order_packet.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_new_multiple_order_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    multiple_order_packet: &MultipleOrderPacket,
) -> Instruction {
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new_readonly(seat, false),
        ],
        data: [
            PhoenixInstruction::PlaceMultiplePostOnlyOrdersWithFreeFunds.to_vec(),
            multiple_order_packet.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_cancel_all_order_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
        ],
        data: PhoenixInstruction::CancelAllOrdersWithFreeFunds.to_vec(),
    }
}

pub fn create_cancel_up_to_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    params: &CancelUpToParams,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
        ],
        data: [
            PhoenixInstruction::CancelUpToWithFreeFunds.to_vec(),
            params.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_cancel_multiple_orders_by_id_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    params: &CancelMultipleOrdersByIdParams,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
        ],
        data: [
            PhoenixInstruction::CancelMultipleOrdersByIdWithFreeFunds.to_vec(),
            params.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_reduce_order_with_free_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    params: &ReduceOrderParams,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
        ],
        data: [
            PhoenixInstruction::ReduceOrderWithFreeFunds.to_vec(),
            params.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_deposit_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &DepositParams,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    let (seat, _) = get_seat_address(market, trader);
    create_deposit_funds_instruction_with_custom_token_accounts(
        market,
        trader,
        &seat,
        &base_account,
        &quote_account,
        base,
        quote,
        params,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create_deposit_funds_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    seat: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &DepositParams,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    let ix_data = params.try_to_vec().unwrap();
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new(*seat, false),
            AccountMeta::new(*base_account, false),
            AccountMeta::new(*quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [PhoenixInstruction::DepositFunds.to_vec(), ix_data].concat(),
    }
}

#[allow(clippy::too_many_arguments)]
fn _phoenix_instruction_template<T: BorshSerialize>(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    ix_id: PhoenixInstruction,
    params: Option<&T>,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    let ix_data = match params {
        Some(i) => i.try_to_vec().unwrap(),
        None => vec![],
    };
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new(*base_account, false),
            AccountMeta::new(*quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [[ix_id as u8].to_vec(), ix_data].concat(),
    }
}

fn _phoenix_instruction_template_no_param(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    ix_id: PhoenixInstruction,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*trader, true),
            AccountMeta::new(*base_account, false),
            AccountMeta::new(*quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [ix_id as u8].to_vec(),
    }
}

pub fn reduce_order_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &ReduceOrderParams,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_reduce_order_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        params,
    )
}

pub fn create_reduce_order_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &ReduceOrderParams,
) -> Instruction {
    _phoenix_instruction_template::<ReduceOrderParams>(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::ReduceOrder,
        Some(params),
    )
}

pub fn create_cancel_all_orders_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_cancel_all_orders_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
    )
}

pub fn create_cancel_all_orders_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Instruction {
    _phoenix_instruction_template_no_param(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::CancelAllOrders,
    )
}

pub fn create_cancel_up_to_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &CancelUpToParams,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_cancel_up_to_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        params,
    )
}

pub fn create_cancel_up_to_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &CancelUpToParams,
) -> Instruction {
    _phoenix_instruction_template::<CancelUpToParams>(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::CancelUpTo,
        Some(params),
    )
}

pub fn create_cancel_multiple_orders_by_id_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &CancelMultipleOrdersByIdParams,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_cancel_multiple_orders_by_id_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        params,
    )
}

pub fn create_cancel_multiple_orders_by_id_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &CancelMultipleOrdersByIdParams,
) -> Instruction {
    _phoenix_instruction_template::<CancelMultipleOrdersByIdParams>(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::CancelMultipleOrdersById,
        Some(params),
    )
}

pub fn create_withdraw_funds_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_withdraw_funds_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
    )
}

pub fn create_withdraw_funds_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Instruction {
    _phoenix_instruction_template::<WithdrawParams>(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::WithdrawFunds,
        Some(&WithdrawParams {
            quote_lots_to_withdraw: None,
            base_lots_to_withdraw: None,
        }),
    )
}

pub fn create_withdraw_funds_with_custom_amounts_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    base_lots: u64,
    quote_lots: u64,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    create_withdraw_funds_with_custom_amounts_instruction_with_custom_token_accounts(
        market,
        trader,
        &base_account,
        &quote_account,
        base,
        quote,
        &WithdrawParams {
            quote_lots_to_withdraw: Some(quote_lots),
            base_lots_to_withdraw: Some(base_lots),
        },
    )
}

pub fn create_withdraw_funds_with_custom_amounts_instruction_with_custom_token_accounts(
    market: &Pubkey,
    trader: &Pubkey,
    base_account: &Pubkey,
    quote_account: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    params: &WithdrawParams,
) -> Instruction {
    _phoenix_instruction_template::<WithdrawParams>(
        market,
        trader,
        base_account,
        quote_account,
        base,
        quote,
        PhoenixInstruction::WithdrawFunds,
        Some(params),
    )
}

pub fn create_request_seat_instruction(payer: &Pubkey, market: &Pubkey) -> Instruction {
    let (seat, _) = get_seat_address(market, payer);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*payer, true),
            AccountMeta::new(seat, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: PhoenixInstruction::RequestSeat.to_vec(),
    }
}
