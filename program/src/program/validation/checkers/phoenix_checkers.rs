use crate::program::{
    error::assert_with_msg,
    get_discriminant, get_seat_address,
    status::{MarketStatus, SeatApprovalStatus},
    MarketHeader, MarketSizeParams, PhoenixError, Seat,
};
use sokoban::node_allocator::ZeroCopy;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};
use std::{
    cell::{Ref, RefMut},
    mem::size_of,
    ops::{Deref, DerefMut},
};

#[derive(Clone)]
pub struct MarketAccountInfo<'a, 'info> {
    pub info: &'a AccountInfo<'info>,
    pub size_params: MarketSizeParams,
}

impl<'a, 'info> MarketAccountInfo<'a, 'info> {
    #[inline(always)]
    fn _new_unchecked(
        info: &'a AccountInfo<'info>,
    ) -> Result<MarketAccountInfo<'a, 'info>, ProgramError> {
        assert_with_msg(
            info.owner == &crate::ID,
            ProgramError::IllegalOwner,
            "Market must be owned by the Phoenix program",
        )?;
        Ok(Self {
            info,
            size_params: MarketSizeParams::default(),
        })
    }

    pub fn new(info: &'a AccountInfo<'info>) -> Result<MarketAccountInfo<'a, 'info>, ProgramError> {
        let mut market_info = Self::_new_unchecked(info)?;
        let header = market_info.get_header()?;
        assert_with_msg(
            header.discriminant == get_discriminant::<MarketHeader>()?,
            ProgramError::InvalidAccountData,
            "Invalid market discriminant",
        )?;
        let params = header.market_size_params;
        drop(header);
        market_info.size_params = params;
        Ok(market_info)
    }

    pub fn assert_reduce_allowed(&self) -> ProgramResult {
        let header = self.get_header()?;
        let status = MarketStatus::from(header.status);
        assert_with_msg(
            status.reduce_allowed(),
            ProgramError::InvalidAccountData,
            &format!("Reduce order is not allowed, market status is {}", status),
        )
    }

    pub fn assert_cross_allowed(&self) -> ProgramResult {
        let header = self.get_header()?;
        let status = MarketStatus::from(header.status);
        assert_with_msg(
            status.cross_allowed(),
            ProgramError::InvalidAccountData,
            &format!(
                "FOK and IOC orders are not allowed, market status is {}",
                status
            ),
        )
    }

    pub fn assert_post_allowed(&self) -> ProgramResult {
        let header = self.get_header()?;
        let status = MarketStatus::from(header.status);
        assert_with_msg(
            status.post_allowed(),
            ProgramError::InvalidAccountData,
            &format!(
                "Post only order is not allowed, market status is {}",
                status
            ),
        )
    }

    pub fn assert_valid_authority(&self, authority: &Pubkey) -> ProgramResult {
        let header = self.get_header()?;
        assert_with_msg(
            &header.authority == authority,
            PhoenixError::InvalidMarketAuthority,
            "Invalid market authority",
        )
    }

    pub fn assert_valid_successor(&self, successor: &Pubkey) -> ProgramResult {
        let header = self.get_header()?;
        assert_with_msg(
            &header.successor == successor,
            PhoenixError::InvalidMarketAuthority,
            "Invalid market successor",
        )
    }

    pub fn new_init(
        info: &'a AccountInfo<'info>,
    ) -> Result<MarketAccountInfo<'a, 'info>, ProgramError> {
        let market_bytes = info.try_borrow_data()?;
        let (header_bytes, _) = market_bytes.split_at(size_of::<MarketHeader>());
        let header =
            MarketHeader::load_bytes(header_bytes).ok_or(ProgramError::InvalidAccountData)?;
        assert_with_msg(
            info.owner == &crate::ID,
            ProgramError::IllegalOwner,
            "Market must be owned by the Phoenix program",
        )?;
        // On initialization, the discriminant is not set yet.
        assert_with_msg(
            header.discriminant == 0,
            ProgramError::InvalidAccountData,
            "Expected uninitialized market with discriminant 0",
        )?;
        assert_with_msg(
            header.status == MarketStatus::Uninitialized as u64,
            ProgramError::InvalidAccountData,
            "MarketStatus must be uninitialized",
        )?;
        Ok(Self {
            info,
            size_params: MarketSizeParams::default(),
        })
    }

