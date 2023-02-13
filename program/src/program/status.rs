use super::{assert_with_msg, PhoenixError};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::program_error::ProgramError;
use std::fmt::Display;

#[derive(Debug, Copy, Clone, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
#[repr(u64)]
pub enum MarketStatus {
    Uninitialized,
    /// All new orders, placements, and reductions are accepted. Crossing the spread is permissionless.
    Active,
    /// Only places, reductions and withdrawals are accepted.
    PostOnly,
    /// Only reductions and withdrawals are accepted.
    Paused,
    /// Only reductions and withdrawals are accepted. The exchange authority can forcibly cancel
    /// all orders.
    Closed,
    /// Used to signal the market to be deleted. Can only be called in a Closed state where all orders
    /// and traders are removed from the book
    Tombstoned,
}

impl Display for MarketStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketStatus::Uninitialized => write!(f, "Uninitialized"),
            MarketStatus::Active => write!(f, "Active"),
            MarketStatus::PostOnly => write!(f, "PostOnly"),
            MarketStatus::Paused => write!(f, "Paused"),
            MarketStatus::Closed => write!(f, "Closed"),
            MarketStatus::Tombstoned => write!(f, "Tombstoned"),
        }
    }
}

impl Default for MarketStatus {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl From<u64> for MarketStatus {
    fn from(status: u64) -> Self {
        match status {
            0 => Self::Uninitialized,
            1 => Self::Active,
            2 => Self::PostOnly,
            3 => Self::Paused,
            4 => Self::Closed,
            5 => Self::Tombstoned,
            _ => panic!("Invalid market status"),
        }
    }
}

impl MarketStatus {
    pub fn valid_state_transition(&self, new_state: &MarketStatus) -> bool {
        matches!(
            (self, new_state),
            (MarketStatus::Uninitialized, MarketStatus::PostOnly)
                | (MarketStatus::Active, MarketStatus::PostOnly)
                | (MarketStatus::Active, MarketStatus::Paused)
                | (MarketStatus::Active, MarketStatus::Active)
                | (MarketStatus::PostOnly, MarketStatus::Active)
                | (MarketStatus::PostOnly, MarketStatus::Paused)
                | (MarketStatus::PostOnly, MarketStatus::Closed)
                | (MarketStatus::PostOnly, MarketStatus::PostOnly)
                | (MarketStatus::Closed, MarketStatus::Tombstoned)
                | (MarketStatus::Closed, MarketStatus::Paused)
                | (MarketStatus::Closed, MarketStatus::Closed)
                | (MarketStatus::Closed, MarketStatus::PostOnly)
                | (MarketStatus::Paused, MarketStatus::Active)
                | (MarketStatus::Paused, MarketStatus::PostOnly)
                | (MarketStatus::Paused, MarketStatus::Closed)
                | (MarketStatus::Paused, MarketStatus::Paused)
        )
    }

    pub fn assert_valid_state_transition(
        &self,
        new_state: &MarketStatus,
    ) -> Result<(), ProgramError> {
        assert_with_msg(
            self.valid_state_transition(new_state),
            PhoenixError::InvalidStateTransition,
            "Invalid state transition",
        )
    }

    pub fn cross_allowed(&self) -> bool {
        matches!(self, MarketStatus::Active)
    }

    pub fn post_allowed(&self) -> bool {
        matches!(self, MarketStatus::Active | MarketStatus::PostOnly)
    }

    pub fn reduce_allowed(&self) -> bool {
        matches!(
            self,
            MarketStatus::Active
                | MarketStatus::PostOnly
                | MarketStatus::Paused
                | MarketStatus::Closed
        )
    }

    // TODO: Implement instructions for authority to withdraw funds in a Closed state
    pub fn authority_can_cancel(&self) -> bool {
        matches!(self, MarketStatus::Closed)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
#[repr(u64)]
pub enum SeatApprovalStatus {
    NotApproved,
    Approved,
    Retired,
}

impl From<u64> for SeatApprovalStatus {
    fn from(status: u64) -> Self {
        match status {
            0 => SeatApprovalStatus::NotApproved,
            1 => SeatApprovalStatus::Approved,
            2 => SeatApprovalStatus::Retired,
            _ => panic!("Invalid approval status"),
        }
    }
}

impl Display for SeatApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeatApprovalStatus::NotApproved => write!(f, "NotApproved"),
            SeatApprovalStatus::Approved => write!(f, "Approved"),
            SeatApprovalStatus::Retired => write!(f, "Retired"),
        }
    }
}

impl Default for SeatApprovalStatus {
    fn default() -> Self {
        SeatApprovalStatus::NotApproved
    }
}
