use crate::program::{
    dispatch_market::load_with_dispatch_mut, error::assert_with_msg, loaders::get_seat_address,
    status::SeatApprovalStatus, system_utils::create_account, AuthorizedSeatRequestContext,
    MarketHeader, ModifySeatContext, PhoenixMarketContext, RequestSeatContext, Seat,
};
use borsh::BorshDeserialize;
use sokoban::node_allocator::ZeroCopy;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey, rent::Rent, sysvar::Sysvar,
};
use std::mem::size_of;

/// This instruction is used to request a seat on the market by the market authority for a trader
pub(crate) fn process_request_seat_authorized<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    _data: &[u8],
) -> ProgramResult {
    let AuthorizedSeatRequestContext {
        payer,
        trader,
        seat,
        system_program,
    } = AuthorizedSeatRequestContext::load(market_context, accounts)?;
    _create_seat(
        payer.as_ref(),
        trader.key,
        seat.as_ref(),
        market_context.market_info.key,
        system_program.as_ref(),
    )
}

/// This instruction is used to request a seat on the market for a trader (by the trader)
pub(crate) fn process_request_seat<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    _data: &[u8],
) -> ProgramResult {
    let RequestSeatContext {
        seat,
        system_program,
        ..
    } = RequestSeatContext::load(market_context, accounts)?;
    let PhoenixMarketContext {
        market_info,
        signer: trader,
    } = market_context;
    _create_seat(
        trader.as_ref(),
        trader.key,
        seat.as_ref(),
        market_info.key,
        system_program.as_ref(),
    )
}

fn _create_seat<'a, 'info>(
    payer: &'a AccountInfo<'info>,
    trader: &'a Pubkey,
    seat: &'a AccountInfo<'info>,
    market_key: &Pubkey,
    system_program: &'a AccountInfo<'info>,
) -> ProgramResult {
    let (seat_address, bump) = get_seat_address(market_key, trader);
    assert_with_msg(
        &seat_address == seat.key,
        ProgramError::InvalidAccountData,
        "Invalid seat address",
    )?;
    let space = size_of::<Seat>();
    let seeds = vec![
        b"seat".to_vec(),
        market_key.as_ref().to_vec(),
        trader.as_ref().to_vec(),
        vec![bump],
    ];
    create_account(
        payer,
        seat,
        system_program,
        &crate::id(),
        &Rent::get()?,
        space as u64,
        seeds,
    )?;
    let mut seat_bytes = seat.try_borrow_mut_data()?;
    *Seat::load_mut_bytes(&mut seat_bytes).ok_or(ProgramError::InvalidAccountData)? =
        Seat::new_init(*market_key, *trader)?;
    Ok(())
}

/// This instruction is used to modify a seat on the market
/// The seat can be modified only by the market authority
pub(crate) fn process_change_seat_status<'a, 'info>(
    _program_id: &Pubkey,
    market_context: &PhoenixMarketContext<'a, 'info>,
    accounts: &'a [AccountInfo<'info>],
    data: &[u8],
) -> ProgramResult {
    let ModifySeatContext { seat: seat_info } = ModifySeatContext::load(market_context, accounts)?;
    let PhoenixMarketContext {
        market_info,
        signer: _,
    } = market_context;
    let new_status = SeatApprovalStatus::try_from_slice(data)?;
    let mut seat = seat_info.load_mut()?;
    let current_status = SeatApprovalStatus::from(seat.approval_status);
    if current_status == new_status {
        phoenix_log!("Seat status is unchanged");
        return Ok(());
    }
    match (current_status, new_status) {
        (SeatApprovalStatus::NotApproved, SeatApprovalStatus::Approved) => {
            seat.approval_status = SeatApprovalStatus::Approved as u64;
            // Initialize a seat for the approved trader
            let market_bytes = &mut market_info.try_borrow_mut_data()?[size_of::<MarketHeader>()..];
            let market = load_with_dispatch_mut(&market_info.size_params, market_bytes)?.inner;
            assert_with_msg(
                market.get_or_register_trader(&seat.trader).is_some(),
                ProgramError::InvalidArgument,
                "Failed to register trader",
            )?;
        }
        (SeatApprovalStatus::Approved, SeatApprovalStatus::NotApproved) => {
            seat.approval_status = SeatApprovalStatus::NotApproved as u64;
        }
        (SeatApprovalStatus::Approved, SeatApprovalStatus::Retired) => {
            seat.approval_status = SeatApprovalStatus::Retired as u64;
        }
        (SeatApprovalStatus::NotApproved, SeatApprovalStatus::Retired) => {
            seat.approval_status = SeatApprovalStatus::Retired as u64;
        }
        _ => {
            phoenix_log!(
                "Invalid seat status transition from {} to {}",
                current_status,
                new_status
            );
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    Ok(())
}
