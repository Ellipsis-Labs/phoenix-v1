use borsh::BorshDeserialize;

use crate::quantities::{BaseLots, QuoteLots, Ticks};

#[derive(Debug, Copy, Clone)]
pub enum MarketEvent<MarketTraderId: BorshDeserialize + BorshDeserialize> {
    Fill {
        maker_id: MarketTraderId,
        order_sequence_number: u64,
        price_in_ticks: Ticks,
        base_lots_filled: BaseLots,
        base_lots_remaining: BaseLots,
    },
    Place {
        order_sequence_number: u64,
        client_order_id: u128,
        price_in_ticks: Ticks,
        base_lots_placed: BaseLots,
    },
    Reduce {
        order_sequence_number: u64,
        price_in_ticks: Ticks,
        base_lots_removed: BaseLots,
        base_lots_remaining: BaseLots,
    },
    Evict {
        maker_id: MarketTraderId,
        order_sequence_number: u64,
        price_in_ticks: Ticks,
        base_lots_evicted: BaseLots,
    },
    FillSummary {
        client_order_id: u128,
        total_base_lots_filled: BaseLots,
        total_quote_lots_filled: QuoteLots,
        total_fee_in_quote_lots: QuoteLots,
    },
    Fee {
        fees_collected_in_quote_lots: QuoteLots,
    },
    TimeInForce {
        order_sequence_number: u64,
        last_valid_slot: u64,
        last_valid_unix_timestamp_in_seconds: u64,
    },
}
