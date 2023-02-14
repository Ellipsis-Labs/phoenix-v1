use crate::phoenix_log_authority;
use crate::program::status::{MarketStatus, SeatApprovalStatus};
use crate::program::{
    get_market_size, processor::*, MarketHeader, MarketSizeParams, PhoenixInstruction,
};
use crate::state::Side;
use borsh::BorshSerialize;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
};
use spl_associated_token_account::get_associated_token_address;

use crate::program::loaders::get_vault_address;
use crate::program::validation::loaders::get_seat_address;

#[allow(clippy::too_many_arguments)]
pub fn create_initialize_market_instructions(
    market: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    market_creator: &Pubkey,
    header_params: MarketSizeParams,
    num_quote_lots_per_quote_unit: u64,
    num_base_lots_per_base_unit: u64,
    tick_size_in_quote_lots_per_base_unit: u64,
    taker_fee_bps: u16,
    fee_collector: &Pubkey,
    raw_base_units_per_base_unit: Option<u32>,
) -> Result<Vec<Instruction>, ProgramError> {
    let space = std::mem::size_of::<MarketHeader>() + get_market_size(&header_params)?;
    Ok(vec![
        system_instruction::create_account(
            market_creator,
            market,
            Rent::default().minimum_balance(space),
            space as u64,
            &crate::id(),
        ),
        create_initialize_market_instruction(
            market,
            base,
            quote,
            market_creator,
            header_params,
            num_quote_lots_per_quote_unit,
            num_base_lots_per_base_unit,
            tick_size_in_quote_lots_per_base_unit,
            taker_fee_bps,
            fee_collector,
            raw_base_units_per_base_unit,
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn create_initialize_market_instructions_default(
    market: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    market_creator: &Pubkey,
    header_params: MarketSizeParams,
    num_quote_lots_per_quote_unit: u64,
    num_base_lots_per_base_unit: u64,
    tick_size_in_quote_lots_per_base_unit: u64,
    taker_fee_bps: u16,
    raw_base_units_per_base_unit: Option<u32>,
) -> Result<Vec<Instruction>, ProgramError> {
    let space = std::mem::size_of::<MarketHeader>() + get_market_size(&header_params)?;
    Ok(vec![
        system_instruction::create_account(
            market_creator,
            market,
            Rent::default().minimum_balance(space),
            space as u64,
            &crate::id(),
        ),
        create_initialize_market_instruction(
            market,
            base,
            quote,
            market_creator,
            header_params,
            num_quote_lots_per_quote_unit,
            num_base_lots_per_base_unit,
            tick_size_in_quote_lots_per_base_unit,
            taker_fee_bps,
            market_creator,
            raw_base_units_per_base_unit,
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
pub fn create_initialize_market_instruction(
    market: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    market_creator: &Pubkey,
    header_params: MarketSizeParams,
    num_quote_lots_per_quote_unit: u64,
    num_base_lots_per_base_unit: u64,
    tick_size_in_quote_lots_per_base_unit: u64,
    taker_fee_bps: u16,
    fee_collector: &Pubkey,
    raw_base_units_per_base_unit: Option<u32>,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*market_creator, true),
            AccountMeta::new_readonly(*base, false),
            AccountMeta::new_readonly(*quote, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [
            PhoenixInstruction::InitializeMarket.to_vec(),
            InitializeParams {
                market_size_params: header_params,
                num_quote_lots_per_quote_unit,
                num_base_lots_per_base_unit,
                tick_size_in_quote_lots_per_base_unit,
                taker_fee_bps,
                fee_collector: *fee_collector,
                raw_base_units_per_base_unit,
            }
            .try_to_vec()
            .unwrap(),
        ]
        .concat(),
    }
}

pub fn create_evict_seat_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    trader: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Instruction {
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    let (seat, _) = get_seat_address(market, trader);
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*trader, false),
            AccountMeta::new_readonly(seat, false),
            AccountMeta::new(base_account, false),
            AccountMeta::new(quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: PhoenixInstruction::EvictSeat.to_vec(),
    }
}

pub fn create_claim_authority_instruction(authority: &Pubkey, market: &Pubkey) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data: PhoenixInstruction::ClaimAuthority.to_vec(),
    }
}

pub fn create_name_successor_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    successor: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
        ],
        data: [
            PhoenixInstruction::NameSuccessor.to_vec(),
            successor.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_change_market_status_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    status: MarketStatus,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new(*authority, true),
        ],
        data: [
            PhoenixInstruction::ChangeMarketStatus.to_vec(),
            status.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_request_seat_authorized_instruction(
    authority: &Pubkey,
    payer: &Pubkey,
    market: &Pubkey,
    trader: &Pubkey,
) -> Instruction {
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*payer, true),
            AccountMeta::new_readonly(*trader, false),
            AccountMeta::new(seat, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: PhoenixInstruction::RequestSeatAuthorized.to_vec(),
    }
}

pub fn create_change_seat_status_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    trader: &Pubkey,
    status: SeatApprovalStatus,
) -> Instruction {
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(seat, false),
        ],
        data: [
            PhoenixInstruction::ChangeSeatStatus.to_vec(),
            status.try_to_vec().unwrap(),
        ]
        .concat(),
    }
}

