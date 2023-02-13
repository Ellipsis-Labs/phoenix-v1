use std::collections::VecDeque;

use phoenix::quantities::*;
use phoenix::state::markets::*;
use phoenix::state::*;
use rand::prelude::*;
use sokoban::node_allocator::NodeAllocatorMap;
use sokoban::ZeroCopy;

const BOOK_SIZE: usize = 2048;

type TraderId = u128;
type Dex = FIFOMarket<TraderId, BOOK_SIZE, BOOK_SIZE, 1024>;

fn setup_market() -> Dex {
    Dex::new(
        QuoteLotsPerBaseUnitPerTick::new(10000),
        BaseLotsPerBaseUnit::new(100),
    )
}

fn setup_market_with_params(
    tick_size_in_quote_lots_per_base_unit: u64,
    base_lots_per_base_unit: u64,
    fees: u64,
) -> Dex {
    let mut data = vec![0; std::mem::size_of::<Dex>()];
    let dex = Dex::load_mut_bytes(&mut data).unwrap();
    dex.initialize_with_params(
        QuoteLotsPerBaseUnitPerTick::new(tick_size_in_quote_lots_per_base_unit),
        BaseLotsPerBaseUnit::new(base_lots_per_base_unit),
    );
    dex.set_fee(fees);
    dex.clone()
}

#[allow(clippy::too_many_arguments)]
fn layer_orders(
    dex: &mut Dex,
    trader: TraderId,
    start_price: u64,
    end_price: u64,
    price_step: u64,
    start_size: u64,
    size_step: u64,
    side: Side,
    event_recorder: &mut dyn FnMut(MarketEvent<TraderId>) -> (),
) {
    assert!(price_step > 0 && size_step > 0);
    let mut prices = vec![];
    let mut sizes = vec![];
    match side {
        Side::Bid => {
            assert!(start_price >= end_price);
            let mut price = start_price;
            let mut size = start_size;
            while price >= end_price && price > 0 {
                prices.push(price);
                sizes.push(size);
                price -= price_step;
                size += size_step;
            }
        }
        Side::Ask => {
            assert!(start_price <= end_price);
            let mut price = start_price;
            let mut size = start_size;
            while price <= end_price {
                prices.push(price);
                sizes.push(size);
                price += price_step;
                size += size_step;
            }
        }
    }
    let adj = dex.get_base_lots_per_base_unit().as_u64();
    for (p, s) in prices.iter().zip(sizes.iter()) {
        dex.place_order(
            &trader,
            OrderPacket::new_limit_order_default(side, *p, *s * adj),
            event_recorder,
        )
        .unwrap();
    }
}

