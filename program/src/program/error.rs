use num_enum::IntoPrimitive;
use solana_program::{entrypoint::ProgramResult, msg, program_error::ProgramError};
use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq, IntoPrimitive)]
#[repr(u32)]
pub enum PhoenixError {
    #[error("Invalid market parameters error")]
    InvalidMarketParameters = 0,
    #[error("Invalid exchange authority error")]
    InvalidMarketAuthority = 1,
    #[error("Market deserialization error")]
    FailedToLoadMarketFromAccount = 2,
    #[error("Market already initialized error")]
    MarketAlreadyInitialized = 3,
    #[error("Market is not initialized error")]
    MarketUninitialized = 4,
    #[error("Invalid state transition error")]
    InvalidStateTransition = 5,
    #[error("Invalid market signer error")]
    InvalidMarketSigner = 6,
    #[error("Invalid lot size error")]
    InvalidLotSize = 7,
    #[error("Invalid tick size error")]
    InvalidTickSize = 8,
    #[error("Invalid mint error")]
    InvalidMint = 9,
    #[error("Invalid base vault error")]
    InvalidBaseVault = 10,
    #[error("Invalid quote vault error")]
    InvalidQuoteVault = 11,
    #[error("Invalid base account error")]
    InvalidBaseAccount = 12,
    #[error("Invalid quote account error")]
    InvalidQuoteAccount = 13,
    #[error("Too many events error")]
    TooManyEvents = 14,
    #[error("New order error")]
    NewOrderError = 15,
    #[error("Reduce order error")]
    ReduceOrderError = 16,
    #[error("Cancel multiple orders error")]
    CancelMultipleOrdersError = 17,
    #[error("Withdraw funds error")]
    WithdrawFundsError = 18,
    #[error("Remove empty orders error")]
    RemoveEmptyOrdersError = 19,
    #[error("Trader not found error")]
    TraderNotFound = 20,
    #[error("Invalid seat status")]
    InvalidSeatStatus = 21,
    #[error("Failed to evict trader")]
    EvictionError = 22,
    #[error("Non empty scratch buffer")]
    NonEmptyScratchBuffer = 23,
    #[error("Failed to serialize event")]
    FailedToSerializeEvent = 24,
    #[error("Failed to flush buffer")]
    FailedToFlushBuffer = 25,
}

impl From<PhoenixError> for ProgramError {
    fn from(e: PhoenixError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

#[track_caller]
#[inline(always)]
pub fn assert_with_msg(v: bool, err: impl Into<ProgramError>, msg: &str) -> ProgramResult {
    if v {
        Ok(())
    } else {
        let caller = std::panic::Location::caller();
        msg!("{}. \n{}", msg, caller);
        Err(err.into())
    }
}
