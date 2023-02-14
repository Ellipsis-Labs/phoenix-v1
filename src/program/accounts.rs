use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use sokoban::node_allocator::ZeroCopy;
use solana_program::{keccak, program_error::ProgramError, pubkey::Pubkey};

use crate::quantities::{
    BaseAtomsPerBaseLot, QuoteAtomsPerBaseUnitPerTick, QuoteAtomsPerQuoteLot, WrapperU64,
};

use super::status::{MarketStatus, SeatApprovalStatus};

/// This function returns the canonical discriminant of the given type. It is the result
/// of hashing together the program ID and the name of the type.
///
/// Suppose a program has an account type named `Foo` and another type named `Bar`.
/// A common attack vector would be to pass an account of type `Bar` to a function
/// expecting an account of type `Foo`, but by checking the discriminants, the function
/// would be able to detect that the `Bar` account is not of the expected type `Foo`.
pub fn get_discriminant<T>() -> Result<u64, ProgramError> {
    let type_name = std::any::type_name::<T>();
    let discriminant = u64::from_le_bytes(
        keccak::hashv(&[crate::ID.as_ref(), type_name.as_bytes()]).as_ref()[..8]
            .try_into()
            .map_err(|_| {
                phoenix_log!("Failed to convert discriminant hash to u64");
                ProgramError::InvalidAccountData
            })?,
    );
    phoenix_log!("Discriminant for {} is {}", type_name, discriminant);
    Ok(discriminant)
}

#[derive(Default, Debug, Copy, Clone, BorshDeserialize, BorshSerialize, Zeroable, Pod)]
#[repr(C)]
pub struct MarketSizeParams {
    pub bids_size: u64,
    pub asks_size: u64,
    pub num_seats: u64,
}
impl ZeroCopy for MarketSizeParams {}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize, Zeroable, Pod)]
#[repr(C)]
pub struct TokenParams {
    /// Number of decimals for the token (e.g. 9 for SOL, 6 for USDC).
    pub decimals: u32,

    /// Bump used for generating the PDA for the market's token vault.
    pub vault_bump: u32,

    /// Pubkey of the token mint.
    pub mint_key: Pubkey,

    /// Pubkey of the token vault.
    pub vault_key: Pubkey,
}
impl ZeroCopy for TokenParams {}

#[derive(Debug, Clone, Copy, Zeroable, Pod)]
#[repr(C)]
pub struct MarketHeader {
    pub discriminant: u64,
    pub status: u64,
    pub market_size_params: MarketSizeParams,
    pub base_params: TokenParams,
    base_lot_size: BaseAtomsPerBaseLot,
    pub quote_params: TokenParams,
    quote_lot_size: QuoteAtomsPerQuoteLot,
    tick_size_in_quote_atoms_per_base_unit: QuoteAtomsPerBaseUnitPerTick,
    pub authority: Pubkey,
    pub fee_recipient: Pubkey,
    pub market_sequence_number: u64,
    pub successor: Pubkey,
    pub raw_base_units_per_base_unit: u32,
    _padding1: u32,
    _padding2: [u64; 32],
}
impl ZeroCopy for MarketHeader {}

impl MarketHeader {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        market_size_params: MarketSizeParams,
        base_params: TokenParams,
        base_lot_size: BaseAtomsPerBaseLot,
        quote_params: TokenParams,
        quote_lot_size: QuoteAtomsPerQuoteLot,
        tick_size_in_quote_atoms_per_base_unit: QuoteAtomsPerBaseUnitPerTick,
        authority: Pubkey,
        successor: Pubkey,
        fee_recipient: Pubkey,
        raw_base_units_per_base_unit: u32,
    ) -> Self {
        Self {
            discriminant: get_discriminant::<MarketHeader>().unwrap(),
            status: MarketStatus::PostOnly as u64,
            market_size_params,
            base_params,
            base_lot_size,
            quote_params,
            quote_lot_size,
            tick_size_in_quote_atoms_per_base_unit,
            authority,
            fee_recipient,
            market_sequence_number: 0,
            successor,
            raw_base_units_per_base_unit,
            _padding1: 0,
            _padding2: [0; 32],
        }
    }

    /// Converts a price from quote atoms per base unit to ticks.
    /// TODO: should probably have an option to round up or round down
    pub fn price_in_ticks(&self, price: u64) -> u64 {
        price / self.tick_size_in_quote_atoms_per_base_unit.as_u64()
    }

    pub fn get_base_lot_size(&self) -> BaseAtomsPerBaseLot {
        self.base_lot_size
    }

    pub fn get_quote_lot_size(&self) -> QuoteAtomsPerQuoteLot {
        self.quote_lot_size
    }

    pub fn get_tick_size_in_quote_atoms_per_base_unit(&self) -> QuoteAtomsPerBaseUnitPerTick {
        self.tick_size_in_quote_atoms_per_base_unit
    }

    pub fn increment_sequence_number(&mut self) {
        self.market_sequence_number += 1;
    }
}

/// This struct represents the state of a seat. Only traders with seats can
/// place limit orders on the exchange. The seat is valid when the approval_status
/// field is set to Approved. The initial state is NotApproved, and the seat will
/// be retired if it is a Retired state.
#[derive(Debug, Clone, Copy, BorshDeserialize, BorshSerialize, Zeroable, Pod)]
#[repr(C)]
pub struct Seat {
    pub discriminant: u64,
    pub market: Pubkey,
    pub trader: Pubkey,
    pub approval_status: u64,
    // Padding
    _padding: [u64; 6],
}

impl ZeroCopy for Seat {}

impl Seat {
    pub fn new_init(market: Pubkey, trader: Pubkey) -> Result<Self, ProgramError> {
        Ok(Self {
            discriminant: get_discriminant::<Seat>()?,
            market,
            trader,
            approval_status: SeatApprovalStatus::NotApproved as u64,
            _padding: [0; 6],
        })
    }
}

// Always run tests before every deploy
#[test]
fn test_valid_discriminants() {
    assert_eq!(
        std::any::type_name::<MarketHeader>(),
        "phoenix::program::accounts::MarketHeader"
    );
    assert_eq!(
        std::any::type_name::<Seat>(),
        "phoenix::program::accounts::Seat"
    );
    assert_eq!(
        get_discriminant::<MarketHeader>().unwrap(),
        8167313896524341111
    );
    assert_eq!(get_discriminant::<Seat>().unwrap(), 2002603505298356104);
}
