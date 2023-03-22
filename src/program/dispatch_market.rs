use super::error::{assert_with_msg, PhoenixError};
use super::MarketSizeParams;
use crate::state::markets::{
    FIFOMarket, FIFOOrderId, FIFORestingOrder, Market, MarketWrapper, MarketWrapperMut,
    WritableMarket,
};
use crate::state::OrderPacket;
use sokoban::node_allocator::ZeroCopy;
use solana_program::{program_error::ProgramError, pubkey::Pubkey};

macro_rules! fifo_market_mut {
    ($num_bids:literal, $num_asks:literal, $num_seats:literal, $bytes:expr) => {
        FIFOMarket::<Pubkey, $num_bids, $num_asks, $num_seats>::load_mut_bytes($bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &mut dyn WritableMarket<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>
    };
}

macro_rules! fifo_market {
    ($num_bids:literal, $num_asks:literal, $num_seats:literal, $bytes:expr) => {
        FIFOMarket::<Pubkey, $num_bids, $num_asks, $num_seats>::load_bytes($bytes)
            .ok_or(PhoenixError::FailedToLoadMarketFromAccount)?
            as &dyn Market<Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>
    };
}

macro_rules! fifo_market_size {
    ($num_bids:literal, $num_asks:literal, $num_seats:literal) => {
        std::mem::size_of::<FIFOMarket<Pubkey, $num_bids, $num_asks, $num_seats>>()
    };
}

pub(crate) fn load_with_dispatch_mut<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a mut [u8],
) -> Result<MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError>
{
    dispatch_market_mut(market_size_params, bytes, false)
}

pub(crate) fn load_with_dispatch_init<'a>(
    market_size_params: &'a MarketSizeParams,
    bytes: &'a mut [u8],
) -> Result<MarketWrapperMut<'a, Pubkey, FIFOOrderId, FIFORestingOrder, OrderPacket>, ProgramError>
{
    dispatch_market_mut(market_size_params, bytes, true)
}

pub(crate) fn dispatch_market_mut<'a>(
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
        (512, 512, 128) => fifo_market_mut!(512, 512, 128, bytes),
        (512, 512, 1025) => fifo_market_mut!(512, 512, 1025, bytes),
        (512, 512, 1153) => fifo_market_mut!(512, 512, 1153, bytes),
        (1024, 1024, 128) => fifo_market_mut!(1024, 1024, 128, bytes),
        (1024, 1024, 2049) => fifo_market_mut!(1024, 1024, 2049, bytes),
        (1024, 1024, 2177) => fifo_market_mut!(1024, 1024, 2177, bytes),
        (2048, 2048, 128) => fifo_market_mut!(2048, 2048, 128, bytes),
        (2048, 2048, 4097) => fifo_market_mut!(2048, 2048, 4097, bytes),
        (2048, 2048, 4225) => fifo_market_mut!(2048, 2048, 4225, bytes),
        (4096, 4096, 128) => fifo_market_mut!(4096, 4096, 128, bytes),
        (4096, 4096, 8193) => fifo_market_mut!(4096, 4096, 8193, bytes),
        (4096, 4096, 8321) => fifo_market_mut!(4096, 4096, 8321, bytes),
        _ => {
            phoenix_log!("Invalid parameters for market");
            return Err(PhoenixError::InvalidMarketParameters.into());
        }
    };
    if !is_initial {
        assert_with_msg(
            market.get_sequence_number() > 0,
            PhoenixError::MarketUninitialized,
            "Market is not initialized",
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
        (512, 512, 128) => fifo_market!(512, 512, 128, bytes),
        (512, 512, 1025) => fifo_market!(512, 512, 1025, bytes),
        (512, 512, 1153) => fifo_market!(512, 512, 1153, bytes),
        (1024, 1024, 128) => fifo_market!(1024, 1024, 128, bytes),
        (1024, 1024, 2049) => fifo_market!(1024, 1024, 2049, bytes),
        (1024, 1024, 2177) => fifo_market!(1024, 1024, 2177, bytes),
        (2048, 2048, 128) => fifo_market!(2048, 2048, 128, bytes),
        (2048, 2048, 4097) => fifo_market!(2048, 2048, 4097, bytes),
        (2048, 2048, 4225) => fifo_market!(2048, 2048, 4225, bytes),
        (4096, 4096, 128) => fifo_market!(4096, 4096, 128, bytes),
        (4096, 4096, 8193) => fifo_market!(4096, 4096, 8193, bytes),
        (4096, 4096, 8321) => fifo_market!(4096, 4096, 8321, bytes),
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
        (512, 512, 128) => fifo_market_size!(512, 512, 128),
        (512, 512, 1025) => fifo_market_size!(512, 512, 1025),
        (512, 512, 1153) => fifo_market_size!(512, 512, 1153),
        (1024, 1024, 128) => fifo_market_size!(1024, 1024, 128),
        (1024, 1024, 2049) => fifo_market_size!(1024, 1024, 2049),
        (1024, 1024, 2177) => fifo_market_size!(1024, 1024, 2177),
        (2048, 2048, 128) => fifo_market_size!(2048, 2048, 128),
        (2048, 2048, 4097) => fifo_market_size!(2048, 2048, 4097),
        (2048, 2048, 4225) => fifo_market_size!(2048, 2048, 4225),
        (4096, 4096, 128) => fifo_market_size!(4096, 4096, 128),
        (4096, 4096, 8193) => fifo_market_size!(4096, 4096, 8193),
        (4096, 4096, 8321) => fifo_market_size!(4096, 4096, 8321),
        _ => {
            phoenix_log!("Invalid parameters for market");
            return Err(PhoenixError::InvalidMarketParameters.into());
        }
    };
    Ok(size)
}

#[test]
fn test_market_size() {
    use solana_program::rent::Rent;
    let valid_configs = [
        (512, 512, 128),
        (512, 512, 1025),
        (512, 512, 1153),
        (1024, 1024, 128),
        (1024, 1024, 2049),
        (1024, 1024, 2177),
        (2048, 2048, 128),
        (2048, 2048, 4097),
        (2048, 2048, 4225),
        (4096, 4096, 128),
        (4096, 4096, 8193),
        (4096, 4096, 8321),
    ];
    for (bids_size, asks_size, num_seats) in valid_configs.into_iter() {
        let market_size_params = MarketSizeParams {
            bids_size,
            asks_size,
            num_seats,
        };
        if let Ok(size) = get_market_size(&market_size_params) {
            println!(
                "({} {} {}) {} bytes, {} rent (SOL)",
                bids_size,
                asks_size,
                num_seats,
                size,
                Rent::default().minimum_balance(size) as f64 / 1e9
            );
        } else {
            panic!("Invalid market size params")
        }
    }
    assert!(get_market_size(&MarketSizeParams {
        bids_size: 1234,
        asks_size: 89345,
        num_seats: 2134
    })
    .is_err());
}
