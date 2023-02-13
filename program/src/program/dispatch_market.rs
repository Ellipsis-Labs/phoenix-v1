use super::error::{assert_with_msg, PhoenixError};
use super::MarketSizeParams;
use crate::state::markets::{
    FIFOMarket, FIFOOrderId, FIFORestingOrder, Market, MarketWrapper, MarketWrapperMut,
};
use crate::state::OrderPacket;
use sokoban::node_allocator::ZeroCopy;
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

pub fn load_with_dispatch_mut<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a mut [u8],
) -> Result<MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError>
{
    dispatch_market_mut(market_size_params, bytes, false)
}

pub fn load_with_dispatch_init<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a mut [u8],
) -> Result<MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError>
{
    dispatch_market_mut(market_size_params, bytes, true)
}

pub fn dispatch_market_mut<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a mut [u8],
    is_initial: bool,
) -> Result<MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError>
{
    let MarketSizeParams {
        bids_size,
        asks_size,
        num_seats,
    } = market_size_params;
    let market = match (bids_size, asks_size, num_seats) {
        (512, 512, 256) => FIFOMarket::<Pubkey, 512, 512, 256>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (2048, 2048, 4096) => FIFOMarket::<Pubkey, 2048, 2048, 4096>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (4096, 4096, 8192) => FIFOMarket::<Pubkey, 4096, 4096, 8192>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (1024, 1024, 128) => FIFOMarket::<Pubkey, 1024, 1024, 128>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (2048, 2048, 128) => FIFOMarket::<Pubkey, 2048, 2048, 128>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (4096, 4096, 128) => FIFOMarket::<Pubkey, 4096, 4096, 128>::load_mut_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        _ => {
            phoenix_log!("Invalid parameters for market");
            return Err(PhoenixError::InvalidMarketParameters.into());
        }
    };
    if !is_initial {
        assert_with_msg(
            market.get_sequence_number() > 0,
            PhoenixError::MarketUninitialized,
            "Market is not inialized",
        )?;
    }
    Ok(MarketWrapperMut::<
        Pubkey,
        FIFOOrderId,
        FIFORestingOrder,
        OrderPacket,
    >::new(market))
}

/// Loads a market from a given buffer and known market params.
pub fn load_with_dispatch<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a [u8],
) -> Result<MarketWrapper<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError> {
    dispatch_market(market_size_params, bytes)
}

fn dispatch_market<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a [u8],
) -> Result<MarketWrapper<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError> {
    let market = match (
        market_size_params.bids_size,
        market_size_params.asks_size,
        market_size_params.num_seats,
    ) {
        (512, 512, 256) => FIFOMarket::<Pubkey, 512, 512, 256>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (2048, 2048, 4096) => FIFOMarket::<Pubkey, 2048, 2048, 4096>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (4096, 4096, 8192) => FIFOMarket::<Pubkey, 4096, 4096, 8192>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (1024, 1024, 128) => FIFOMarket::<Pubkey, 1024, 1024, 128>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (2048, 2048, 128) => FIFOMarket::<Pubkey, 2048, 2048, 128>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        (4096, 4096, 128) => FIFOMarket::<Pubkey, 4096, 4096, 128>::load_bytes(bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>,
        _ => {
            phoenix_log!("Invalid parameters for market");
            return Err(PhoenixError::InvalidMarketParameters.into());
        }
    };
    Ok(MarketWrapper::<
        Pubkey,
        FIFOOrderId,
        FIFORestingOrder,
        OrderPacket,
    >::new(market))
}

pub fn get_market_size(market_size_params: &MarketSizeParams) -> Result<usize, ProgramError> {
    let MarketSizeParams {
        bids_size,
        asks_size,
        num_seats,
    } = market_size_params;
    let size = match (bids_size, asks_size, num_seats) {
        (512, 512, 256) => std::mem::size_of::<FIFOMarket<Pubkey, 512, 512, 256>>(),
        (2048, 2048, 4096) => std::mem::size_of::<FIFOMarket<Pubkey, 2048, 2048, 4096>>(),
        (4096, 4096, 8192) => std::mem::size_of::<FIFOMarket<Pubkey, 4096, 4096, 8192>>(),
        (1024, 1024, 128) => std::mem::size_of::<FIFOMarket<Pubkey, 1024, 1024, 128>>(),
        (2048, 2048, 128) => std::mem::size_of::<FIFOMarket<Pubkey, 2048, 2048, 128>>(),
        (4096, 4096, 128) => std::mem::size_of::<FIFOMarket<Pubkey, 4096, 4096, 128>>(),
        _ => {
            phoenix_log!("Invalid parameters for market");
            return Err(PhoenixError::InvalidMarketParameters.into());
        }
    };
    Ok(size)
}