    pub fn get_header(&self) -> Result<Ref<'_, MarketHeader>, ProgramError> {
        let data = self.info.try_borrow_data()?;
        Ok(Ref::map(data, |data| {
            return MarketHeader::load_bytes(&data[..size_of::<MarketHeader>()]).unwrap();
        }))
    }

    pub fn get_header_mut(&self) -> Result<RefMut<'_, MarketHeader>, ProgramError> {
        let data = self.info.try_borrow_mut_data()?;
        Ok(RefMut::map(data, |data| {
            return MarketHeader::load_mut_bytes(&mut data[..size_of::<MarketHeader>()]).unwrap();
        }))
    }
}

impl<'a, 'info> AsRef<AccountInfo<'info>> for MarketAccountInfo<'a, 'info> {
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

impl<'a, 'info> Deref for MarketAccountInfo<'a, 'info> {
    type Target = AccountInfo<'info>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}

#[derive(Clone)]
pub struct SeatAccountInfo<'a, 'info> {
    pub info: &'a AccountInfo<'info>,
}

impl<'a, 'info> SeatAccountInfo<'a, 'info> {
    pub fn new_with_context(
        info: &'a AccountInfo<'info>,
        market: &Pubkey,
        trader: &Pubkey,
        approved: bool,
    ) -> Result<SeatAccountInfo<'a, 'info>, ProgramError> {
        let (seat_address, _) = get_seat_address(market, trader);
        assert_with_msg(
            info.owner == &crate::ID,
            ProgramError::IllegalOwner,
            "Seat must be owned by the Phoenix program",
        )?;
        assert_with_msg(
            &seat_address == info.key,
            ProgramError::InvalidInstructionData,
            "Invalid address for seat",
        )?;
        let seat_bytes = info.try_borrow_data()?;
        let seat = Seat::load_bytes(&seat_bytes).ok_or(ProgramError::InvalidAccountData)?;
        assert_with_msg(
            seat.discriminant == get_discriminant::<Seat>()?,
            ProgramError::InvalidAccountData,
            "Invalid discriminant for seat",
        )?;
        assert_with_msg(
            &seat.trader == trader,
            ProgramError::InvalidAccountData,
            "Invalid trader for seat",
        )?;
        assert_with_msg(
            &seat.market == market,
            ProgramError::InvalidAccountData,
            "Invalid market for seat",
        )?;
        let seat_status = SeatApprovalStatus::from(seat.approval_status);
        if approved {
            assert_with_msg(
                matches!(seat_status, SeatApprovalStatus::Approved),
                PhoenixError::InvalidSeatStatus,
                "Seat must be approved",
            )?;
        } else {
            assert_with_msg(
                !matches!(seat_status, SeatApprovalStatus::Approved),
                PhoenixError::InvalidSeatStatus,
                "Seat must be unapproved or retired",
            )?;
        }
        Ok(Self { info })
    }

    pub fn new(info: &'a AccountInfo<'info>) -> Result<SeatAccountInfo<'a, 'info>, ProgramError> {
        let seat_bytes = info.try_borrow_data()?;
        let seat = Seat::load_bytes(&seat_bytes).ok_or(ProgramError::InvalidAccountData)?;
        let (seat_address, _) = get_seat_address(&seat.market, &seat.trader);
        assert_with_msg(
            info.owner == &crate::ID,
            ProgramError::IllegalOwner,
            "Seat must be owned by the Phoenix program",
        )?;
        assert_with_msg(
            &seat_address == info.key,
            ProgramError::InvalidInstructionData,
            "Invalid address for seat",
        )?;
        assert_with_msg(
            seat.discriminant == get_discriminant::<Seat>()?,
            ProgramError::InvalidAccountData,
            "Invalid discriminant for seat",
        )?;
        Ok(Self { info })
    }

    // TODO factor this away into a generic trait
    pub fn load(&self) -> Result<Ref<'_, Seat>, ProgramError> {
        let data = self.info.try_borrow_data()?;
        Ok(Ref::map(data, |data| {
            return Seat::load_bytes(data).unwrap();
        }))
    }

    pub fn load_mut(&self) -> Result<RefMut<'_, Seat>, ProgramError> {
        let data = self.info.try_borrow_mut_data()?;
        Ok(RefMut::map(data, |data| {
            return Seat::load_mut_bytes(&mut data.deref_mut()[..]).unwrap();
        }))
    }
}

impl<'a, 'info> AsRef<AccountInfo<'info>> for SeatAccountInfo<'a, 'info> {
    fn as_ref(&self) -> &AccountInfo<'info> {
        self.info
    }
}

impl<'a, 'info> Deref for SeatAccountInfo<'a, 'info> {
    type Target = AccountInfo<'info>;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}