#[test]
fn test_market_simple() {
    use std::collections::{HashSet, LinkedList};

    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut makers = vec![];
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
    for _ in 0..10 {
        let maker = rng.gen::<u128>();
        let bid_start = rng.gen_range(9996, 10_000);
        let bid_end = rng.gen_range(9900, 9978);
        let ask_start = rng.gen_range(10_001, 10_005);
        let ask_end = rng.gen_range(10_033, 10_100);
        let start_size = rng.gen_range(1, 10);
        let size_step = rng.gen_range(1, 5);
        layer_orders(
            &mut market,
            maker,
            bid_start,
            bid_end,
            1,
            start_size,
            size_step,
            Side::Bid,
            &mut record_event_fn,
        );
        layer_orders(
            &mut market,
            maker,
            ask_start,
            ask_end,
            1,
            start_size,
            size_step,
            Side::Ask,
            &mut record_event_fn,
        );
        makers.push(maker)
    }
    for ((o_id_prv, _), (o_id_cur, _)) in market.bids.iter().zip(market.bids.iter().skip(1)) {
        if o_id_prv.price_in_ticks == o_id_cur.price_in_ticks {
            assert!(o_id_prv.order_sequence_number > o_id_cur.order_sequence_number);
        } else {
            assert!(o_id_prv.price_in_ticks > o_id_cur.price_in_ticks);
        }
    }
    for ((o_id_prv, _), (o_id_cur, _)) in market.asks.iter().zip(market.asks.iter().skip(1)) {
        if o_id_prv.price_in_ticks == o_id_cur.price_in_ticks {
            assert!(o_id_prv.order_sequence_number < o_id_cur.order_sequence_number);
        } else {
            assert!(o_id_prv.price_in_ticks < o_id_cur.price_in_ticks);
        }
    }

    let ladder = market.get_typed_ladder(5);
    let taker = rng.gen::<u128>();
    let price = ladder.asks[0].price_in_ticks;
    let size = ladder.asks[0].size_in_base_lots;
    let expected_price = ladder.asks[1].price_in_ticks;
    let expected_size = ladder.asks[1].size_in_base_lots;
    let (
        o,
        MatchingEngineResponse {
            num_quote_lots_in: num_quote_lots,
            num_base_lots_out: num_base_lots,
            ..
        },
    ) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Bid,
                price.as_u64(),
                size.as_u64(),
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o.is_none());
    println!("num_quote_lots: {}", num_quote_lots);
    // println!("price * size: {}", price * size);
    assert!(
        (num_quote_lots * market.base_lots_per_base_unit)
            == market.tick_size_in_quote_lots_per_base_unit * price * size
    );
    assert_eq!(num_base_lots, size);

    let ladder = market.get_typed_ladder(5);
    assert_eq!(ladder.asks[0].price_in_ticks, expected_price);
    assert_eq!(ladder.asks[0].size_in_base_lots, expected_size);

    let price = ladder.bids[0].price_in_ticks;
    let size = ladder.bids[0].size_in_base_lots;
    let expected_price = ladder.bids[1].price_in_ticks;
    let expected_size = ladder.bids[1].size_in_base_lots;
    let (
        _,
        MatchingEngineResponse {
            num_quote_lots_out: num_quote_lots,
            num_base_lots_in: num_base_lots,
            ..
        },
    ) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                price.as_u64(),
                size.as_u64(),
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    println!(
        "num_quote_lots: {} num_base_lots: {} price: {} size: {}",
        num_quote_lots, num_base_lots, price, size
    );
    assert!(
        (num_quote_lots * market.base_lots_per_base_unit)
            == market.tick_size_in_quote_lots_per_base_unit * price * size
    );
    assert_eq!(num_base_lots, size);
    let ladder = market.get_typed_ladder(5);
    assert_eq!(ladder.bids[0].price_in_ticks, expected_price);
    assert_eq!(ladder.bids[0].size_in_base_lots, expected_size);

    market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                ladder.asks[0].price_in_ticks.as_u64(),
                ladder.asks[0].size_in_base_lots.as_u64(),
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();

    let ladder_new = market.get_typed_ladder(5);
    assert_eq!(ladder, ladder_new);

    // Test clearing the bids besides 1 quote lot left on the 5th level
    let price = ladder.bids[4].price_in_ticks;
    let size = ladder
        .bids
        .iter()
        .map(|x| x.size_in_base_lots)
        .sum::<BaseLots>()
        - BaseLots::new(1);
    let total_adjusted_quote_lots = ladder
        .bids
        .iter()
        .map(|x| {
            market.tick_size_in_quote_lots_per_base_unit * x.price_in_ticks * x.size_in_base_lots
        })
        .sum::<AdjustedQuoteLots>();
    println!("Ladder: {:?}", ladder.bids);
    let (
        _,
        MatchingEngineResponse {
            num_quote_lots_out: num_quote_lots,
            num_base_lots_in: num_base_lots,
            ..
        },
    ) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                price.as_u64(),
                size.as_u64(),
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();

    let ladder_new = market.get_typed_ladder(5);
    assert!(ladder_new.bids[0].size_in_base_lots == BaseLots::new(1));
    assert!(ladder_new.bids[0].price_in_ticks == price);
    assert!(num_base_lots == size);
    println!(
        "{} {} {}",
        num_quote_lots,
        total_adjusted_quote_lots,
        (total_adjusted_quote_lots
            - price * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(1))
            / market.base_lots_per_base_unit
    );
    println!("price {}", price);

    assert!(
        num_quote_lots
            == (total_adjusted_quote_lots
                - price * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(1))
                / market.base_lots_per_base_unit
    );
    let mut settlement_list = vec![];
    for (trader, pos) in market.traders.iter() {
        if pos.base_lots_free > BaseLots::ZERO || pos.quote_lots_free > QuoteLots::ZERO {
            settlement_list.push((*trader, *pos));
        }
    }

    for (trader, pos) in settlement_list.iter() {
        market.claim_all_funds(trader, true);
        if pos.base_lots_locked != BaseLots::ZERO || pos.quote_lots_locked != QuoteLots::ZERO {
            let new_pos = market.traders.get(trader).unwrap();
            assert!(
                new_pos.base_lots_free == BaseLots::ZERO
                    && new_pos.quote_lots_free == QuoteLots::ZERO
            );
        } else {
            assert!(market.traders.get(trader).is_none())
        }
    }
    let registed_makers = market
        .get_registered_traders()
        .iter()
        .map(|(k, _)| *k)
        .collect::<HashSet<_>>();

    for m in makers.iter() {
        assert!(registed_makers.contains(m));
        if rng.gen::<f64>() < 0.5 {
            market.cancel_up_to(m, Side::Bid, None, None, None, true, &mut record_event_fn);
        } else {
            let orders = market
                .bids
                .iter()
                .filter(|(_k, v)| v.trader_index == market.traders.get_addr(&m) as u64)
                .map(|(k, _v)| (*k, Side::Bid))
                .collect::<LinkedList<_>>();
            market.cancel_multiple_orders_by_id(m, &orders, true, &mut record_event_fn);
        }
    }

    for m in makers.iter() {
        let ts1 = *market.traders.get(m).unwrap();
        market.cancel_up_to(m, Side::Ask, None, None, None, true, &mut record_event_fn);
        let ts2 = *market.traders.get(m).unwrap();
        market.claim_all_funds(m, true);
        assert!(
            market.traders.get(m).is_none(),
            "{}, {:?} {:?} {:?}",
            m,
            ts1,
            ts2,
            market.traders.get(m)
        );
    }
    assert!(
        market.get_typed_ladder(1).bids.is_empty() && market.get_typed_ladder(1).asks.is_empty()
    );
    println!("{} {}", market.traders.len(), market.traders.capacity());
    assert!(market.traders.is_empty());
    assert!(market.asks.is_empty());
    assert!(market.bids.is_empty());
    println!(
        "{} Memory Size: {}, Order Capacity: {}",
        std::any::type_name::<Dex>(),
        market.get_data_size(),
        BOOK_SIZE * 2
    );
}

