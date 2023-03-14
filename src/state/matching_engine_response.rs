use crate::quantities::{BaseLots, QuoteLots};

#[repr(C)]
#[derive(Debug, Eq, PartialEq, Default, Copy, Clone)]
pub struct MatchingEngineResponse {
    pub num_quote_lots_in: QuoteLots,
    pub num_base_lots_in: BaseLots,
    pub num_quote_lots_out: QuoteLots,
    pub num_base_lots_out: BaseLots,
    pub num_quote_lots_posted: QuoteLots,
    pub num_base_lots_posted: BaseLots,
    pub num_free_quote_lots_used: QuoteLots,
    pub num_free_base_lots_used: BaseLots,
}

impl MatchingEngineResponse {
    pub fn new_from_buy(num_quote_lots_in: QuoteLots, num_base_lots_out: BaseLots) -> Self {
        MatchingEngineResponse {
            num_quote_lots_in,
            num_base_lots_in: BaseLots::ZERO,
            num_quote_lots_out: QuoteLots::ZERO,
            num_base_lots_out,
            num_quote_lots_posted: QuoteLots::ZERO,
            num_base_lots_posted: BaseLots::ZERO,
            num_free_quote_lots_used: QuoteLots::ZERO,
            num_free_base_lots_used: BaseLots::ZERO,
        }
    }

    pub fn new_from_sell(num_base_lots_in: BaseLots, num_quote_lots_out: QuoteLots) -> Self {
        MatchingEngineResponse {
            num_quote_lots_in: QuoteLots::ZERO,
            num_base_lots_in,
            num_quote_lots_out,
            num_base_lots_out: BaseLots::ZERO,
            num_quote_lots_posted: QuoteLots::ZERO,
            num_base_lots_posted: BaseLots::ZERO,
            num_free_quote_lots_used: QuoteLots::ZERO,
            num_free_base_lots_used: BaseLots::ZERO,
        }
    }

    pub fn new_withdraw(num_base_lots_out: BaseLots, num_quote_lots_out: QuoteLots) -> Self {
        MatchingEngineResponse {
            num_quote_lots_in: QuoteLots::ZERO,
            num_base_lots_in: BaseLots::ZERO,
            num_quote_lots_out,
            num_base_lots_out,
            num_quote_lots_posted: QuoteLots::ZERO,
            num_base_lots_posted: BaseLots::ZERO,
            num_free_quote_lots_used: QuoteLots::ZERO,
            num_free_base_lots_used: BaseLots::ZERO,
        }
    }

    #[inline(always)]
    pub fn post_quote_lots(&mut self, num_quote_lots: QuoteLots) {
        self.num_quote_lots_posted += num_quote_lots;
    }

    #[inline(always)]
    pub fn post_base_lots(&mut self, num_base_lots: BaseLots) {
        self.num_base_lots_posted += num_base_lots;
    }

    #[inline(always)]
    pub fn num_base_lots(&self) -> BaseLots {
        self.num_base_lots_in + self.num_base_lots_out
    }

    #[inline(always)]
    pub fn num_quote_lots(&self) -> QuoteLots {
        self.num_quote_lots_in + self.num_quote_lots_out
    }

    #[inline(always)]
    pub fn use_free_quote_lots(&mut self, num_quote_lots: QuoteLots) {
        self.num_free_quote_lots_used += num_quote_lots;
    }

    #[inline(always)]
    pub fn use_free_base_lots(&mut self, num_base_lots: BaseLots) {
        self.num_free_base_lots_used += num_base_lots;
    }

    #[inline(always)]
    pub fn get_deposit_amount_bid_in_quote_lots(&self) -> QuoteLots {
        self.num_quote_lots_in + self.num_quote_lots_posted - self.num_free_quote_lots_used
    }

    #[inline(always)]
    pub fn get_deposit_amount_ask_in_base_lots(&self) -> BaseLots {
        self.num_base_lots_in + self.num_base_lots_posted - self.num_free_base_lots_used
    }

    #[inline(always)]
    pub fn verify_no_deposit(&self) -> bool {
        self.num_base_lots_in + self.num_base_lots_posted == self.num_free_base_lots_used
            && self.num_quote_lots_in + self.num_quote_lots_posted == self.num_free_quote_lots_used
    }

    #[inline(always)]
    pub fn verify_no_withdrawal(&self) -> bool {
        self.num_base_lots_out == BaseLots::ZERO && self.num_quote_lots_out == QuoteLots::ZERO
    }
}