pub fn create_collect_fees_instruction_default(
    market: &Pubkey,
    sweeper: &Pubkey,
    fee_collector: &Pubkey,
    quote_mint: &Pubkey,
) -> Instruction {
    let quote_account = get_associated_token_address(fee_collector, quote_mint);
    create_collect_fees_instruction(market, sweeper, &quote_account, quote_mint)
}

pub fn create_collect_fees_instruction(
    market: &Pubkey,
    sweeper: &Pubkey,
    quote_account: &Pubkey,
    quote_mint: &Pubkey,
) -> Instruction {
    let (quote_vault, _) = get_vault_address(market, quote_mint);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*sweeper, true),
            AccountMeta::new(*quote_account, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: PhoenixInstruction::CollectFees.to_vec(),
    }
}

pub fn create_change_fee_recipient_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    new_fee_recipient: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new_readonly(*new_fee_recipient, false),
        ],
        data: [PhoenixInstruction::ChangeFeeRecipient.to_vec()].concat(),
    }
}

pub fn create_change_fee_recipient_with_unclaimed_fees_instruction(
    authority: &Pubkey,
    market: &Pubkey,
    new_fee_recipient: &Pubkey,
    current_fee_recipient: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new_readonly(*new_fee_recipient, false),
            AccountMeta::new_readonly(*current_fee_recipient, true),
        ],
        data: [PhoenixInstruction::ChangeFeeRecipient.to_vec()].concat(),
    }
}

pub fn create_force_cancel_orders_instructions(
    market: &Pubkey,
    trader: &Pubkey,
    market_authority: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
) -> Vec<Instruction> {
    vec![
        create_force_cancel_orders_instruction(
            market,
            trader,
            market_authority,
            base,
            quote,
            Side::Bid,
        ),
        create_force_cancel_orders_instruction(
            market,
            trader,
            market_authority,
            base,
            quote,
            Side::Ask,
        ),
    ]
}

fn create_force_cancel_orders_instruction(
    market: &Pubkey,
    trader: &Pubkey,
    market_authority: &Pubkey,
    base: &Pubkey,
    quote: &Pubkey,
    side: Side,
) -> Instruction {
    let (base_vault, _) = get_vault_address(market, base);
    let (quote_vault, _) = get_vault_address(market, quote);
    let base_account = get_associated_token_address(trader, base);
    let quote_account = get_associated_token_address(trader, quote);
    let (seat, _) = get_seat_address(market, trader);
    Instruction {
        program_id: crate::id(),
        accounts: vec![
            AccountMeta::new_readonly(crate::id(), false),
            AccountMeta::new_readonly(phoenix_log_authority::id(), false),
            AccountMeta::new(*market, false),
            AccountMeta::new_readonly(*market_authority, true),
            AccountMeta::new_readonly(*trader, false),
            AccountMeta::new_readonly(seat, false),
            AccountMeta::new(base_account, false),
            AccountMeta::new(quote_account, false),
            AccountMeta::new(base_vault, false),
            AccountMeta::new(quote_vault, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: [
            PhoenixInstruction::ForceCancelOrders.to_vec(),
            CancelUpToParams {
                side,
                tick_limit: None,
                num_orders_to_cancel: None,
                num_orders_to_search: None,
            }
            .try_to_vec()
            .unwrap(),
        ]
        .concat(),
    }
}