#[test]
fn test_post_only_default() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Bid, 100, 1),
            &mut record_event_fn,
        )
        .is_some());
    // Cannot place post only order that would match
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Ask, 100, 1),
            &mut record_event_fn,
        )
        .is_none());

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Ask, 102, 1),
            &mut record_event_fn,
        )
        .is_some());
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Bid, 101, 1),
            &mut record_event_fn,
        )
        .is_some());

    // Cannot place post only order that would match
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Bid, 102, 1),
            &mut record_event_fn,
        )
        .is_none());

    let ladder = market.get_typed_ladder(5);
    assert!(ladder.bids[0].size_in_base_lots == BaseLots::ONE);
    assert!(ladder.bids[0].price_in_ticks == Ticks::new(101));
    assert!(ladder.bids[1].size_in_base_lots == BaseLots::ONE);
    assert!(ladder.bids[1].price_in_ticks == Ticks::new(100));
    assert!(ladder.asks[0].size_in_base_lots == BaseLots::ONE);
    assert!(ladder.asks[0].price_in_ticks == Ticks::new(102));
}

#[test]
fn test_post_only_rejection() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 100, 1, 0, true, false),
            &mut record_event_fn,
        )
        .is_some());
    // Cannot place post only order that would match if reject flag is true
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Ask, 100, 1, 0, true, false),
            &mut record_event_fn,
        )
        .is_none());

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Ask, 102, 1, 0, true, false),
            &mut record_event_fn,
        )
        .is_some());
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 101, 1, 0, true, false),
            &mut record_event_fn,
        )
        .is_some());

    // Cannot place post only order that would match if reject flag is true
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 102, 1, 0, true, false),
            &mut record_event_fn,
        )
        .is_none());

    // Post only is amended if reject flag is false
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Ask, 100, 1, 0, false, false),
            &mut record_event_fn,
        )
        .is_some());

    // Post only is amended if reject flag is false
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 102, 1, 0, false, false),
            &mut record_event_fn,
        )
        .is_some());

    let ladder = market.get_typed_ladder(5);
    assert!(ladder.bids[0].size_in_base_lots == BaseLots::new(2));
    assert!(ladder.bids[0].price_in_ticks == Ticks::new(101));
    assert!(ladder.bids[1].size_in_base_lots == BaseLots::new(1));
    assert!(ladder.bids[1].price_in_ticks == Ticks::new(100));
    assert!(ladder.asks[0].size_in_base_lots == BaseLots::new(2));
    assert!(ladder.asks[0].price_in_ticks == Ticks::new(102));

    market.cancel_all_orders(&trader, true, &mut record_event_fn);

    // Price of the ask is set to the minimum price (1 tick) if the book is empty
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Ask, 1, 1, 0, false, false),
            &mut record_event_fn,
        )
        .is_some());

    // Test rejection of post_only if amended price is below min market price
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 1, 1, 0, false, false),
            &mut record_event_fn,
        )
        .is_none());

    // Test rejection of post_only if original price is below min market price
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 0, 1, 0, false, false),
            &mut record_event_fn,
        )
        .is_none());

    let ladder = market.get_typed_ladder(5);
    assert!(ladder.asks[0].price_in_ticks == Ticks::ONE);
}

#[test]
fn test_cancel_all() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();

    // Place 10 bids starting from 100.
    for i in 0..10 {
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Bid, 100 - i, 1),
                &mut record_event_fn,
            )
            .is_some());
    }

    // Place 10 asks starting from 102.
    for i in 0..10 {
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Ask, 102 + i, 1),
                &mut record_event_fn,
            )
            .is_some());
    }
    market.cancel_all_orders(&trader, true, &mut record_event_fn);

    assert!(market.asks.is_empty());
    assert!(market.bids.is_empty());
}

