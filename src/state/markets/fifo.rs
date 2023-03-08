use super::Market;
use super::MarketEvent;
use super::OrderId;
use super::RestingOrder;
use super::WritableMarket;
use crate::quantities::AdjustedQuoteLots;
use crate::quantities::BaseLots;
use crate::quantities::BaseLotsPerBaseUnit;
use crate::quantities::QuoteLots;
use crate::quantities::QuoteLotsPerBaseUnit;
use crate::quantities::QuoteLotsPerBaseUnitPerTick;
use crate::quantities::Ticks;
use crate::quantities::WrapperU64;
use crate::state::inflight_order::InflightOrder;
use crate::state::matching_engine_response::MatchingEngineResponse;
use crate::state::*;
use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use phoenix_log;
use sokoban::node_allocator::{NodeAllocatorMap, OrderedNodeAllocatorMap, ZeroCopy, SENTINEL};
use sokoban::{FromSlice, RedBlackTree};
use std::fmt::Debug;

#[repr(C)]
#[derive(Eq, PartialEq, Debug, Default, Copy, Clone, Zeroable, Pod)]
pub struct FIFOOrderId {
    /// The price of the order, in ticks. Each market has a designated
    /// tick size (some number of quote lots per base unit) that is used to convert the price to ticks.
    /// For example, if the tick size is 0.01, then a price of 1.23 is converted to 123 ticks.
    /// If the quote lot size is 0.001, this means that there is a spacing of 10 quote lots
    /// in between each tick.
    pub price_in_ticks: Ticks,

    /// This is the unique identifier of the order, which is used to determine the side of the order.
    /// It is derived from the sequence number of the market.
    ///
    /// If the order is a bid, the sequence number will have its bits inverted, and if it is an ask,
    /// the sequence number will be used as is.
    ///
    /// The way to identify the side of the order is to check the leading bit of `order_id`.
    /// A leading bit of 0 indicates an ask, and a leading bit of 1 indicates a bid. See Side::from_order_id.
    pub order_sequence_number: u64,
}

impl OrderId for FIFOOrderId {
    fn price_in_ticks(&self) -> u64 {
        self.price_in_ticks.as_u64()
    }
}

impl FIFOOrderId {
    pub fn new_from_untyped(price_in_ticks: u64, order_sequence_number: u64) -> Self {
        FIFOOrderId {
            price_in_ticks: Ticks::new(price_in_ticks),
            order_sequence_number,
        }
    }

    pub fn new(price_in_ticks: Ticks, order_sequence_number: u64) -> Self {
        FIFOOrderId {
            price_in_ticks,
            order_sequence_number,
        }
    }
}

impl PartialOrd for FIFOOrderId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // The ordering of the `FIFOOrderId` struct is determined by the price of the order. If the price is the same,
        // then the order with the lower sequence number is considered to be the lower order.
        //
        // Asks are sorted in ascending order, and bids are sorted in descending order.
        let (tick_cmp, seq_cmp) = match Side::from_order_sequence_number(self.order_sequence_number)
        {
            Side::Bid => (
                other.price_in_ticks.partial_cmp(&self.price_in_ticks)?,
                other
                    .order_sequence_number
                    .partial_cmp(&self.order_sequence_number)?,
            ),
            Side::Ask => (
                self.price_in_ticks.partial_cmp(&other.price_in_ticks)?,
                self.order_sequence_number
                    .partial_cmp(&other.order_sequence_number)?,
            ),
        };
        if tick_cmp == std::cmp::Ordering::Equal {
            Some(seq_cmp)
        } else {
            Some(tick_cmp)
        }
    }
}

impl Ord for FIFOOrderId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, Zeroable, Pod)]
pub struct FIFORestingOrder {
    pub trader_index: u64,
    pub num_base_lots: BaseLots, // Number of base lots quoted
    pub last_valid_slot: u64,
    pub last_valid_unix_timestamp_in_seconds: u64,
}

impl FIFORestingOrder {
    pub fn new_default(trader_index: u64, num_base_lots: BaseLots) -> Self {
        FIFORestingOrder {
            trader_index,
            num_base_lots,
            last_valid_slot: 0,
            last_valid_unix_timestamp_in_seconds: 0,
        }
    }

    pub fn new(
        trader_index: u64,
        num_base_lots: BaseLots,
        last_valid_slot: Option<u64>,
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    ) -> Self {
        FIFORestingOrder {
            trader_index,
            num_base_lots,
            last_valid_slot: last_valid_slot.unwrap_or(0),
            last_valid_unix_timestamp_in_seconds: last_valid_unix_timestamp_in_seconds.unwrap_or(0),
        }
    }

    pub fn new_with_last_valid_slot(
        trader_index: u64,
        num_base_lots: BaseLots,
        last_valid_slot: u64,
    ) -> Self {
        FIFORestingOrder {
            trader_index,
            num_base_lots,
            last_valid_slot,
            last_valid_unix_timestamp_in_seconds: 0,
        }
    }

    pub fn new_with_last_valid_unix_timestamp(
        trader_index: u64,
        num_base_lots: BaseLots,
        last_valid_unix_timestamp_in_seconds: u64,
    ) -> Self {
        FIFORestingOrder {
            trader_index,
            num_base_lots,
            last_valid_slot: 0,
            last_valid_unix_timestamp_in_seconds,
        }
    }

    pub fn is_expired(&self, current_slot: u64, current_unix_timestamp_in_seconds: u64) -> bool {
        (self.last_valid_slot != 0 && self.last_valid_slot < current_slot)
            || (self.last_valid_unix_timestamp_in_seconds != 0
                && self.last_valid_unix_timestamp_in_seconds < current_unix_timestamp_in_seconds)
    }
}

impl RestingOrder for FIFORestingOrder {
    fn size(&self) -> u64 {
        self.num_base_lots.as_u64()
    }
}

#[repr(C)]
#[derive(Default, Copy, Clone, Zeroable)]
pub struct FIFOMarket<
    MarketTraderId: Debug
        + PartialOrd
        + Ord
        + Default
        + Copy
        + Clone
        + Zeroable
        + Pod
        + BorshDeserialize
        + BorshSerialize,
    const BIDS_SIZE: usize,
    const ASKS_SIZE: usize,
    const NUM_SEATS: usize,
