use itertools::Itertools;

use crate::{
    quantities::{
        BaseLots, BaseLotsPerBaseUnit, QuoteLots, QuoteLotsPerBaseUnitPerTick, Ticks, WrapperU64,
    },
    state::{matching_engine_response::MatchingEngineResponse, *},
};
use borsh::{BorshDeserialize, BorshSerialize};
use sokoban::node_allocator::OrderedNodeAllocatorMap;

use super::MarketEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LadderOrder {
    pub price_in_ticks: u64,
    pub size_in_base_lots: u64,
}

/// Helpful struct for processing the order book state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ladder {
    pub bids: Vec<LadderOrder>,
    pub asks: Vec<LadderOrder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypedLadderOrder {
    pub price_in_ticks: Ticks,
    pub size_in_base_lots: BaseLots,
}

/// Helpful struct for processing the order book state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedLadder {
    pub bids: Vec<TypedLadderOrder>,
    pub asks: Vec<TypedLadderOrder>,
}

pub trait OrderId {
    fn price_in_ticks(&self) -> u64;
}

pub trait RestingOrder {
    fn size(&self) -> u64;
}

/// A wrapper around an matching algorithm implementation that allows the specific struct to be
/// used as a generic market.
pub trait Market<
    MarketTraderId: BorshDeserialize + BorshSerialize + Copy,
    MarketOrderId: OrderId,
    MarketRestingOrder: RestingOrder,
    MarketOrderPacket: OrderPacketMetadata,
>
{
    fn initialize_with_params(
        &mut self,
        tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
        base_lots_per_base_unit: BaseLotsPerBaseUnit,
    );

    fn set_fee(&mut self, taker_fee_bps: u64);

    fn get_data_size(&self) -> usize {
        unimplemented!()
    }
    fn get_uncollected_fee_amount(&self) -> QuoteLots {
        unimplemented!()
    }

    fn get_ladder(&self, levels: u64) -> Ladder {
        let ladder = self.get_typed_ladder(levels);
        Ladder {
            bids: ladder
                .bids
                .iter()
                .map(|order| LadderOrder {
                    price_in_ticks: order.price_in_ticks.as_u64(),
                    size_in_base_lots: order.size_in_base_lots.as_u64(),
                })
                .collect(),
            asks: ladder
                .asks
                .iter()
                .map(|order| LadderOrder {
                    price_in_ticks: order.price_in_ticks.as_u64(),
                    size_in_base_lots: order.size_in_base_lots.as_u64(),
                })
                .collect(),
        }
    }

    fn get_typed_ladder(&self, levels: u64) -> TypedLadder {
        let mut bids = vec![];
        let mut asks = vec![];
        for (side, book) in [(Side::Bid, &mut bids), (Side::Ask, &mut asks)].iter_mut() {
            book.extend_from_slice(
                &self
                    .get_book(*side)
                    .iter()
                    .map(|(order_id, resting_order)| {
                        (order_id.price_in_ticks(), resting_order.size())
                    })
                    .group_by(|(price_in_ticks, _)| *price_in_ticks)
                    .into_iter()
                    .take(levels as usize)
                    .map(|(price_in_ticks, group)| TypedLadderOrder {
                        price_in_ticks: Ticks::new(price_in_ticks),
                        size_in_base_lots: BaseLots::new(group.map(|(_, size)| size).sum()),
                    })
                    .collect::<Vec<TypedLadderOrder>>(),
            );
        }
        TypedLadder { bids, asks }
    }

    fn get_base_lots_per_base_unit(&self) -> BaseLotsPerBaseUnit;
    fn get_sequence_number(&self) -> u64;

    fn get_registered_traders(&self) -> &dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>;
    fn get_registered_traders_mut(
        &mut self,
    ) -> &mut dyn OrderedNodeAllocatorMap<MarketTraderId, TraderState>;

    fn get_trader_state_mut(&mut self, key: &MarketTraderId) -> Option<&mut TraderState>;
    fn get_trader_state(&self, key: &MarketTraderId) -> Option<&TraderState>;

    fn get_trader_state_from_index_mut(&mut self, index: u32) -> &mut TraderState;
    fn get_trader_state_from_index(&self, index: u32) -> &TraderState;

    fn get_trader_index(&self, trader: &MarketTraderId) -> Option<u32>;
    fn get_trader_id_from_index(&self, trader_index: u32) -> MarketTraderId;

    fn get_or_register_trader(&mut self, trader: &MarketTraderId) -> Option<u32> {
        let registered_traders = self.get_registered_traders_mut();
        if !registered_traders.contains(trader) {
            registered_traders.insert(*trader, TraderState::default())?;
        }
        self.get_trader_index(trader)
    }

    fn try_remove_trader_state(&mut self, trader: &MarketTraderId) -> Option<()> {
        let registered_traders = self.get_registered_traders_mut();
        let trader_state = registered_traders.get(trader)?;
        if *trader_state == TraderState::default() {
            registered_traders.remove(trader)?;
        }
        Some(())
    }

    fn get_book(
        &self,
        side: Side,
    ) -> &dyn OrderedNodeAllocatorMap<MarketOrderId, MarketRestingOrder>;

    fn get_book_mut(
        &mut self,
        side: Side,
    ) -> &mut dyn OrderedNodeAllocatorMap<MarketOrderId, MarketRestingOrder>;

    fn place_order(
        &mut self,
        trader: &MarketTraderId,
        order_packet: MarketOrderPacket,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<(Option<MarketOrderId>, MatchingEngineResponse)>;

    fn cancel_order(
        &mut self,
        trader_id: &MarketTraderId,
        order_id: &MarketOrderId,
        side: Side,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse> {
        self.reduce_order(
            trader_id,
            order_id,
            side,
            None,
            claim_funds,
            record_event_fn,
        )
    }

    fn reduce_order(
        &mut self,
        trader_id: &MarketTraderId,
        order_id: &MarketOrderId,
        side: Side,
        size: Option<BaseLots>,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse>;

    fn cancel_all_orders(
        &mut self,
        trader_id: &MarketTraderId,
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse>;

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
    ) -> Option<MatchingEngineResponse>;

    fn cancel_multiple_orders_by_id(
        &mut self,
        trader_id: &MarketTraderId,
        orders_to_cancel: &[MarketOrderId],
        claim_funds: bool,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> Option<MatchingEngineResponse>;

    fn claim_all_funds(
        &mut self,
        trader: &MarketTraderId,
        allow_seat_eviction: bool,
    ) -> Option<MatchingEngineResponse> {
        self.claim_funds(trader, None, None, allow_seat_eviction)
    }

    fn claim_funds(
        &mut self,
        trader: &MarketTraderId,
        num_quote_lots: Option<QuoteLots>,
        num_base_lots: Option<BaseLots>,
        allow_seat_eviction: bool,
    ) -> Option<MatchingEngineResponse>;

    fn collect_fees(
        &mut self,
        record_event_fn: &mut dyn FnMut(MarketEvent<MarketTraderId>),
    ) -> QuoteLots;
}