#[test]
fn test_limit_orders_with_self_trade() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 100, 5),
            &mut record_event_fn,
        )
        .is_some());
    let ladder = market.get_typed_ladder(1);
    // Assert that cancel provide yields the correct order on the book
    assert!(ladder.bids[0].size_in_base_lots == BaseLots::new(5));
    // Can place limit order that would self trade
    let (_order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Ask, 100, 10),
            &mut record_event_fn,
        )
        .unwrap();
    let mut res = MatchingEngineResponse::default();
    res.post_base_lots(BaseLots::new(10));
    assert!(matching_engine_response == res);
    let ladder = market.get_typed_ladder(1);
    // Assert that cancel provide yields the correct order on the book
    assert!(ladder.asks[0].size_in_base_lots == BaseLots::new(10));

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order(
                Side::Bid,
                100,
                15,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .is_none());
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order(
                Side::Bid,
                100,
                15,
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::default();

    res.post_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(5)
            / market.base_lots_per_base_unit,
    );
    res.use_free_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(5)
            / market.base_lots_per_base_unit,
    );
    assert!(matching_engine_response == res);
    let ladder = market.get_typed_ladder(1);
    assert!(ladder.bids[0].size_in_base_lots == BaseLots::new(5));
    let (order, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_limit_order(
                Side::Ask,
                100,
                10,
                SelfTradeBehavior::DecrementTake,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    println!("released quantities: {:?}", matching_engine_response);
    let mut res = MatchingEngineResponse::new_from_sell(
        BaseLots::new(5),
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(5)
            / market.base_lots_per_base_unit,
    );
    res.post_base_lots(BaseLots::new(5));
    println!("Matching engine response: {:?}", matching_engine_response);
    println!("Res: {:?}", res);
    assert!(matching_engine_response == res);
    let ladder = market.get_typed_ladder(1);
    assert!(ladder.asks[0].size_in_base_lots == BaseLots::new(5));
}

#[test]
fn test_limit_orders_with_free_lots() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();

    // Place 2 bids for 100 and 95, then fill them both
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 100, 5),
            &mut record_event_fn,
        )
        .is_some());

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 95, 15),
            &mut record_event_fn,
        )
        .is_some());

    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order_default(Side::Ask, 95, 15,),
            &mut record_event_fn,
        )
        .is_some());

    // Place an offer that utilizes only free lots
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Ask, 100, 5),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::default();
    res.post_base_lots(BaseLots::new(5));
    res.use_free_base_lots(BaseLots::new(5));
    assert!(matching_engine_response == res);

    // Place an offer that utilizes both free and new lots
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Ask, 100, 20),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::default();
    res.post_base_lots(BaseLots::new(20));
    res.use_free_base_lots(BaseLots::new(10));

    assert!(matching_engine_response == res);

    // Place a self trade to free up lots
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 100, 10),
            &mut record_event_fn,
        )
        .is_some());

    // Place an offer that matches with the book and posts using free lots
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order_default(Side::Bid, 101, 10),
            &mut record_event_fn,
        )
        .is_some());
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Ask, 100, 50),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::new_from_sell(
        BaseLots::new(10),
        Ticks::new(101) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(10)
            / market.base_lots_per_base_unit,
    );
    res.post_base_lots(BaseLots::new(40));
    res.use_free_base_lots(BaseLots::new(25));
    assert!(matching_engine_response == res);

    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order_default(Side::Bid, 105, 55,),
            &mut record_event_fn,
        )
        .is_some());

    // Place a bid using some of the freed lots
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 100, 20),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::default();

    res.post_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(20)
            / market.base_lots_per_base_unit,
    );
    res.use_free_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(20)
            / market.base_lots_per_base_unit,
    );
    assert!(matching_engine_response == res);

    // Place a bid using the rest of the freed lots
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 100, 50),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::default();
    res.post_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(50)
            / market.base_lots_per_base_unit,
    );
    res.use_free_quote_lots(
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(30)
            / market.base_lots_per_base_unit,
    );
    assert!(matching_engine_response == res);

    // Place a bid and self trade against it to free up lots
    println!("ladder: {:?}", market.get_typed_ladder(3));
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 120, 50),
            &mut record_event_fn,
        )
        .is_some());
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Ask, 120, 25),
            &mut record_event_fn,
        )
        .is_some());

    // Place a bid that matches with the book and posts using the newly freed lots
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order_default(Side::Ask, 110, 25),
            &mut record_event_fn,
        )
        .is_some());
    let (order, matching_engine_response) = market
        .place_order(
            &trader,
            OrderPacket::new_limit_order_default(Side::Bid, 111, 75),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_some());
    let mut res = MatchingEngineResponse::new_from_buy(
        Ticks::new(110) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(25)
            / market.base_lots_per_base_unit,
        BaseLots::new(25),
    );
    res.post_quote_lots(
        Ticks::new(111) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(50)
            / market.base_lots_per_base_unit,
    );
    res.use_free_quote_lots(
        Ticks::new(120) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(50)
            / market.base_lots_per_base_unit,
    );

    assert!(matching_engine_response == res);
}

#[test]
fn test_orders_with_only_free_funds() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();

    // Note that both the taker and trader will be registered after attempting to place their first limit order
    // Limit order fails as the taker has no free funds
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_post_only(Side::Bid, 100, 5, 0, false, true,),
            &mut record_event_fn,
        )
        .is_none());

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only(Side::Bid, 100, 5, 0, false, false,),
            &mut record_event_fn,
        )
        .is_some());

    // IOC order fails as the taker has no free funds
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                100,
                5,
                SelfTradeBehavior::CancelProvide,
                None,
                0,
                true,
            ),
            &mut record_event_fn,
        )
        .is_none());

    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                100,
                5,
                SelfTradeBehavior::CancelProvide,
                None,
                0,
                false,
            ),
            &mut record_event_fn,
        )
        .is_some());

    // Order succeeds with sufficient deposited funds
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_limit_order(
                Side::Ask,
                100,
                5,
                SelfTradeBehavior::Abort,
                None,
                0,
                true,
            ),
            &mut record_event_fn,
        )
        .is_some());

    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order(
                Side::Bid,
                100,
                5,
                SelfTradeBehavior::Abort,
                None,
                0,
                false,
            ),
            &mut record_event_fn,
        )
        .is_some());

    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_limit_order(
                Side::Ask,
                100,
                5,
                SelfTradeBehavior::Abort,
                None,
                0,
                false,
            ),
            &mut record_event_fn,
        )
        .is_some());

    // Order succeeds as the trader has funds to cover the matched amount, but not the sent amount
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_ioc_by_lots(
                Side::Bid,
                100,
                6,
                SelfTradeBehavior::Abort,
                None,
                0,
                true,
            ),
            &mut record_event_fn,
        )
        .is_some());
}