> {
    /// Padding
    pub _padding: [u64; 32],

    /// Number of base lots in a base unit. For example, if the lot size is 0.001 SOL, then base_lots_per_base_unit is 1000.
    pub base_lots_per_base_unit: BaseLotsPerBaseUnit,

    /// Tick size in quote lots per base unit. For example, if the tick size is 0.01 USDC and the quote lot size is 0.001 USDC, then tick_size_in_quote_lots_per_base_unit is 10.
    pub tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,

    /// The sequence number of the next event.
    order_sequence_number: u64,

    /// There are no maker fees. Taker fees are charged on the quote lots transacted in the trade, in basis points.
    pub taker_fee_bps: u64,

    /// Amount of fees collected from the market in its lifetime, in quote lots.
    collected_quote_lot_fees: QuoteLots,

    /// Amount of unclaimed fees accrued to the market, in quote lots.
    unclaimed_quote_lot_fees: QuoteLots,

    /// Red-black tree representing the bids in the order book.
    pub bids: RedBlackTree<FIFOOrderId, FIFORestingOrder, BIDS_SIZE>,

    /// Red-black tree representing the asks in the order book.
    pub asks: RedBlackTree<FIFOOrderId, FIFORestingOrder, ASKS_SIZE>,

    /// Red-black tree representing the authorized makers in the market.
    pub traders: RedBlackTree<MarketTraderId, TraderState, NUM_SEATS>,
}

unsafe impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > Pod for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}

impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > FromSlice for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    fn new_from_slice(data: &mut [u8]) -> &mut Self {
        let market = Self::load_mut_bytes(data).unwrap();
        assert_eq!(market.base_lots_per_base_unit, BaseLotsPerBaseUnit::ZERO);
        assert_eq!(market.order_sequence_number, 0);
        market.initialize();
        market
    }
}

impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > ZeroCopy for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
}

impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > Market<MarketTraderId, FIFOOrderId, FIFORestingOrder, OrderPacket>
    for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    fn get_data_size(&self) -> usize {
        std::mem::size_of::<Self>()
    }

    fn get_taker_fee_bps(&self) -> u64 {
        self.taker_fee_bps
    }

    fn get_tick_size(&self) -> QuoteLotsPerBaseUnitPerTick {
        self.tick_size_in_quote_lots_per_base_unit
    }

    fn get_base_lots_per_base_unit(&self) -> BaseLotsPerBaseUnit {
        self.base_lots_per_base_unit
    }

    fn get_sequence_number(&self) -> u64 {
        self.order_sequence_number
    }

    fn get_collected_fee_amount(&self) -> QuoteLots {
        self.collected_quote_lot_fees
    }

    fn get_uncollected_fee_amount(&self) -> QuoteLots {
        self.unclaimed_quote_lot_fees
    }

    fn get_registered_traders(&self) -> &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState> {
        &self.traders as &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>
    }

    fn get_trader_state(&self, trader_id: &MarketTraderId) -> Option<&TraderState> {
        self.get_registered_traders().get(trader_id)
    }

    fn get_trader_state_from_index(&self, index: u32) -> &TraderState {
        &self.traders.get_node(index).value
    }

    #[inline(always)]
    fn get_trader_index(&self, trader_id: &MarketTraderId) -> Option<u32> {
        let addr = self.traders.get_addr(trader_id);
        if addr == SENTINEL {
            None
        } else {
            Some(addr)
        }
    }

    fn get_trader_id_from_index(&self, trader_index: u32) -> MarketTraderId {
        self.traders.get_node(trader_index).key
    }

    #[inline(always)]
    fn get_book(&self, side: Side) -> &dyn OrderedNodeAllocatorMap<FIFOOrderId, FIFORestingOrder> {
        match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        }
    }
}

impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > WritableMarket<MarketTraderId, FIFOOrderId, FIFORestingOrder, OrderPacket>
    for FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    fn initialize_with_params(
        &mut self,
        tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
        base_lots_per_base_unit: BaseLotsPerBaseUnit,
    ) {
        self.initialize_with_params_inner(
            tick_size_in_quote_lots_per_base_unit,
            base_lots_per_base_unit,
        );
    }

    fn set_fee(&mut self, taker_fee_bps: u64) {
        self.taker_fee_bps = taker_fee_bps;
    }

    fn get_registered_traders_mut(
        &mut self,
    ) -> &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState> {
        &mut self.traders as &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>
    }

    fn get_trader_state_mut(&mut self, trader_id: &MarketTraderId) -> Option<&mut TraderState> {
        self.get_registered_traders_mut().get_mut(trader_id)
    }

    fn get_trader_state_from_index_mut(&mut self, index: u32) -> &mut TraderState {
        &mut self.traders.get_node_mut(index).value
    }

    #[inline(always)]
    fn get_book_mut(
        &mut self,
        side: Side,
    ) -> &mut dyn OrderedNodeAllocatorMap<FIFOOrderId, FIFORestingOrder> {
        match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        }
    }

    fn place_order(
        &mut self,
        trader_id: &MarketTraderId,
        order_packet: OrderPacket,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
        get_clock_fn: &mut dyn FnMut() -> (u64, u64),
    ) -> Option<(Option<FIFOOrderId>, MatchingEngineResponse)> {
        self.place_order_inner(trader_id, order_packet, record_event_fn, get_clock_fn)
    }

    fn reduce_order(
        &mut self,
        trader_id: &MarketTraderId,
        order_id: &FIFOOrderId,
        side: Side,
        size: Option<BaseLots>,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        self.reduce_order_inner(
            self.get_trader_index(trader_id)?,
            order_id,
            side,
            size,
            false,
            claim_funds,
            record_event_fn,
        )
    }

    fn cancel_all_orders(
        &mut self,
        trader_id: &MarketTraderId,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        self.cancel_all_orders_inner(trader_id, claim_funds, record_event_fn)
    }

    #[allow(clippy::too_many_arguments)]
    fn cancel_up_to(
        &mut self,
        trader_id: &MarketTraderId,
        side: Side,
        num_orders_to_search: Option<usize>,
        num_orders_to_cancel: Option<usize>,
        tick_limit: Option<Ticks>,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        self.cancel_up_to_inner(
            trader_id,
            side,
            num_orders_to_search,
            num_orders_to_cancel,
            tick_limit,
            claim_funds,
            record_event_fn,
        )
    }

    fn cancel_multiple_orders_by_id(
        &mut self,
        trader_id: &MarketTraderId,
        orders_to_cancel: &[FIFOOrderId],
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        self.cancel_multiple_orders_by_id_inner(
            self.get_trader_index(trader_id)?,
            orders_to_cancel,
            claim_funds,
            record_event_fn,
        )
    }

    fn claim_funds(
        &mut self,
        trader_id: &MarketTraderId,
        num_quote_lots: Option<QuoteLots>,
        num_base_lots: Option<BaseLots>,
        allow_seat_eviction: bool,
    ) -> Option<MatchingEngineResponse> {
        self.claim_funds_inner(
            self.get_trader_index(trader_id)?,
            num_quote_lots,
            num_base_lots,
            allow_seat_eviction,
        )
    }

    fn collect_fees(
        &mut self,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> QuoteLots {
        let quote_lot_fees = self.unclaimed_quote_lot_fees;
        self.collected_quote_lot_fees += self.unclaimed_quote_lot_fees;
        self.unclaimed_quote_lot_fees = QuoteLots::ZERO;
        let fees_collected_in_quote_lots = quote_lot_fees;
        record_event_fn(MarketEvent::Fee {
            fees_collected_in_quote_lots,
        });
        fees_collected_in_quote_lots
    }
}

