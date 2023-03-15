///! These types are unused in the program, but are used to generate the IDL.
///! The program uses explicit types for all quantities, but these unit types
///! will not be exposed to the client.
///!
///! Instead, these wrapper structs expose all of the quantities as u64s.
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

use crate::{
    program::{MarketSizeParams, TokenParams},
    state::{SelfTradeBehavior, Side},
};

#[derive(Clone, Copy, BorshDeserialize, BorshSerialize)]
#[repr(C)]
struct MarketHeader {
    discriminant: u64,
    status: u64,
    market_size_params: MarketSizeParams,
    base_params: TokenParams,
    base_lot_size: u64,
    quote_params: TokenParams,
    quote_lot_size: u64,
    tick_size_in_quote_atoms_per_base_unit: u64,
    authority: Pubkey,
    fee_recipient: Pubkey,
    market_sequence_number: u64,
    successor: Pubkey,
    raw_base_units_per_base_unit: u32,
    _padding1: u32,
    _padding2: [u64; 32],
}

#[derive(BorshDeserialize, BorshSerialize, Copy, Clone, PartialEq, Eq, Debug)]
enum OrderPacket {
    PostOnly {
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
        reject_post_only: bool,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },
    Limit {
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },
    ImmediateOrCancel {
        side: Side,
        price_in_ticks: Option<u64>,
        num_base_lots: u64,
        num_quote_lots: u64,
        min_base_lots_to_fill: u64,
        min_quote_lots_to_fill: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },
}