#[test]
fn test_fok_and_ioc_limit() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = Box::new(setup_market());
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();

    for i in 1..11 {
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Bid, 100 - i, 10),
                &mut record_event_fn,
            )
            .is_some());
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Ask, 100 + i, 10),
                &mut record_event_fn,
            )
            .is_some());
    }

    let prev_market_state = Box::new(*market);
    let starting_ladder = market.get_typed_ladder(5);

    // buy through 3 levels of offers
    let expected_quote_lots_used = Ticks::new(102) // average price 102
            * market.tick_size_in_quote_lots_per_base_unit
            * BaseLots::new(30) // clear 3 levels of 10 base lots each
            / market.base_lots_per_base_unit;
    let (order, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_fok_buy_with_limit_price(
                103,
                30,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_none());
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_buy(expected_quote_lots_used, BaseLots::new(30))
    );

    // sell through 3 levels of bids
    let expected_quote_lots_used = Ticks::new(98) // average price 98
            * market.tick_size_in_quote_lots_per_base_unit
            * BaseLots::new(30) // clear 3 levels of 10 base lots each
            / market.base_lots_per_base_unit;
    let (order, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_fok_sell_with_limit_price(
                97,
                30,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_none());
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_sell(BaseLots::new(30), expected_quote_lots_used,)
    );

    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);

    // (1) FOK should fail if the base lot budget is not enough to fill the order (changed tick limit)
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_fok_buy_with_limit_price(
                103,
                31,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .is_none());
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_fok_sell_with_limit_price(
                97,
                31,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .is_none());

    assert!(
        market
            .place_order(
                &taker,
                OrderPacket::new_ioc(
                    Side::Bid,
                    None,
                    100,
                    1,
                    0,
                    0,
                    SelfTradeBehavior::Abort,
                    None,
                    rng.gen::<u128>(),
                    false,
                ),
                &mut record_event_fn,
            )
            .is_none(),
        "Only one of num_base_lots or num_quote_lots should be set"
    );

    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);

    // (2) FOK should fail if the tick/lot budget is not enough to fill the order (changed price limit)
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_fok_buy_with_limit_price(
                102,
                30,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .is_none());
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_fok_sell_with_limit_price(
                98,
                30,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .is_none());

    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);

    // (3) IOC should succeed if the tick/lot budget is not enough to fill the order (same params as 1)
    let (o, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_buy_with_limit_price(
                103,
                (Ticks::new(102)
                    * market.tick_size_in_quote_lots_per_base_unit
                    * BaseLots::new(30)
                    / market.base_lots_per_base_unit
                    + QuoteLots::ONE)
                    .as_u64(),
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o.is_none());
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_buy(
                Ticks::new(102) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(30)
                    / market.base_lots_per_base_unit,
                BaseLots::new(30)
            )
    );
    let (o, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_sell_with_limit_price(
                97,
                31,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o.is_none());
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_sell(
                BaseLots::new(30),
                Ticks::new(98) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(30)
                    / market.base_lots_per_base_unit,
            )
    );
    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);

    // (4) IOC should succeed if the tick/lot budget is not enough to fill the order (same params as 2)
    let (o, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_buy_with_limit_price(
                102,
                (Ticks::new(102)
                    * market.tick_size_in_quote_lots_per_base_unit
                    * BaseLots::new(30)
                    / market.base_lots_per_base_unit)
                    .as_u64(),
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o.is_none());
    // Expect two levels filled
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_buy(
                Ticks::new(101) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(10)
                    / market.base_lots_per_base_unit
                    + Ticks::new(102)
                        * market.tick_size_in_quote_lots_per_base_unit
                        * BaseLots::new(10)
                        / market.base_lots_per_base_unit,
                BaseLots::new(20)
            )
    );

    let (o, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_sell_with_limit_price(
                98,
                30,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o.is_none());
    // expect two levels filled
    assert!(
        matching_engine_response
            == MatchingEngineResponse::new_from_sell(
                BaseLots::new(20),
                Ticks::new(99) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(10)
                    / market.base_lots_per_base_unit
                    + Ticks::new(98)
                        * market.tick_size_in_quote_lots_per_base_unit
                        * BaseLots::new(10)
                        / market.base_lots_per_base_unit,
            )
    );
}

// Base lots = (quote lots * base lots per base unit) / (tick size in quote lots per base unit * price in ticks)
// Then adjust for fees.
fn get_min_base_lots_out(
    quote_lots_in: QuoteLots,
    tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
    base_lots_per_base_unit: BaseLotsPerBaseUnit,
    price_in_ticks: Ticks,
    slippage_bps: u64,
) -> BaseLots {
    let base_lots_out = ((quote_lots_in * base_lots_per_base_unit).as_u64() as f64
        / ((tick_size_in_quote_lots_per_base_unit * price_in_ticks).as_u64() as f64))
        * (1.0 - (slippage_bps as f64 / 10000.0));
    BaseLots::new(base_lots_out as u64)
}

