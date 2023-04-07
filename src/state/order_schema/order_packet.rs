// By aliasing the BorshDeserialize and BorshSerialize traits, we prevent Shank from
// writing structs with these annotations to the IDL.
use borsh::{BorshDeserialize as Deserialize, BorshSerialize as Serialize};

use crate::{
    quantities::{BaseLots, QuoteLots, Ticks, WrapperU64},
    state::{SelfTradeBehavior, Side},
};

pub trait OrderPacketMetadata {
    fn is_take_only(&self) -> bool {
        self.is_ioc() || self.is_fok()
    }
    fn is_ioc(&self) -> bool;
    fn is_fok(&self) -> bool;
    fn is_post_only(&self) -> bool;
    fn no_deposit_or_withdrawal(&self) -> bool;
}

#[derive(Deserialize, Serialize, Copy, Clone, PartialEq, Eq, Debug)]
pub enum OrderPacket {
    /// This order type is used to place a limit order on the book.
    /// It will never be matched against other existing limit orders
    PostOnly {
        side: Side,

        /// The price of the order, in ticks
        price_in_ticks: Ticks,

        /// Number of base lots to place on the book
        num_base_lots: BaseLots,

        /// Client order id used to identify the order in the response to the client
        client_order_id: u128,

        /// Flag for whether or not to reject the order if it would immediately match or amend it to the best non-crossing price
        /// Default value is true
        reject_post_only: bool,

        /// Flag for whether or not the order should only use funds that are already in the account
        /// Using only deposited funds will allow the trader to pass in less accounts per instruction and
        /// save transaction space as well as compute. This is only for traders who have a seat
        use_only_deposited_funds: bool,

        /// If this is set, the order will be invalid after the specified slot
        last_valid_slot: Option<u64>,

        /// If this is set, the order will be invalid after the specified unix timestamp
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },

    /// This order type is used to place a limit order on the book
    /// It can be matched against other existing limit orders, but will posted at the
    /// specified level if it is not matched
    Limit {
        side: Side,

        /// The price of the order, in ticks
        price_in_ticks: Ticks,

        /// Total number of base lots to place on the book or fill at a better price
        num_base_lots: BaseLots,

        /// How the matching engine should handle a self trade
        self_trade_behavior: SelfTradeBehavior,

        /// Number of orders to match against. If this is `None` there is no limit
        match_limit: Option<u64>,

        /// Client order id used to identify the order in the response to the client
        client_order_id: u128,

        /// Flag for whether or not the order should only use funds that are already in the account.
        /// Using only deposited funds will allow the trader to pass in less accounts per instruction and
        /// save transaction space as well as compute. This is only for traders who have a seat
        use_only_deposited_funds: bool,

        /// If this is set, the order will be invalid after the specified slot
        last_valid_slot: Option<u64>,

        /// If this is set, the order will be invalid after the specified unix timestamp
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },

    /// This order type is used to place an order that will be matched against existing resting orders
    /// If the order matches fewer than `min_lots` lots, it will be cancelled.
    ///
    /// Fill or Kill (FOK) orders are a subset of Immediate or Cancel (IOC) orders where either
    /// the `num_base_lots` is equal to the `min_base_lots_to_fill` of the order, or the `num_quote_lots` is
    /// equal to the `min_quote_lots_to_fill` of the order.
    ImmediateOrCancel {
        side: Side,

        /// The most aggressive price an order can be matched at. For example, if there is an IOC buy order
        /// to purchase 10 lots with the tick_per_lot parameter set to 10, then the order will never
        /// be matched at a price higher than 10 quote ticks per base unit. If this value is None, then the order
        /// is treated as a market order.
        price_in_ticks: Option<Ticks>,

        /// The number of base lots to fill against the order book. Either this parameter or the `num_quote_lots`
        /// parameter must be set to a nonzero value.
        num_base_lots: BaseLots,

        /// The number of quote lots to fill against the order book. Either this parameter or the `num_base_lots`
        /// parameter must be set to a nonzero value.
        num_quote_lots: QuoteLots,

        /// The minimum number of base lots to fill against the order book. If the order does not fill
        /// this many base lots, it will be voided.
        min_base_lots_to_fill: BaseLots,

        /// The minimum number of quote lots to fill against the order book. If the order does not fill
        /// this many quote lots, it will be voided.
        min_quote_lots_to_fill: QuoteLots,

        /// How the matching engine should handle a self trade.
        self_trade_behavior: SelfTradeBehavior,

        /// Number of orders to match against. If set to `None`, there is no limit.
        match_limit: Option<u64>,

        /// Client order id used to identify the order in the program's inner instruction data.
        client_order_id: u128,

        /// Flag for whether or not the order should only use funds that are already in the account.
        /// Using only deposited funds will allow the trader to pass in less accounts per instruction and
        /// save transaction space as well as compute. This is only for traders who have a seat
        use_only_deposited_funds: bool,

        /// If this is set, the order will be invalid after the specified slot
        last_valid_slot: Option<u64>,

        /// If this is set, the order will be invalid after the specified unix timestamp
        last_valid_unix_timestamp_in_seconds: Option<u64>,
    },
}

