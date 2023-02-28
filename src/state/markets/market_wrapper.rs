use super::{Market, WritableMarket};

/// Struct that holds an object implementing the WritableMarket trait.
pub(crate) struct MarketWrapperMut<
    'a,
    MarketTraderId,
    MarketOrderId,
    MarketRestingOrder,
    MarketOrderPacket,
> {
    pub inner: &'a mut dyn WritableMarket<
        MarketTraderId,
        MarketOrderId,
        MarketRestingOrder,
        MarketOrderPacket,
    >,
}

impl<'a, MarketTraderId, MarketOrderPacket, MarketRestingOrder, MarketOrderId>
    MarketWrapperMut<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
{
    pub(crate) fn new(
        market: &'a mut dyn WritableMarket<
            MarketTraderId,
            MarketOrderId,
            MarketRestingOrder,
            MarketOrderPacket,
        >,
    ) -> Self {
        Self { inner: market }
    }
}

/// Struct that holds an object implementing the Market trait.
pub struct MarketWrapper<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket> {
    pub inner: &'a dyn Market<MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>,
}

impl<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
    MarketWrapper<'a, MarketTraderId, MarketOrderId, MarketRestingOrder, MarketOrderPacket>
{
    pub fn new(
        market: &'a dyn Market<
            MarketTraderId,
            MarketOrderId,
            MarketRestingOrder,
            MarketOrderPacket,
        >,
    ) -> Self {
        Self { inner: market }
    }
}