fn get_min_quote_lots_out(
    base_lots_in: BaseLots,
    tick_size_in_quote_lots_per_base_unit: QuoteLotsPerBaseUnitPerTick,
    base_lots_per_base_unit: BaseLotsPerBaseUnit,
    price_in_ticks: Ticks,
    slippage_bps: u64,
) -> QuoteLots {
    let quote_lots_out = ((tick_size_in_quote_lots_per_base_unit * price_in_ticks * base_lots_in)
        .as_u64() as f64
        / (base_lots_per_base_unit.as_u64() as f64))
        * (1.0 - (slippage_bps as f64 / 10000.0));
    QuoteLots::new(quote_lots_out as u64)
}

#[test]
fn test_fok_with_slippage() {
    let mut rng = StdRng::seed_from_u64(2);
    let taker_bps = 5;
    let mut market = Box::new(setup_market_with_params(1000_u64, 1000_u64, taker_bps));
    let base_lots_per_base_unit = market.base_lots_per_base_unit;
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();

    for i in 1..11 {
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Bid, 100 - i, 10000 * i),
                &mut record_event_fn,
            )
            .is_some());
        assert!(market
            .place_order(
                &trader,
                OrderPacket::new_post_only_default(Side::Ask, 100 + i, 10000 * i),
                &mut record_event_fn,
            )
            .is_some());
    }

    let prev_market_state = Box::new(*market);
    let starting_ladder = market.get_typed_ladder(5);

    assert!(starting_ladder.asks[2].price_in_ticks == Ticks::new(103));
    assert!(starting_ladder.asks[2].size_in_base_lots == BaseLots::new(30000));

    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
    // Performs a swap order with a slippage of at most 50bps
    // Go through approximately 3 levels of the book
    let quote_lots_in =
        Ticks::new(100) * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(60000)
            / market.base_lots_per_base_unit;
    let slippage_bps = 50;
    let min_base_lots_out = get_min_base_lots_out(
        quote_lots_in,
        market.tick_size_in_quote_lots_per_base_unit,
        base_lots_per_base_unit,
        Ticks::new(102),
        slippage_bps,
    );

    println!("min base_lots_out: {}", min_base_lots_out);
    println!("quote_lots_in: {}", quote_lots_in);

    let (order, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_buy_with_slippage(
                quote_lots_in.as_u64(),
                min_base_lots_out.as_u64(),
            ),
            &mut record_event_fn,
        )
        .unwrap();
    println!("matching_engine_response: {:?}", matching_engine_response);
    assert!(order.is_none());

    // Ensure that the fill was within the slippage limit
    let average_price_in_ticks = (base_lots_per_base_unit.as_u64() as f64
        * matching_engine_response.num_quote_lots_in.as_u64() as f64
        / (market.tick_size_in_quote_lots_per_base_unit.as_u64() as f64))
        / matching_engine_response.num_base_lots_out.as_u64() as f64;
    println!("average_price_in_ticks: {}", average_price_in_ticks);
    let bps = (average_price_in_ticks - 102.0) / 102.0 * 10000.0;
    println!("bps: {}", bps);
    assert!(bps.floor() <= 50.0);

    let ladder = market.get_typed_ladder(5);

    let mut prev_ladder = starting_ladder.clone();
    for event in event_recorder.iter() {
        if let MarketEvent::Fill {
            order_sequence_number: order_id,
            base_lots_filled,
            price_in_ticks,
            ..
        } = event
        {
            let book = match Side::from_order_sequence_number(*order_id) {
                Side::Bid => &mut prev_ladder.bids,
                Side::Ask => &mut prev_ladder.asks,
            };
            assert!(!book.is_empty());
            assert!(book[0].price_in_ticks == *price_in_ticks);
            book[0].size_in_base_lots -= *base_lots_filled;
            if book[0].size_in_base_lots == BaseLots::ZERO {
                book.remove(0);
            }
        }
    }
    assert!(ladder.asks[0].price_in_ticks == prev_ladder.asks[0].price_in_ticks);
    assert!(ladder.asks[0].size_in_base_lots == prev_ladder.asks[0].size_in_base_lots);

    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);

    // Show that the order is rejected if the slippage is too high
    assert!(market
        .place_order(
            &taker,
            OrderPacket::new_ioc_sell_with_slippage(
                50_000,
                (Ticks::new(98)
                    * market.tick_size_in_quote_lots_per_base_unit
                    * BaseLots::new(50000)
                    / market.base_lots_per_base_unit)
                    .as_u64()
            ), // 2 full levels, 1 partial level
            &mut record_event_fn,
        )
        .is_none());

    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
    *market = *prev_market_state;
    assert!(market.get_typed_ladder(5) == starting_ladder);
    // Performs a swap sell order with a slippage of at most 28bps
    let target_bps = 28;
    let base_lots_in = BaseUnits::new(50) * base_lots_per_base_unit;
    let min_quote_lots_out = get_min_quote_lots_out(
        base_lots_in,
        market.tick_size_in_quote_lots_per_base_unit,
        base_lots_per_base_unit,
        Ticks::new(98),
        target_bps,
    );

    println!("min_quote_lots_out: {}", min_quote_lots_out);
    println!(
        "quote_lots out at price of 98: {}",
        (Ticks::new(98) * market.tick_size_in_quote_lots_per_base_unit * base_lots_in)
            / (base_lots_per_base_unit)
    );
    let adjusted_bps = (1.0
        - (min_quote_lots_out.as_u64() as f64
            / (Ticks::new(98) * market.tick_size_in_quote_lots_per_base_unit * base_lots_in
                / (base_lots_per_base_unit))
                .as_u64() as f64))
        * 10000.0;
    println!("adjusted_bps: {}", adjusted_bps);
    let (order, matching_engine_response) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_sell_with_slippage(
                base_lots_in.as_u64(),
                min_quote_lots_out.as_u64(),
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(order.is_none());
    // Ensure that the fill was within the slippage limit
    assert!(matching_engine_response.num_base_lots_in == base_lots_in);
    let average_price_in_ticks = (base_lots_per_base_unit.as_u64() as f64
        * matching_engine_response.num_quote_lots_out.as_u64() as f64
        / market.tick_size_in_quote_lots_per_base_unit.as_u64() as f64)
        / matching_engine_response.num_base_lots_in.as_u64() as f64;

    let bps = ((98.0 - average_price_in_ticks) / 98.0) * 10000.0;
    println!("average_price_in_ticks: {}", average_price_in_ticks);
    println!("bps: {}", bps);
    assert!(bps.floor() <= bps + taker_bps as f64);

    let ladder = market.get_typed_ladder(5);
    let mut prev_ladder = starting_ladder;
    for event in event_recorder.iter() {
        if let MarketEvent::Fill {
            order_sequence_number: order_id,
            base_lots_filled,
            price_in_ticks,
            ..
        } = event
        {
            let book = match Side::from_order_sequence_number(*order_id) {
                Side::Bid => &mut prev_ladder.bids,
                Side::Ask => &mut prev_ladder.asks,
            };
            assert!(!book.is_empty());
            assert!(book[0].price_in_ticks == *price_in_ticks);
            book[0].size_in_base_lots -= *base_lots_filled;
            if book[0].size_in_base_lots == BaseLots::ZERO {
                book.remove(0);
            }
        }
    }
    assert!(ladder.bids[0].price_in_ticks == prev_ladder.bids[0].price_in_ticks);
    assert!(ladder.bids[0].size_in_base_lots == prev_ladder.bids[0].size_in_base_lots);
}