impl OrderPacketMetadata for OrderPacket {
    fn is_ioc(&self) -> bool {
        matches!(self, OrderPacket::ImmediateOrCancel { .. })
    }

    fn is_fok(&self) -> bool {
        match self {
            &Self::ImmediateOrCancel {
                num_base_lots,
                num_quote_lots,
                min_base_lots_to_fill,
                min_quote_lots_to_fill,
                ..
            } => {
                num_base_lots > BaseLots::ZERO && num_base_lots == min_base_lots_to_fill
                    || num_quote_lots > QuoteLots::ZERO && num_quote_lots == min_quote_lots_to_fill
            }
            _ => false,
        }
    }

    fn is_post_only(&self) -> bool {
        matches!(self, OrderPacket::PostOnly { .. })
    }

    fn no_deposit_or_withdrawal(&self) -> bool {
        match *self {
            Self::PostOnly {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
            Self::Limit {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
            Self::ImmediateOrCancel {
                use_only_deposited_funds,
                ..
            } => use_only_deposited_funds,
        }
    }
}

impl OrderPacket {
    pub fn new_post_only_default(side: Side, price_in_ticks: u64, num_base_lots: u64) -> Self {
        Self::PostOnly {
            side,
            price_in_ticks: Ticks::new(price_in_ticks),
            num_base_lots: BaseLots::new(num_base_lots),
            client_order_id: 0,
            reject_post_only: true,
            use_only_deposited_funds: false,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }

    pub fn new_post_only_default_with_client_order_id(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
    ) -> Self {
        Self::PostOnly {
            side,
            price_in_ticks: Ticks::new(price_in_ticks),
            num_base_lots: BaseLots::new(num_base_lots),
            client_order_id,
            reject_post_only: true,
            use_only_deposited_funds: false,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }

    pub fn new_adjustable_post_only_default_with_client_order_id(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
    ) -> Self {
        Self::PostOnly {
            side,
            price_in_ticks: Ticks::new(price_in_ticks),
            num_base_lots: BaseLots::new(num_base_lots),
            client_order_id,
            reject_post_only: false,
            use_only_deposited_funds: false,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }

    pub fn new_post_only(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
        reject_post_only: bool,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::PostOnly {
            side,
            price_in_ticks: Ticks::new(price_in_ticks),
            num_base_lots: BaseLots::new(num_base_lots),
            client_order_id,
            reject_post_only,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }

    pub fn new_limit_order_default(side: Side, price_in_ticks: u64, num_base_lots: u64) -> Self {
        Self::new_limit_order(
            side,
            price_in_ticks,
            num_base_lots,
            SelfTradeBehavior::CancelProvide,
            None,
            0,
            false,
        )
    }

    pub fn new_limit_order_default_with_client_order_id(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
    ) -> Self {
        Self::new_limit_order(
            side,
            price_in_ticks,
            num_base_lots,
            SelfTradeBehavior::CancelProvide,
            None,
            client_order_id,
            false,
        )
    }

    pub fn new_limit_order(
        side: Side,
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::Limit {
            side,
            price_in_ticks: Ticks::new(price_in_ticks),
            num_base_lots: BaseLots::new(num_base_lots),
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        }
    }

    pub fn new_fok_sell_with_limit_price(
        target_price_in_ticks: u64,
        base_lot_budget: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            Side::Ask,
            Some(target_price_in_ticks),
            base_lot_budget,
            0,
            base_lot_budget,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            None,
            None,
        )
    }

    pub fn new_fok_buy_with_limit_price(
        target_price_in_ticks: u64,
        base_lot_budget: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            Side::Bid,
            Some(target_price_in_ticks),
            base_lot_budget,
            0,
            base_lot_budget,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            None,
            None,
        )
    }

    pub fn new_ioc_sell_with_limit_price(
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            Side::Ask,
            Some(price_in_ticks),
            num_base_lots,
            0,
            0,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            None,
            None,
        )
    }

    pub fn new_ioc_buy_with_limit_price(
        price_in_ticks: u64,
        num_quote_lots: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            Side::Bid,
            Some(price_in_ticks),
            0,
            num_quote_lots,
            0,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            None,
            None,
        )
    }

    pub fn new_ioc_by_base_lots(
        side: Side,
        price_in_ticks: u64,
        base_lot_budget: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            side,
            Some(price_in_ticks),
            base_lot_budget,
            0,
            0,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            None,
            None,
        )
    }

    pub fn new_ioc_by_quote_lots(
        side: Side,
        price_in_ticks: u64,
        quote_lot_budget: u64,
        self_trade_behavior: SelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        use_only_deposited_funds: bool,
    ) -> Self {
        Self::new_ioc(
            side,
            Some(price_in_ticks),
            0,
            quote_lot_budget,
            0,
            0,
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
        )
    }

    pub fn new_ioc_buy_with_slippage(quote_lots_in: u64, min_base_lots_out: u64) -> Self {
        Self::new_ioc(
            Side::Bid,
            None,
            0,
            quote_lots_in,
            min_base_lots_out,
            0,
            SelfTradeBehavior::CancelProvide,
            None,
            0,
            false,
            None,
            None,
        )
    }

    pub fn new_ioc_sell_with_slippage(base_lots_in: u64, min_quote_lots_out: u64) -> Self {
        Self::new_ioc(
            Side::Ask,
            None,
            base_lots_in,
            0,
            0,
            min_quote_lots_out,
            SelfTradeBehavior::CancelProvide,
            None,
            0,
            false,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_ioc(
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
    ) -> Self {
        Self::ImmediateOrCancel {
            side,
            price_in_ticks: price_in_ticks.map(Ticks::new),
            num_base_lots: BaseLots::new(num_base_lots),
            num_quote_lots: QuoteLots::new(num_quote_lots),
            min_base_lots_to_fill: BaseLots::new(min_base_lots_to_fill),
            min_quote_lots_to_fill: QuoteLots::new(min_quote_lots_to_fill),
            self_trade_behavior,
            match_limit,
            client_order_id,
            use_only_deposited_funds,
            last_valid_slot,
            last_valid_unix_timestamp_in_seconds,
        }
    }
}

impl OrderPacket {
    pub fn side(&self) -> Side {
        match self {
            Self::PostOnly { side, .. } => *side,
            Self::Limit { side, .. } => *side,
            Self::ImmediateOrCancel { side, .. } => *side,
        }
    }

    pub fn client_order_id(&self) -> u128 {
        match self {
            Self::PostOnly {
                client_order_id, ..
            } => *client_order_id,
            Self::Limit {
                client_order_id, ..
            } => *client_order_id,
            Self::ImmediateOrCancel {
                client_order_id, ..
            } => *client_order_id,
        }
    }

    pub fn num_base_lots(&self) -> BaseLots {
        match self {
            Self::PostOnly { num_base_lots, .. } => *num_base_lots,
            Self::Limit { num_base_lots, .. } => *num_base_lots,
            Self::ImmediateOrCancel { num_base_lots, .. } => *num_base_lots,
        }
    }

    pub fn num_quote_lots(&self) -> QuoteLots {
        match self {
            Self::PostOnly { .. } => QuoteLots::ZERO,
            Self::Limit { .. } => QuoteLots::ZERO,
            Self::ImmediateOrCancel { num_quote_lots, .. } => *num_quote_lots,
        }
    }

    pub fn base_lot_budget(&self) -> BaseLots {
        let base_lots = self.num_base_lots();
        if base_lots == BaseLots::ZERO {
            BaseLots::MAX
        } else {
            base_lots
        }
    }

    pub fn quote_lot_budget(&self) -> Option<QuoteLots> {
        let quote_lots = self.num_quote_lots();
        if quote_lots == QuoteLots::ZERO {
            None
        } else {
            Some(quote_lots)
        }
    }

    pub fn match_limit(&self) -> u64 {
        match self {
            Self::PostOnly { .. } => u64::MAX,
            Self::Limit { match_limit, .. } => match_limit.unwrap_or(u64::MAX),
            Self::ImmediateOrCancel { match_limit, .. } => match_limit.unwrap_or(u64::MAX),
        }
    }

    pub fn self_trade_behavior(&self) -> SelfTradeBehavior {
        match self {
            Self::PostOnly { .. } => panic!("PostOnly orders do not have a self trade behavior"),
            Self::Limit {
                self_trade_behavior,
                ..
            } => *self_trade_behavior,
            Self::ImmediateOrCancel {
                self_trade_behavior,
                ..
            } => *self_trade_behavior,
        }
    }

    pub fn get_price_in_ticks(&self) -> Ticks {
        match self {
            Self::PostOnly { price_in_ticks, .. } => *price_in_ticks,
            Self::Limit { price_in_ticks, .. } => *price_in_ticks,
            Self::ImmediateOrCancel { price_in_ticks, .. } => {
                price_in_ticks.unwrap_or(match self.side() {
                    Side::Bid => Ticks::MAX,
                    Side::Ask => Ticks::MIN,
                })
            }
        }
    }

    pub fn set_price_in_ticks(&mut self, price_in_ticks: Ticks) {
        match self {
            Self::PostOnly {
                price_in_ticks: old_price_in_ticks,
                ..
            } => *old_price_in_ticks = price_in_ticks,
            Self::Limit {
                price_in_ticks: old_price_in_ticks,
                ..
            } => *old_price_in_ticks = price_in_ticks,
            Self::ImmediateOrCancel {
                price_in_ticks: old_price_in_ticks,
                ..
            } => *old_price_in_ticks = Some(price_in_ticks),
        }
    }

    pub fn get_last_valid_slot(&self) -> Option<u64> {
        match self {
            Self::PostOnly {
                last_valid_slot, ..
            } => *last_valid_slot,
            Self::Limit {
                last_valid_slot, ..
            } => *last_valid_slot,
            Self::ImmediateOrCancel {
                last_valid_slot, ..
            } => *last_valid_slot,
        }
    }

    pub fn get_last_valid_unix_timestamp_in_seconds(&self) -> Option<u64> {
        match self {
            Self::PostOnly {
                last_valid_unix_timestamp_in_seconds,
                ..
            } => *last_valid_unix_timestamp_in_seconds,
            Self::Limit {
                last_valid_unix_timestamp_in_seconds,
                ..
            } => *last_valid_unix_timestamp_in_seconds,
            Self::ImmediateOrCancel {
                last_valid_unix_timestamp_in_seconds,
                ..
            } => *last_valid_unix_timestamp_in_seconds,
        }
    }

    pub fn is_expired(&self, current_slot: u64, current_unix_timestamp_in_seconds: u64) -> bool {
        if let Some(last_valid_slot) = self.get_last_valid_slot() {
            if current_slot > last_valid_slot {
                return true;
            }
        }
        if let Some(last_valid_unix_timestamp_in_seconds) =
            self.get_last_valid_unix_timestamp_in_seconds()
        {
            if current_unix_timestamp_in_seconds > last_valid_unix_timestamp_in_seconds {
                return true;
            }
        }
        false
    }
}

pub fn decode_order_packet(bytes: &[u8]) -> Option<OrderPacket> {
    let order_packet = match OrderPacket::try_from_slice(bytes) {
        Ok(order_packet) => order_packet,
        Err(_) => {
            // Options with a None value are encoded with a 0 byte.
            // If the input data is missing the `last_valid_slot` and `last_valid_unix_timestamp_in_seconds`
            // fields on the order packet, this function infers these parameters to be None and tries
            // to decode the order packet again.
            let padded_bytes = [bytes, &[0_u8, 0_u8]].concat();
            let order_packet = OrderPacket::try_from_slice(&padded_bytes).ok()?;
            if order_packet.get_last_valid_slot().is_some()
                || order_packet
                    .get_last_valid_unix_timestamp_in_seconds()
                    .is_some()
            {
                return None;
            }
            order_packet
        }
    };
    Some(order_packet)
}

#[test]
fn test_decode_order_packet() {
    use rand::Rng;
    use rand::{rngs::StdRng, SeedableRng};
    let mut rng = StdRng::seed_from_u64(42);

    let num_iters = 100;

    #[derive(Deserialize, Serialize, Copy, Clone, PartialEq, Eq, Debug)]
    pub enum DeprecatedOrderPacket {
        PostOnly {
            side: Side,
            price_in_ticks: Ticks,
            num_base_lots: BaseLots,
            client_order_id: u128,
            reject_post_only: bool,
            use_only_deposited_funds: bool,
        },
        Limit {
            side: Side,
            price_in_ticks: Ticks,
            num_base_lots: BaseLots,
            self_trade_behavior: SelfTradeBehavior,
            match_limit: Option<u64>,
            client_order_id: u128,
            use_only_deposited_funds: bool,
        },

        ImmediateOrCancel {
            side: Side,
            price_in_ticks: Option<Ticks>,
            num_base_lots: BaseLots,
            num_quote_lots: QuoteLots,
            min_base_lots_to_fill: BaseLots,
            min_quote_lots_to_fill: QuoteLots,
            self_trade_behavior: SelfTradeBehavior,
            match_limit: Option<u64>,
            client_order_id: u128,
            use_only_deposited_funds: bool,
        },
    }
    for _ in 0..num_iters {
        let side = if rng.gen::<f64>() > 0.5 {
            Side::Bid
        } else {
            Side::Ask
        };

        let price_in_ticks = Ticks::new(rng.gen::<u64>());
        let num_base_lots = BaseLots::new(rng.gen::<u64>());
        let client_order_id = rng.gen::<u128>();
        let reject_post_only = rng.gen::<bool>();
        let use_only_deposited_funds = rng.gen::<bool>();
        let packet = OrderPacket::PostOnly {
            side,
            price_in_ticks,
            num_base_lots,
            client_order_id,
            reject_post_only,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        };
        let deprecated_packet = DeprecatedOrderPacket::PostOnly {
            side,
            price_in_ticks,
            num_base_lots,
            client_order_id,
            reject_post_only,
            use_only_deposited_funds,
        };
        let bytes = packet.try_to_vec().unwrap();
        let decoded_normal = decode_order_packet(&bytes).unwrap();
        let decoded_inferred = decode_order_packet(&bytes[..bytes.len() - 2]).unwrap();
        let deprecated_bytes = deprecated_packet.try_to_vec().unwrap();
        let decoded_deprecated = decode_order_packet(&deprecated_bytes).unwrap();
        assert_eq!(packet, decoded_normal);
        assert_eq!(decoded_normal, decoded_inferred);
        assert_eq!(decoded_inferred, decoded_deprecated);
    }

    for _ in 0..num_iters {
        let side = if rng.gen::<f64>() > 0.5 {
            Side::Bid
        } else {
            Side::Ask
        };

        let price_in_ticks = Ticks::new(rng.gen::<u64>());
        let num_base_lots = BaseLots::new(rng.gen::<u64>());
        let client_order_id = rng.gen::<u128>();
        let self_trade_behavior = match rng.gen_range(0, 3) {
            0 => SelfTradeBehavior::DecrementTake,
            1 => SelfTradeBehavior::CancelProvide,
            2 => SelfTradeBehavior::Abort,
            _ => unreachable!(),
        };
        let match_limit = if rng.gen::<f64>() > 0.5 {
            Some(rng.gen::<u64>())
        } else {
            None
        };
        let use_only_deposited_funds = rng.gen::<bool>();
        let packet = OrderPacket::Limit {
            side,
            price_in_ticks,
            num_base_lots,
            client_order_id,
            self_trade_behavior,
            match_limit,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        };
        let deprecated_packet = DeprecatedOrderPacket::Limit {
            side,
            price_in_ticks,
            num_base_lots,
            client_order_id,
            self_trade_behavior,
            match_limit,
            use_only_deposited_funds,
        };
        let bytes = packet.try_to_vec().unwrap();
        let decoded_normal = decode_order_packet(&bytes).unwrap();
        let decoded_inferred = decode_order_packet(&bytes[..bytes.len() - 2]).unwrap();
        let deprecated_bytes = deprecated_packet.try_to_vec().unwrap();
        let decoded_deprecated = decode_order_packet(&deprecated_bytes).unwrap();
        assert_eq!(packet, decoded_normal);
        assert_eq!(decoded_normal, decoded_inferred);
        assert_eq!(decoded_inferred, decoded_deprecated);
    }

    for _ in 0..num_iters {
        let side = if rng.gen::<f64>() > 0.5 {
            Side::Bid
        } else {
            Side::Ask
        };

        let price_in_ticks = if rng.gen::<f64>() > 0.5 {
            Some(Ticks::new(rng.gen::<u64>()))
        } else {
            None
        };
        let num_base_lots = BaseLots::new(rng.gen::<u64>());
        let min_base_lots_to_fill = BaseLots::new(rng.gen::<u64>());
        let num_quote_lots = QuoteLots::new(rng.gen::<u64>());
        let min_quote_lots_to_fill = QuoteLots::new(rng.gen::<u64>());
        let client_order_id = rng.gen::<u128>();
        let self_trade_behavior = match rng.gen_range(0, 3) {
            0 => SelfTradeBehavior::DecrementTake,
            1 => SelfTradeBehavior::CancelProvide,
            2 => SelfTradeBehavior::Abort,
            _ => unreachable!(),
        };
        let match_limit = if rng.gen::<f64>() > 0.5 {
            Some(rng.gen::<u64>())
        } else {
            None
        };
        let use_only_deposited_funds = rng.gen::<bool>();
        let packet = OrderPacket::ImmediateOrCancel {
            side,
            price_in_ticks,
            num_base_lots,
            num_quote_lots,
            min_base_lots_to_fill,
            min_quote_lots_to_fill,
            client_order_id,
            self_trade_behavior,
            match_limit,
            use_only_deposited_funds,
            last_valid_slot: None,
            last_valid_unix_timestamp_in_seconds: None,
        };
        let deprecated_packet = DeprecatedOrderPacket::ImmediateOrCancel {
            side,
            price_in_ticks,
            num_base_lots,
            num_quote_lots,
            min_base_lots_to_fill,
            min_quote_lots_to_fill,
            client_order_id,
            self_trade_behavior,
            match_limit,
            use_only_deposited_funds,
        };
        let bytes = packet.try_to_vec().unwrap();
        let decoded_normal = decode_order_packet(&bytes).unwrap();
        let decoded_inferred = decode_order_packet(&bytes[..bytes.len() - 2]).unwrap();
        let deprecated_bytes = deprecated_packet.try_to_vec().unwrap();
        let decoded_deprecated = decode_order_packet(&deprecated_bytes).unwrap();
        assert_eq!(packet, decoded_normal);
        assert_eq!(decoded_normal, decoded_inferred);
        assert_eq!(decoded_inferred, decoded_deprecated);
    }
}