impl<
        MarketTraderId: Debug
            + PartialOrd
            + Ord
            + Default
            + Copy
            + Clone
            + Zeroable
            + Pod
            + BorshDeserialize
            + BorshSerialize,
        const BIDS_SIZE: usize,
        const ASKS_SIZE: usize,
        const NUM_SEATS: usize,
    > FIFOMarket<MarketTraderId, BIDS_SIZE, ASKS_SIZE, NUM_SEATS>
{
    pub fn new(
        tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
        base_lots_per_base_unit: BaseLotsPerBaseUnit,
    ) -> Self {
        let mut market = Self::default();
        market.set_initial_params(
            tick_size_in_quote_lots_per_base_unit,
            base_lots_per_base_unit,
        );
        market
    }

    fn initialize(&mut self) {
        self.bids.initialize();
        self.asks.initialize();
        self.traders.initialize();
    }

    fn initialize_with_params_inner(
        &mut self,
        tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
        base_lots_per_base_unit: BaseLotsPerBaseUnit,
    ) {
        self.initialize();
        self.set_initial_params(
            tick_size_in_quote_lots_per_base_unit,
            base_lots_per_base_unit,
        );
    }

    fn set_initial_params(
        &mut self,
        tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
        base_lots_per_base_unit: BaseLotsPerBaseUnit,
    ) {
        assert!(tick_size_in_quote_lots_per_base_unit % base_lots_per_base_unit == 0);

        // Ensure there is no re-entrancy
        assert_eq!(self.order_sequence_number, 0);
        self.tick_size_in_quote_lots_per_base_unit = tick_size_in_quote_lots_per_base_unit;
        self.base_lots_per_base_unit = base_lots_per_base_unit;
        // After setting the initial params, this function can never be called again
        self.order_sequence_number += 1;
    }

    #[inline]
    /// Round up the fee to the nearest adjusted quote lot
    fn compute_fee(&self, size_in_adjusted_quote_lots: AdjustedQuoteLots) -> AdjustedQuoteLots {
        AdjustedQuoteLots::new(
            ((size_in_adjusted_quote_lots.as_u128() * self.taker_fee_bps as u128 + 10000 - 1)
                / 10000) as u64,
        )
    }

    #[inline]
    /// Quote lot budget with fees adjusted (buys)
    ///
    /// The desired result is adjusted_quote_lots / (1 + fee_bps). We approach this result by taking
    /// (size_in_lots * u64::MAX) / (u64::MAX * (1 + fee_bps)) for accurate numerical precision.
    /// This will never overflow at any point in the calculation because all intermediate values
    /// will be stored in a u128. There is only a single multiplication of u64's which will be
    /// strictly less than u128::MAX
    fn adjusted_quote_lot_budget_post_fee_adjustment_for_buys(
        &self,
        size_in_adjusted_quote_lots: AdjustedQuoteLots,
    ) -> Option<AdjustedQuoteLots> {
        let fee_adjustment = self.compute_fee(AdjustedQuoteLots::MAX).as_u128() + u64::MAX as u128;
        // Return an option to catch truncation from downcasting to u64
        u64::try_from(size_in_adjusted_quote_lots.as_u128() * u64::MAX as u128 / fee_adjustment)
            .ok()
            .map(AdjustedQuoteLots::new)
    }

    #[inline]
    /// Quote lot budget with fees adjusted (sells)
    ///
    /// The desired result is adjusted_quote_lots / (1 - fee_bps). We approach this result by taking
    /// (size_in_lots * u64::MAX) / (u64::MAX * (1 - fee_bps)) for accurate numerical precision.
    /// This will never overflow at any point in the calculation because all intermediate values
    /// will be stored in a u128. There is only a single multiplication of u64's which will be
    /// strictly less than u128::MAX
    fn adjusted_quote_lot_budget_post_fee_adjustment_for_sells(
        &self,
        size_in_adjusted_quote_lots: AdjustedQuoteLots,
    ) -> Option<AdjustedQuoteLots> {
        let fee_adjustment = self.compute_fee(AdjustedQuoteLots::MAX).as_u128() - u64::MAX as u128;
        // Return an option to catch truncation from downcasting to u64
        u64::try_from(size_in_adjusted_quote_lots.as_u128() * u64::MAX as u128 / fee_adjustment)
            .ok()
            .map(AdjustedQuoteLots::new)
    }

    #[inline]
    /// Adjusted quote lots, rounded up to the nearest multiple of base_lots_per_base_unit
    pub fn round_adjusted_quote_lots_up(
        &self,
        num_adjusted_quote_lots: AdjustedQuoteLots,
    ) -> AdjustedQuoteLots {
        ((num_adjusted_quote_lots
            + AdjustedQuoteLots::new(self.base_lots_per_base_unit.as_u64() - 1))
        .unchecked_div::<BaseLotsPerBaseUnit, QuoteLots>(self.base_lots_per_base_unit))
            * self.base_lots_per_base_unit
    }

    #[inline]
    /// Adjusted quote lots, rounded down to the nearest multiple of base_lots_per_base_unit
    pub fn round_adjusted_quote_lots_down(
        &self,
        num_adjusted_quote_lots: AdjustedQuoteLots,
    ) -> AdjustedQuoteLots {
        num_adjusted_quote_lots
            .unchecked_div::<BaseLotsPerBaseUnit, QuoteLots>(self.base_lots_per_base_unit)
            * self.base_lots_per_base_unit
    }

    /// This function determines whether a PostOnly order crosses the book.
    /// If the order crosses the book, the function returns the price of the best unexpired order
    /// on the opposite side of the book in Ticks. Otherwise, it returns None.
    fn check_for_cross(
        &mut self,
        side: Side,
        num_ticks: Ticks,
        current_slot: u64,
        current_unix_timestamp_in_seconds: u64,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<Ticks> {
        loop {
            let book_entry = self.get_book_mut(side.opposite()).get_min();
            if let Some((o_id, order)) = book_entry {
                let crosses = match side.opposite() {
                    Side::Bid => o_id.price_in_ticks >= num_ticks,
                    Side::Ask => o_id.price_in_ticks <= num_ticks,
                };
                if !crosses {
                    break;
                } else if order.num_base_lots > BaseLots::ZERO {
                    if order.is_expired(current_slot, current_unix_timestamp_in_seconds) {
                        self.reduce_order_inner(
                            order.trader_index as u32,
                            &o_id,
                            side.opposite(),
                            None,
                            true,
                            false,
                            record_event_fn,
                        )?;
                    } else {
                        return Some(o_id.price_in_ticks);
                    }
                } else {
                    // If the order is empty, we can remove it from the tree
                    // This case should never occur in v1
                    phoenix_log!("WARNING: Empty order found in check_for_cross");
                    self.get_book_mut(side.opposite()).remove(&o_id);
                }
            } else {
                // Book is empty
                break;
            }
        }
        None
    }

    #[inline(always)]
    fn claim_funds_inner(
        &mut self,
        trader_index: u32,
        num_quote_lots: Option<QuoteLots>,
        num_base_lots: Option<BaseLots>,
        allow_seat_eviction: bool,
    ) -> Option<MatchingEngineResponse> {
        if self.get_sequence_number() == 0 {
            return None;
        }
        let (is_empty, quote_lots_received, base_lots_received) = {
            let trader_state = self.get_trader_state_from_index_mut(trader_index);
            let quote_lots_free = num_quote_lots
                .unwrap_or(trader_state.quote_lots_free)
                .min(trader_state.quote_lots_free);
            let base_lots_free = num_base_lots
                .unwrap_or(trader_state.base_lots_free)
                .min(trader_state.base_lots_free);
            trader_state.quote_lots_free -= quote_lots_free;
            trader_state.base_lots_free -= base_lots_free;
            (
                *trader_state == TraderState::default(),
                quote_lots_free,
                base_lots_free,
            )
        };
        if is_empty && allow_seat_eviction {
            let trader_id = self.get_trader_id_from_index(trader_index);
            self.traders.remove(&trader_id);
        }
        Some(MatchingEngineResponse::new_withdraw(
            base_lots_received,
            quote_lots_received,
        ))
    }

    fn place_order_inner(
        &mut self,
        trader_id: &MarketTraderId,
        mut order_packet: OrderPacket,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
        get_clock_fn: &mut dyn FnMut() -> (u64, u64),
    ) -> Option<(Option<FIFOOrderId>, MatchingEngineResponse)> {
        if self.order_sequence_number == 0 {
            phoenix_log!("Market is uninitialized");
            return None;
        }
        if self.order_sequence_number == u64::MAX >> 1 {
            phoenix_log!("Sequence number exceeded maximum");
            return None;
        }

        let side = order_packet.side();
        match side {
            Side::Bid => {
                if order_packet.get_price_in_ticks() == Ticks::ZERO {
                    phoenix_log!("Bid price is too low");
                    return None;
                }
            }
            Side::Ask => {
                if !order_packet.is_take_only() {
                    let tick_price = order_packet.get_price_in_ticks();
                    order_packet.set_price_in_ticks(tick_price.max(Ticks::ONE));
                }
            }
        }
        let trader_index = if order_packet.is_take_only() {
            self.get_trader_index(trader_id).unwrap_or(u32::MAX)
        } else {
            self.get_or_register_trader(trader_id)?
        };

        if order_packet.num_base_lots() == 0 && order_packet.num_quote_lots() == 0 {
            phoenix_log!("Either num_base_lots or num_quote_lots must be nonzero");
            return None;
        }

        // For IOC order types exactly one of num_quote_lots or num_base_lots needs to be specified.
        if let OrderPacket::ImmediateOrCancel {
            num_base_lots,
            num_quote_lots,
            ..
        } = order_packet
        {
            if num_base_lots > BaseLots::ZERO && num_quote_lots > QuoteLots::ZERO
                || num_base_lots == BaseLots::ZERO && num_quote_lots == QuoteLots::ZERO
            {
                phoenix_log!(
                    "Invalid IOC params.
                        Exactly one of num_base_lots or num_quote_lots must be nonzero.
                        num_quote_lots: {},
                        num_base_lots: {}",
                    num_quote_lots,
                    num_base_lots
                );
                return None;
            }
        }

        let (current_slot, current_unix_timestamp) = get_clock_fn();

        if order_packet.is_expired(current_slot, current_unix_timestamp) {
            phoenix_log!("Order parameters include a last_valid_slot or last_valid_unix_timestamp_in_seconds in the past, skipping matching and posting");
            return None;
        }

        let (resting_order, mut matching_engine_response) = if let OrderPacket::PostOnly {
            price_in_ticks,
            reject_post_only,
            ..
        } = &mut order_packet
        {
            // Handle cases where PostOnly order would cross the book
            if let Some(ticks) = self.check_for_cross(
                side,
                *price_in_ticks,
                current_slot,
                current_unix_timestamp,
                record_event_fn,
            ) {
                if *reject_post_only {
                    phoenix_log!("PostOnly order crosses the book - order rejected");
                    return None;
                } else {
                    match side {
                        Side::Bid => {
                            if ticks <= Ticks::ONE {
                                phoenix_log!("PostOnly order crosses the book and can not be amended to a valid price - order rejected");
                                return None;
                            }
                            *price_in_ticks = ticks - Ticks::ONE;
                        }
                        Side::Ask => {
                            *price_in_ticks = ticks + Ticks::ONE;
                        }
                    }
                    phoenix_log!("PostOnly order crosses the book - order amended");
                }
            }

            (
                FIFORestingOrder::new(
                    trader_index as u64,
                    order_packet.num_base_lots(),
                    order_packet.get_last_valid_slot(),
                    order_packet.get_last_valid_unix_timestamp_in_seconds(),
                ),
                MatchingEngineResponse::default(),
            )
        } else {
            let base_lot_budget = order_packet.base_lot_budget();
            // Multiply the quote lot budget by the number of base lots per unit to get the number of
            // adjusted quote lots (quote_lots * base_lots_per_base_unit)
            let quote_lot_budget = order_packet.quote_lot_budget();
            let adjusted_quote_lot_budget = match side {
                // For buys, the adjusted quote lot budget is decreased by the max fee.
                // This is because the fee is added to the quote lots spent after the matching is complete.
                Side::Bid => quote_lot_budget.and_then(|quote_lot_budget| {
                    self.adjusted_quote_lot_budget_post_fee_adjustment_for_buys(
                        quote_lot_budget * self.base_lots_per_base_unit,
                    )
                }),
                // For sells, the adjusted quote lot budget is increased by the max fee.
                // This is because the fee is subtracted from the quote lot received after the matching is complete.
                Side::Ask => quote_lot_budget.and_then(|quote_lot_budget| {
                    self.adjusted_quote_lot_budget_post_fee_adjustment_for_sells(
                        quote_lot_budget * self.base_lots_per_base_unit,
                    )
                }),
            }
            .unwrap_or_else(|| AdjustedQuoteLots::new(u64::MAX));

            let mut inflight_order = InflightOrder::new(
                side,
                order_packet.self_trade_behavior(),
                order_packet.get_price_in_ticks(),
                order_packet.match_limit(),
                base_lot_budget,
                adjusted_quote_lot_budget,
                order_packet.get_last_valid_slot(),
                order_packet.get_last_valid_unix_timestamp_in_seconds(),
            );
            let resting_order = self
                .match_order(
                    &mut inflight_order,
                    trader_index,
                    record_event_fn,
                    current_slot,
                    current_unix_timestamp,
                )
                .map_or_else(
                    || {
                        phoenix_log!("Encountered error matching order");
                        None
                    },
                    Some,
                )?;
            // matched_adjusted_quote_lots is rounded down to the nearest tick for buys and up for
            // sells to yield a whole number of matched_quote_lots.
            let matched_quote_lots = match side {
                // We add the quote_lot_fees to account for the fee being paid on a buy order
                Side::Bid => {
                    (self.round_adjusted_quote_lots_up(inflight_order.matched_adjusted_quote_lots)
                        / self.base_lots_per_base_unit)
                        + inflight_order.quote_lot_fees
                }
                // We subtract the quote_lot_fees to account for the fee being paid on a sell order
                Side::Ask => {
                    (self
                        .round_adjusted_quote_lots_down(inflight_order.matched_adjusted_quote_lots)
                        / self.base_lots_per_base_unit)
                        - inflight_order.quote_lot_fees
                }
            };
            let matching_engine_response = match side {
                Side::Bid => MatchingEngineResponse::new_from_buy(
                    matched_quote_lots,
                    inflight_order.matched_base_lots,
                ),
                Side::Ask => MatchingEngineResponse::new_from_sell(
                    inflight_order.matched_base_lots,
                    matched_quote_lots,
                ),
            };

            record_event_fn(MarketEvent::FillSummary {
                client_order_id: order_packet.client_order_id(),
                total_base_lots_filled: inflight_order.matched_base_lots,
                total_quote_lots_filled: matched_quote_lots,
                total_fee_in_quote_lots: inflight_order.quote_lot_fees,
            });

            (resting_order, matching_engine_response)
        };

        let mut placed_order_id = None;

        if let OrderPacket::ImmediateOrCancel {
            min_base_lots_to_fill,
            min_quote_lots_to_fill,
            ..
        } = order_packet
        {
            // For IOC orders, if the order's minimum fill requirements are not met, then
            // the order is voided
            if matching_engine_response.num_base_lots() < min_base_lots_to_fill
                || matching_engine_response.num_quote_lots() < min_quote_lots_to_fill
            {
                phoenix_log!(
                    "IOC order failed to meet minimum fill requirements. 
                        min_base_lots_to_fill: {},
                        min_quote_lots_to_fill: {},
                        matched_base_lots: {},
                        matched_quote_lots: {}",
                    min_base_lots_to_fill,
                    min_quote_lots_to_fill,
                    matching_engine_response.num_base_lots(),
                    matching_engine_response.num_quote_lots(),
                );
                return None;
            }
        } else {
            let price_in_ticks = order_packet.get_price_in_ticks();
            let (order_id, book_full) = match side {
                Side::Bid => (
                    FIFOOrderId::new(price_in_ticks, !self.order_sequence_number),
                    self.bids.len() == self.bids.capacity(),
                ),
                Side::Ask => (
                    FIFOOrderId::new(price_in_ticks, self.order_sequence_number),
                    self.asks.len() == self.asks.capacity(),
                ),
            };

            // Only place an order if there is more size to place
            if resting_order.num_base_lots > BaseLots::ZERO {
                // Evict order from the book if it is at capacity
                placed_order_id = Some(order_id);
                if book_full {
                    phoenix_log!("Book is full. Evicting order");
                    self.evict_least_aggressive_order(side, record_event_fn, &order_id);
                }
                // Add new order to the book
                self.get_book_mut(side)
                    .insert(order_id, resting_order)
                    .map_or_else(
                        || {
                            phoenix_log!("Failed to insert order into book");
                            None
                        },
                        Some,
                    )?;
                // These constants need to be copied because we mutably borrow below
                let tick_size_in_quote_lots_per_base_unit =
                    self.tick_size_in_quote_lots_per_base_unit;
                let base_lots_per_base_unit = self.base_lots_per_base_unit;
                let trader_state = self.get_trader_state_from_index_mut(trader_index);
                // Update trader state and matching engine response accordingly
                match side {
                    Side::Bid => {
                        let quote_lots_to_lock = (tick_size_in_quote_lots_per_base_unit
                            * order_id.price_in_ticks
                            * resting_order.num_base_lots)
                            / base_lots_per_base_unit;
                        let quote_lots_free_to_use =
                            quote_lots_to_lock.min(trader_state.quote_lots_free);
                        trader_state.use_free_quote_lots(quote_lots_free_to_use);
                        trader_state.lock_quote_lots(quote_lots_to_lock);
                        matching_engine_response.post_quote_lots(quote_lots_to_lock);
                        matching_engine_response.use_free_quote_lots(quote_lots_free_to_use);
                    }
                    Side::Ask => {
                        let base_lots_free_to_use =
                            resting_order.num_base_lots.min(trader_state.base_lots_free);
                        trader_state.use_free_base_lots(base_lots_free_to_use);
                        trader_state.lock_base_lots(resting_order.num_base_lots);
                        matching_engine_response.post_base_lots(resting_order.num_base_lots);
                        matching_engine_response.use_free_base_lots(base_lots_free_to_use);
                    }
                }

                // Record the place event
                record_event_fn(MarketEvent::<MarketTraderId>::Place {
                    order_sequence_number: order_id.order_sequence_number,
                    price_in_ticks: order_id.price_in_ticks,
                    base_lots_placed: resting_order.num_base_lots,
                    client_order_id: order_packet.client_order_id(),
                });

                if resting_order.last_valid_slot != 0
                    || resting_order.last_valid_unix_timestamp_in_seconds != 0
                {
                    // Record the time in force event
                    record_event_fn(MarketEvent::<MarketTraderId>::TimeInForce {
                        order_sequence_number: order_id.order_sequence_number,
                        last_valid_slot: resting_order.last_valid_slot,
                        last_valid_unix_timestamp_in_seconds: resting_order
                            .last_valid_unix_timestamp_in_seconds,
                    });
                }

                // Increment the order sequence number after successfully placing an order
                self.order_sequence_number += 1;
            }
        }

        // If the trader is a registered trader, check if they have free lots
        if trader_index != u32::MAX {
            let trader_state = self.get_trader_state_from_index_mut(trader_index);
            match side {
                Side::Bid => {
                    let quote_lots_free_to_use = trader_state
                        .quote_lots_free
                        .min(matching_engine_response.num_quote_lots());
                    trader_state.use_free_quote_lots(quote_lots_free_to_use);
                    matching_engine_response.use_free_quote_lots(quote_lots_free_to_use);
                }
                Side::Ask => {
                    let base_lots_free_to_use = trader_state
                        .base_lots_free
                        .min(matching_engine_response.num_base_lots());
                    trader_state.use_free_base_lots(base_lots_free_to_use);
                    matching_engine_response.use_free_base_lots(base_lots_free_to_use);
                }
            }

            // If the order crosses and only uses deposited funds, then add the matched funds back to the trader's free funds
            // Set the matching_engine_response lots_out to zero to set token withdrawals to zero
            if order_packet.no_deposit_or_withdrawal() {
                match side {
                    Side::Bid => {
                        trader_state
                            .deposit_free_base_lots(matching_engine_response.num_base_lots_out);
                        matching_engine_response.num_base_lots_out = BaseLots::ZERO;
                    }
                    Side::Ask => {
                        trader_state
                            .deposit_free_quote_lots(matching_engine_response.num_quote_lots_out);
                        matching_engine_response.num_quote_lots_out = QuoteLots::ZERO;
                    }
                }

                // Check if trader has enough deposited funds to process the order
                if !matching_engine_response.verify_no_deposit_or_withdrawal() {
                    phoenix_log!("Insufficient deposited funds to process order");
                    return None;
                }
            }
        }

        Some((placed_order_id, matching_engine_response))
    }

    fn evict_least_aggressive_order(
        &mut self,
        side: Side,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
        placed_order_id: &FIFOOrderId,
    ) -> Option<FIFORestingOrder> {
        let (order_id, resting_order) = {
            // Find the least aggressive order in the book
            let (fifo_order_id, resting_order) = self.get_book_mut(side).get_max()?;
            let maker_id = self.get_trader_id_from_index(resting_order.trader_index as u32);
            if match side {
                Side::Bid => fifo_order_id.price_in_ticks >= placed_order_id.price_in_ticks,
                Side::Ask => fifo_order_id.price_in_ticks <= placed_order_id.price_in_ticks,
            } {
                phoenix_log!("New order is not aggressive enough to evict an existing order");
                return None;
            }
            self.get_book_mut(side).remove(&fifo_order_id)?;
            record_event_fn(MarketEvent::<MarketTraderId>::Evict {
                maker_id,
                order_sequence_number: fifo_order_id.order_sequence_number,
                price_in_ticks: fifo_order_id.price_in_ticks,
                base_lots_evicted: resting_order.num_base_lots,
            });
            (fifo_order_id, resting_order)
        };
        // These constants need to be copied because we mutably borrow below
        let tick_size_in_quote_lots_per_base_unit = self.tick_size_in_quote_lots_per_base_unit;
        let base_lots_per_base_unit = self.base_lots_per_base_unit;
        let trader_state = self.get_trader_state_from_index_mut(resting_order.trader_index as u32);
        match side {
            Side::Bid => {
                let quote_lots_to_unlock = (order_id.price_in_ticks
                    * tick_size_in_quote_lots_per_base_unit
                    * resting_order.num_base_lots)
                    / base_lots_per_base_unit;
                trader_state.unlock_quote_lots(quote_lots_to_unlock);
            }
            Side::Ask => trader_state.unlock_base_lots(resting_order.num_base_lots),
        }
        Some(resting_order)
    }

    fn match_order(
        &mut self,
        inflight_order: &mut InflightOrder,
        current_trader_index: u32,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
        current_slot: u64,
        current_unix_timestamp: u64,
    ) -> Option<FIFORestingOrder> {
        let mut total_matched_adjusted_quote_lots = AdjustedQuoteLots::ZERO;
        while inflight_order.in_progress() {
            // Find the first order on the opposite side of the book that matches the inflight order.
            let (
                trader_index,
                order_id,
                num_base_lots_quoted,
                last_valid_slot,
                last_valid_unix_timestamp_in_seconds,
            ) = {
                let book = self.get_book_mut(inflight_order.side.opposite());
                // Look at the top of the book to compare the book's price to the order's price
                let (
                    crossed,
                    order_id,
                    FIFORestingOrder {
                        trader_index,
                        num_base_lots: num_base_lots_quoted,
                        last_valid_slot,
                        last_valid_unix_timestamp_in_seconds,
                    },
                ) = if let Some((o_id, quote)) = book.get_min() {
                    (
                        match inflight_order.side {
                            Side::Bid => o_id.price_in_ticks <= inflight_order.limit_price_in_ticks,
                            Side::Ask => o_id.price_in_ticks >= inflight_order.limit_price_in_ticks,
                        },
                        o_id,
                        quote,
                    )
                } else {
                    phoenix_log!("Book is empty");
                    break;
                };
                // If the order no longer crosses the limit price (based on limit_price_in_ticks), stop matching
                if !crossed {
                    break;
                }
                if num_base_lots_quoted == BaseLots::ZERO {
                    // This block is entered if we encounter tombstoned orders during the matching process
                    // (Should never trigger in v1)
                    book.remove(&order_id)?;
                    // The tombstone should count as part of the match limit
                    inflight_order.match_limit -= 1;
                    continue;
                }
                (
                    trader_index,
                    order_id,
                    num_base_lots_quoted,
                    last_valid_slot,
                    last_valid_unix_timestamp_in_seconds,
                )
            };

            // This block is entered if the order has expired. The order is removed from the book and
            // the match limit is decremented.
            if (last_valid_slot != 0 && last_valid_slot < current_slot)
                || (last_valid_unix_timestamp_in_seconds != 0
                    && last_valid_unix_timestamp_in_seconds < current_unix_timestamp)
            {
                self.reduce_order_inner(
                    trader_index as u32,
                    &order_id,
                    inflight_order.side.opposite(),
                    None,
                    true,
                    false,
                    record_event_fn,
                )?;
                inflight_order.match_limit -= 1;
                continue;
            }

            // Handle self trade
            if trader_index == current_trader_index as u64 {
                match inflight_order.self_trade_behavior {
                    SelfTradeBehavior::Abort => return None,
                    SelfTradeBehavior::CancelProvide => {
                        // This block is entered if the self trade behavior for the crossing order is
                        // CancelProvide
                        //
                        // We cancel the order from the book and free up the locked quote_lots or base_lots, but
                        // we do not claim them as part of the match
                        self.reduce_order_inner(
                            current_trader_index,
                            &order_id,
                            inflight_order.side.opposite(),
                            None,
                            false,
                            false,
                            record_event_fn,
                        )?;
                        inflight_order.match_limit -= 1;
                    }
                    SelfTradeBehavior::DecrementTake => {
                        let base_lots_removed = inflight_order
                            .base_lot_budget
                            .min(
                                inflight_order
                                    .adjusted_quote_lot_budget
                                    .unchecked_div::<QuoteLotsPerBaseUnit, BaseLots>(
                                        order_id.price_in_ticks
                                            * self.tick_size_in_quote_lots_per_base_unit,
                                    ),
                            )
                            .min(num_base_lots_quoted);

                        self.reduce_order_inner(
                            current_trader_index,
                            &order_id,
                            inflight_order.side.opposite(),
                            Some(base_lots_removed),
                            false,
                            false,
                            record_event_fn,
                        )?;
                        // In the case that the self trade behavior is DecrementTake, we decrement the
                        // the base lot and adjusted quote lot budgets accordingly
                        inflight_order.base_lot_budget = inflight_order
                            .base_lot_budget
                            .saturating_sub(base_lots_removed);
                        inflight_order.adjusted_quote_lot_budget =
                            inflight_order.adjusted_quote_lot_budget.saturating_sub(
                                self.tick_size_in_quote_lots_per_base_unit
                                    * order_id.price_in_ticks
                                    * base_lots_removed,
                            );
                        // Self trades will count towards the match limit
                        inflight_order.match_limit -= 1;
                        // If base_lots_removed < num_base_lots_quoted, then the order budget must be fully
                        // exhausted
                        inflight_order.should_terminate = base_lots_removed < num_base_lots_quoted;
                    }
                }
                continue;
            }

            let num_adjusted_quote_lots_quoted = order_id.price_in_ticks
                * self.tick_size_in_quote_lots_per_base_unit
                * num_base_lots_quoted;

            let (matched_base_lots, matched_adjusted_quote_lots, order_remaining_base_lots) = {
                // This constant needs to be copied because we mutably borrow below
                let tick_size_in_quote_lots_per_base_unit =
                    self.tick_size_in_quote_lots_per_base_unit;

                let book = self.get_book_mut(inflight_order.side.opposite());

                // Check if the inflight order's budget is exhausted
                let has_remaining_adjusted_quote_lots =
                    num_adjusted_quote_lots_quoted <= inflight_order.adjusted_quote_lot_budget;
                let has_remaining_base_lots =
                    num_base_lots_quoted <= inflight_order.base_lot_budget;

                if has_remaining_base_lots && has_remaining_adjusted_quote_lots {
                    // If there is remaining budget, we match the entire book order
                    book.remove(&order_id)?;
                    (
                        num_base_lots_quoted,
                        num_adjusted_quote_lots_quoted,
                        BaseLots::ZERO,
                    )
                } else {
                    // If the order's budget is exhausted, we match as much as we can
                    let base_lots_to_remove = inflight_order.base_lot_budget.min(
                        inflight_order
                            .adjusted_quote_lot_budget
                            .unchecked_div::<QuoteLotsPerBaseUnit, BaseLots>(
                                order_id.price_in_ticks * tick_size_in_quote_lots_per_base_unit,
                            ),
                    );
                    let adjusted_quote_lots_to_remove = order_id.price_in_ticks
                        * tick_size_in_quote_lots_per_base_unit
                        * base_lots_to_remove;
                    let matched_order = book.get_mut(&order_id)?;
                    matched_order.num_base_lots -= base_lots_to_remove;
                    // If this clause is reached, we make ensure that the loop terminates
                    // as the order has been fully filled
                    inflight_order.should_terminate = true;
                    (
                        base_lots_to_remove,
                        adjusted_quote_lots_to_remove,
                        matched_order.num_base_lots,
                    )
                }
            };

            // Deplete the inflight order's budget by the amount matched
            inflight_order.process_match(matched_adjusted_quote_lots, matched_base_lots);

            // Increment the matched adjusted quote lots for fee calculation
            total_matched_adjusted_quote_lots += matched_adjusted_quote_lots;

            // If the matched base lots is zero, we don't record the fill event
            if matched_base_lots != BaseLots::ZERO {
                // The fill event is recorded
                record_event_fn(MarketEvent::<MarketTraderId>::Fill {
                    maker_id: self.get_trader_id_from_index(trader_index as u32),
                    order_sequence_number: order_id.order_sequence_number,
                    price_in_ticks: order_id.price_in_ticks,
                    base_lots_filled: matched_base_lots,
                    base_lots_remaining: order_remaining_base_lots,
                });
            } else if !inflight_order.should_terminate {
                phoenix_log!(
                    "WARNING: should_terminate should always be true if matched_base_lots is zero"
                );
            }

            let base_lots_per_base_unit = self.base_lots_per_base_unit;
            // Update the maker's state to reflect the match
            let trader_state = self.get_trader_state_from_index_mut(trader_index as u32);
            match inflight_order.side {
                Side::Bid => trader_state.process_limit_sell(
                    matched_base_lots,
                    matched_adjusted_quote_lots / base_lots_per_base_unit,
                ),
                Side::Ask => trader_state.process_limit_buy(
                    matched_adjusted_quote_lots / base_lots_per_base_unit,
                    matched_base_lots,
                ),
            }
        }
        // Fees are updated based on the total amount matched
        inflight_order.quote_lot_fees = self
            .round_adjusted_quote_lots_up(self.compute_fee(total_matched_adjusted_quote_lots))
            / self.base_lots_per_base_unit;
        self.unclaimed_quote_lot_fees += inflight_order.quote_lot_fees;

        Some(FIFORestingOrder::new(
            current_trader_index as u64,
            inflight_order.base_lot_budget,
            inflight_order.last_valid_slot,
            inflight_order.last_valid_unix_timestamp_in_seconds,
        ))
    }

    fn cancel_all_orders_inner(
        &mut self,
        trader_id: &MarketTraderId,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        let trader_index = self.get_trader_index(trader_id)?;
        let orders_to_cancel = [Side::Bid, Side::Ask]
            .iter()
            .flat_map(|side| {
                self.get_book(*side)
                    .iter()
                    .filter(|(_o_id, o)| {
                        o.trader_index == trader_index as u64 && o.num_base_lots > BaseLots::ZERO
                    })
                    .map(|(o_id, _)| *o_id)
            })
            .collect::<Vec<_>>();
        self.cancel_multiple_orders_by_id_inner(
            trader_index,
            &orders_to_cancel,
            claim_funds,
            record_event_fn,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn cancel_up_to_inner(
        &mut self,
        trader_id: &MarketTraderId,
        side: Side,
        num_orders_to_search: Option<usize>,
        num_orders_to_cancel: Option<usize>,
        tick_limit: Option<Ticks>,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        let trader_index = self.get_trader_index(trader_id)?;

        let last_tick = tick_limit.unwrap_or(match side {
            Side::Ask => Ticks::MAX,
            Side::Bid => Ticks::MIN,
        });
        let book = self.get_book(side);
        let num_orders = book.len();

        let orders_to_cancel = book
            .iter()
            .take(num_orders_to_search.unwrap_or(num_orders))
            .filter(|(_o_id, o)| o.trader_index == trader_index as u64)
            .filter(|(o_id, _)| match side {
                Side::Bid => o_id.price_in_ticks >= last_tick,
                Side::Ask => o_id.price_in_ticks <= last_tick,
            })
            .take(num_orders_to_cancel.unwrap_or(num_orders))
            .map(|(o_id, _)| *o_id)
            .collect::<Vec<_>>();

        self.cancel_multiple_orders_by_id_inner(
            trader_index,
            &orders_to_cancel,
            claim_funds,
            record_event_fn,
        )
    }

    fn cancel_multiple_orders_by_id_inner(
        &mut self,
        trader_index: u32,
        orders_to_cancel: &[FIFOOrderId],
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        let (quote_lots_released, base_lots_released) = orders_to_cancel
            .iter()
            .filter_map(|&order_id| {
                self.reduce_order_inner(
                    trader_index,
                    &order_id,
                    Side::from_order_sequence_number(order_id.order_sequence_number),
                    None,
                    false,
                    claim_funds,
                    record_event_fn,
                )
                .map(
                    |MatchingEngineResponse {
                         num_quote_lots_out,
                         num_base_lots_out,
                         ..
                     }| (num_quote_lots_out, num_base_lots_out),
                )
            })
            .fold(
                (QuoteLots::ZERO, BaseLots::ZERO),
                |(quote_lots_released, base_lots_released), (quote_lots_out, base_lots_out)| {
                    (
                        quote_lots_released + quote_lots_out,
                        base_lots_released + base_lots_out,
                    )
                },
            );

        Some(MatchingEngineResponse::new_withdraw(
            base_lots_released,
            quote_lots_released,
        ))
    }

    #[inline(always)]
    fn reduce_order_inner(
        &mut self,
        trader_index: u32,
        order_id: &FIFOOrderId,
        side: Side,
        size: Option<BaseLots>,
        order_is_expired: bool,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        let maker_id = self.get_trader_id_from_index(trader_index);
        let removed_base_lots = {
            let book = self.get_book_mut(side);
            let (should_remove_order_from_book, base_lots_to_remove) = {
                if let Some(order) = book.get(order_id) {
                    let base_lots_to_remove = size
                        .map(|s| s.min(order.num_base_lots))
                        .unwrap_or(order.num_base_lots);
                    if order.trader_index != trader_index as u64 {
                        return None;
                    }
                    (
                        base_lots_to_remove == order.num_base_lots,
                        base_lots_to_remove,
                    )
                } else {
                    return Some(MatchingEngineResponse::default());
                }
            };
            let base_lots_remaining = if should_remove_order_from_book {
                // This will never return None because we already checked that the order exists
                book.remove(order_id)?;
                BaseLots::ZERO
            } else {
                // This will never return None because we already checked that the order exists
                let resting_order = book.get_mut(order_id)?;
                resting_order.num_base_lots -= base_lots_to_remove;
                resting_order.num_base_lots
            };
            // If the order was not cancelled by the maker, we make sure that the maker's id is logged.
            if order_is_expired {
                record_event_fn(MarketEvent::ExpiredOrder {
                    maker_id,
                    order_sequence_number: order_id.order_sequence_number,
                    price_in_ticks: order_id.price_in_ticks,
                    base_lots_removed: base_lots_to_remove,
                });
            } else {
                record_event_fn(MarketEvent::Reduce {
                    order_sequence_number: order_id.order_sequence_number,
                    price_in_ticks: order_id.price_in_ticks,
                    base_lots_removed: base_lots_to_remove,
                    base_lots_remaining,
                });
            }
            base_lots_to_remove
        };
        let (num_quote_lots, num_base_lots) = {
            // These constants need to be copied because we mutably borrow below
            let tick_size_in_quote_lots_per_base_unit = self.tick_size_in_quote_lots_per_base_unit;
            let base_lots_per_base_unit = self.base_lots_per_base_unit;
            let trader_state = self.get_trader_state_from_index_mut(trader_index);
            match side {
                Side::Bid => {
                    let quote_lots = (order_id.price_in_ticks
                        * tick_size_in_quote_lots_per_base_unit
                        * removed_base_lots)
                        / base_lots_per_base_unit;
                    trader_state.unlock_quote_lots(quote_lots);
                    (quote_lots, BaseLots::ZERO)
                }
                Side::Ask => {
                    trader_state.unlock_base_lots(removed_base_lots);
                    (QuoteLots::ZERO, removed_base_lots)
                }
            }
        };
        // We don't want to claim funds if an order is removed from the book during a self trade
        // or if the user specifically indicates that they don't want to claim funds.
        if claim_funds {
            self.claim_funds_inner(
                trader_index,
                Some(num_quote_lots),
                Some(num_base_lots),
                false,
            )
        } else {
            Some(MatchingEngineResponse::default())
        }
    }
}