#[test]
fn test_fees_basic() {
    let mut rng = StdRng::seed_from_u64(2);
    let taker_bps = 5;
    let tick_size_in_quote_lots_per_base_unit = QuoteLotsPerBaseUnitPerTick::new(10000_u64);
    let mut market = Box::new(setup_market_with_params(
        tick_size_in_quote_lots_per_base_unit.as_u64(),
        1000_u64,
        taker_bps,
    ));
    let base_lots_per_base_unit = market.base_lots_per_base_unit;
    let mut event_recorder = VecDeque::new();
    let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);

    let trader = rng.gen::<u128>();
    let taker = rng.gen::<u128>();

    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Bid, 9900, 10),
            &mut record_event_fn,
        )
        .is_some());
    assert!(market
        .place_order(
            &trader,
            OrderPacket::new_post_only_default(Side::Ask, 10100, 10),
            &mut record_event_fn,
        )
        .is_some());

    let (o_id, release_quantities) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Bid,
                10100,
                10,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o_id.is_none());
    assert!(release_quantities.num_base_lots_out == BaseLots::new(10));
    assert!(
        release_quantities.num_quote_lots_in.as_u64()
            == (Ticks::new(10100) * tick_size_in_quote_lots_per_base_unit * BaseLots::new(10)
                / base_lots_per_base_unit)
                .as_u64()
                * (10000 + taker_bps)
                / 10000
    );

    let (o_id, release_quantities) = market
        .place_order(
            &taker,
            OrderPacket::new_ioc_by_lots(
                Side::Ask,
                9900,
                10,
                SelfTradeBehavior::Abort,
                None,
                rng.gen::<u128>(),
                false,
            ),
            &mut record_event_fn,
        )
        .unwrap();
    assert!(o_id.is_none());
    assert!(release_quantities.num_base_lots_in == BaseLots::new(10));
    assert!(
        release_quantities.num_quote_lots_out.as_u64()
            == (Ticks::new(9900) * tick_size_in_quote_lots_per_base_unit * BaseLots::new(10)
                / base_lots_per_base_unit)
                .as_u64()
                * (10000 - taker_bps)
                / 10000
    );

    market.collect_fees(&mut record_event_fn);
    assert_eq!(market.get_uncollected_fee_amount(), QuoteLots::ZERO);
}

