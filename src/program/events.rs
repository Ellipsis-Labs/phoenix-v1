use crate::state::markets::MarketEvent;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct AuditLogHeader {
    pub instruction: u8,
    pub sequence_number: u64,
    pub timestamp: i64,
    pub slot: u64,
    pub market: Pubkey,
    pub signer: Pubkey,
    pub total_events: u16,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FillEvent {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_filled: u64,
    pub base_lots_remaining: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct ReduceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_removed: u64,
    pub base_lots_remaining: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct PlaceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub client_order_id: u128,
    pub price_in_ticks: u64,
    pub base_lots_placed: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct EvictEvent {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_evicted: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FillSummaryEvent {
    pub index: u16,
    pub client_order_id: u128,
    pub total_base_lots_filled: u64,
    pub total_quote_lots_filled: u64,
    pub total_fee_in_quote_lots: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct FeeEvent {
    pub index: u16,
    pub fees_collected_in_quote_lots: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct TimeInForceEvent {
    pub index: u16,
    pub order_sequence_number: u64,
    pub last_valid_slot: u64,
    pub last_valid_unix_timestamp_in_seconds: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub struct ExpiredOrder {
    pub index: u16,
    pub maker_id: Pubkey,
    pub order_sequence_number: u64,
    pub price_in_ticks: u64,
    pub base_lots_removed: u64,
}

#[derive(Debug, Copy, Clone, BorshDeserialize, BorshSerialize)]
pub enum PhoenixMarketEvent {
    Uninitialized,
    Header(AuditLogHeader),
    Fill(FillEvent),
    Place(PlaceEvent),
    Reduce(ReduceEvent),
    Evict(EvictEvent),
    FillSummary(FillSummaryEvent),
    Fee(FeeEvent),
    TimeInForce(TimeInForceEvent),
    ExpiredOrder(ExpiredOrder),
}

impl Default for PhoenixMarketEvent {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl PhoenixMarketEvent {
    pub fn set_index(&mut self, i: u16) {
        match self {
            Self::Fill(FillEvent { index, .. }) => *index = i,
            Self::Place(PlaceEvent { index, .. }) => *index = i,
            Self::Reduce(ReduceEvent { index, .. }) => *index = i,
            Self::FillSummary(FillSummaryEvent { index, .. }) => *index = i,
            Self::Evict(EvictEvent { index, .. }) => *index = i,
            Self::Fee(FeeEvent { index, .. }) => *index = i,
            Self::TimeInForce(TimeInForceEvent { index, .. }) => *index = i,
            Self::ExpiredOrder(ExpiredOrder { index, .. }) => *index = i,
            _ => panic!("Cannot set index on uninitialized or header event"),
        }
    }
}

impl From<MarketEvent<Pubkey>> for PhoenixMarketEvent {
    fn from(e: MarketEvent<Pubkey>) -> Self {
        match e {
            MarketEvent::<Pubkey>::Fill {
                maker_id,
                order_sequence_number,
                price_in_ticks,
                base_lots_filled,
                base_lots_remaining,
            } => Self::Fill(FillEvent {
                maker_id,
                order_sequence_number,
                price_in_ticks: price_in_ticks.into(),
                base_lots_filled: base_lots_filled.into(),
                base_lots_remaining: base_lots_remaining.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::Place {
                order_sequence_number,
                client_order_id,
                price_in_ticks,
                base_lots_placed,
            } => Self::Place(PlaceEvent {
                order_sequence_number,
                client_order_id,
                price_in_ticks: price_in_ticks.into(),
                base_lots_placed: base_lots_placed.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::Reduce {
                order_sequence_number,
                price_in_ticks,
                base_lots_removed,
                base_lots_remaining,
            } => Self::Reduce(ReduceEvent {
                order_sequence_number,
                price_in_ticks: price_in_ticks.into(),
                base_lots_removed: base_lots_removed.into(),
                base_lots_remaining: base_lots_remaining.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::Evict {
                maker_id,
                order_sequence_number,
                price_in_ticks,
                base_lots_evicted,
            } => Self::Evict(EvictEvent {
                maker_id,
                order_sequence_number,
                price_in_ticks: price_in_ticks.into(),
                base_lots_evicted: base_lots_evicted.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::FillSummary {
                client_order_id,
                total_base_lots_filled,
                total_quote_lots_filled,
                total_fee_in_quote_lots,
            } => Self::FillSummary(FillSummaryEvent {
                client_order_id,
                total_base_lots_filled: total_base_lots_filled.into(),
                total_quote_lots_filled: total_quote_lots_filled.into(),
                total_fee_in_quote_lots: total_fee_in_quote_lots.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::Fee {
                fees_collected_in_quote_lots,
            } => Self::Fee(FeeEvent {
                fees_collected_in_quote_lots: fees_collected_in_quote_lots.into(),
                index: 0,
            }),
            MarketEvent::<Pubkey>::TimeInForce {
                order_sequence_number,
                last_valid_slot,
                last_valid_unix_timestamp_in_seconds,
            } => Self::TimeInForce(TimeInForceEvent {
                order_sequence_number,
                last_valid_slot,
                last_valid_unix_timestamp_in_seconds,
                index: 0,
            }),
            MarketEvent::<Pubkey>::ExpiredOrder {
                maker_id,
                order_sequence_number,
                price_in_ticks,
                base_lots_removed,
            } => Self::ExpiredOrder(ExpiredOrder {
                maker_id,
                order_sequence_number,
                price_in_ticks: price_in_ticks.into(),
                base_lots_removed: base_lots_removed.into(),
                index: 0,
            }),
        }
    }
}