#[test]
fn test_evict_order() {
    let mut rng = StdRng::seed_from_u64(2);

    let trader = rng.gen::<u128>();
    let stink_order = rng.gen::<u128>();
    let evicter = rng.gen::<u128>();
    for side in [Side::Bid, Side::Ask].into_iter() {
        let mut market = setup_market();

        let mut event_recorder = VecDeque::new();
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        let price = Ticks::new(1000);
        for _ in 0..market.get_book(side).capacity() - 1 {
            market.place_order(
                &trader,
                OrderPacket::new_post_only_default(side, price.as_u64(), 1),
                &mut record_event_fn,
            );
        }
        let direction = match side {
            Side::Bid => -1,
            Side::Ask => 1,
        };
        let stink_price = Ticks::new((price.as_u64() as i64 + direction * 500) as u64);
        market.place_order(
            &stink_order,
            OrderPacket::new_post_only_default(side, stink_price.as_u64(), 99),
            &mut record_event_fn,
        );
        // Order must be more aggressive than the least aggressive order in a full book
        assert!(market
            .place_order(
                &stink_order,
                OrderPacket::new_post_only_default(side, stink_price.as_u64(), 99),
                &mut record_event_fn,
            )
            .is_none());
        let mut event_recorder = VecDeque::new();
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        assert!(market
            .place_order(
                &evicter,
                OrderPacket::new_post_only_default(
                    side,
                    (price.as_u64() as i64 + direction) as u64,
                    99
                ),
                &mut record_event_fn,
            )
            .is_some());

        event_recorder.pop_back();
        let evict_event = *event_recorder.back().unwrap();
        if let MarketEvent::Evict {
            order_sequence_number: order_id,
            price_in_ticks,
            maker_id,
            base_lots_evicted: base_lots_removed,
        } = evict_event
        {
            assert!(Side::from_order_sequence_number(order_id) == side);
            assert_eq!(price_in_ticks, stink_price);
            assert_eq!(maker_id, stink_order);
            assert_eq!(base_lots_removed, BaseLots::new(99));
            let trader_state = market.traders.get(&stink_order).unwrap();
            if side == Side::Ask {
                assert_eq!(trader_state.base_lots_free, BaseLots::new(99));
            } else {
                assert_eq!(
                    trader_state.quote_lots_free,
                    stink_price * market.tick_size_in_quote_lots_per_base_unit * BaseLots::new(99)
                        / market.base_lots_per_base_unit
                );
            }
        } else {
            panic!("Expected evict event");
        }
    }
}

#[test]
fn test_reduce_order() {
    let mut rng = StdRng::seed_from_u64(2);
    let mut market = setup_market();
    let maker = rng.gen::<u128>();
    let mut event_recorder = VecDeque::new();

    let mut client_ids = vec![];
    client_ids.push(rng.gen::<u128>());
    let order_packet = OrderPacket::new_post_only_default_with_client_order_id(
        Side::Bid,
        1000,
        100,
        client_ids[0],
    );

    {
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        market
            .place_order(&maker, order_packet, &mut record_event_fn)
            .unwrap();
    }

    let event = event_recorder.pop_back().unwrap();
    let order_id = if let MarketEvent::<u128>::Place {
        order_sequence_number,
        price_in_ticks,
        base_lots_placed,
        client_order_id,
    } = event
    {
        assert!(Side::from_order_sequence_number(order_sequence_number) == Side::Bid);
        assert_eq!(price_in_ticks, Ticks::new(1000));
        assert_eq!(base_lots_placed, BaseLots::new(100));
        assert_eq!(client_order_id, client_ids[0]);
        FIFOOrderId::new(price_in_ticks, order_sequence_number)
    } else {
        panic!("Expected place event");
    };

    {
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        market
            .reduce_order(
                &maker,
                &order_id,
                Side::Bid,
                Some(BaseLots::new(10)),
                true,
                &mut record_event_fn,
            )
            .unwrap();
    }

    let event = event_recorder.pop_back().unwrap();
    if let MarketEvent::<u128>::Reduce {
        order_sequence_number,
        price_in_ticks,
        base_lots_removed,
        base_lots_remaining,
    } = event
    {
        assert!(Side::from_order_sequence_number(order_sequence_number) == Side::Bid);
        assert_eq!(price_in_ticks, Ticks::new(1000));
        assert_eq!(base_lots_removed, BaseLots::new(10));
        assert_eq!(base_lots_remaining, BaseLots::new(90));
    } else {
        panic!("Expected reduce event");
    }
    assert!(market.bids.get(&order_id).is_some());

    let random_maker = rng.gen::<u128>();
    {
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        market
            .place_order(&random_maker, order_packet, &mut record_event_fn)
            .unwrap();
        assert!(
            market
                .reduce_order(
                    &random_maker,
                    &order_id,
                    Side::Bid,
                    Some(BaseLots::new(10)),
                    true,
                    &mut record_event_fn,
                )
                .is_none(),
            "Trader ID must match order"
        );

        assert_eq!(
            market
                .reduce_order(
                    &maker,
                    &FIFOOrderId::new_from_untyped(rng.gen::<u64>(), rng.gen::<u64>()),
                    Side::Bid,
                    Some(BaseLots::new(10)),
                    true,
                    &mut record_event_fn,
                )
                .unwrap(),
            MatchingEngineResponse::default(),
            "Order ID not in book"
        );
    }
    // If we pass in more size than is in the order, it should reduce the order to zero and should be removed from the book
    {
        let mut record_event_fn = |e: MarketEvent<TraderId>| event_recorder.push_back(e);
        market
            .reduce_order(
                &maker,
                &order_id,
                Side::Bid,
                Some(BaseLots::new(100)),
                true,
                &mut record_event_fn,
            )
            .unwrap();
    }
    let event = event_recorder.pop_back().unwrap();
    if let MarketEvent::<u128>::Reduce {
        order_sequence_number,
        price_in_ticks,
        base_lots_removed,
        base_lots_remaining,
    } = event
    {
        assert!(Side::from_order_sequence_number(order_sequence_number) == Side::Bid);
        assert_eq!(price_in_ticks, Ticks::new(1000));
        assert_eq!(base_lots_removed, BaseLots::new(90));
        assert_eq!(base_lots_remaining, BaseLots::new(0));
    } else {
        panic!("Expected reduce event");
    }

    assert!(market.bids.get(&order_id).is_none());
}
