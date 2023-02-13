use crate::quantities::{BaseLots, QuoteLots};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Zeroable, Pod)]
pub struct TraderState {
    pub quote_lots_locked: QuoteLots,
    pub quote_lots_free: QuoteLots,
    pub base_lots_locked: BaseLots,
    pub base_lots_free: BaseLots,
}

impl TraderState {
    #[inline(always)]
    pub fn unlock_quote_lots(&mut self, quote_lots: QuoteLots) {
        self.quote_lots_locked -= quote_lots;
        self.quote_lots_free += quote_lots;
    }

    #[inline(always)]
    pub fn unlock_base_lots(&mut self, base_lots: BaseLots) {
        self.base_lots_locked -= base_lots;
        self.base_lots_free += base_lots;
    }

    #[inline(always)]
    pub fn process_limit_sell(
        &mut self,
        base_lots_removed: BaseLots,
        quote_lots_received: QuoteLots,
    ) {
        self.base_lots_locked -= base_lots_removed;
        self.quote_lots_free += quote_lots_received;
    }

    #[inline(always)]
    pub fn process_limit_buy(
        &mut self,
        quote_lots_removed: QuoteLots,
        base_lots_received: BaseLots,
    ) {
        self.quote_lots_locked -= quote_lots_removed;
        self.base_lots_free += base_lots_received;
    }

    #[inline(always)]
    pub fn lock_quote_lots(&mut self, quote_lots: QuoteLots) {
        self.quote_lots_locked += quote_lots;
    }

    #[inline(always)]
    pub fn lock_base_lots(&mut self, base_lots: BaseLots) {
        self.base_lots_locked += base_lots;
    }

    #[inline(always)]
    pub fn use_free_quote_lots(&mut self, quote_lots: QuoteLots) {
        self.quote_lots_free -= quote_lots;
    }

    #[inline(always)]
    pub fn use_free_base_lots(&mut self, base_lots: BaseLots) {
        self.base_lots_free -= base_lots;
    }

    #[inline(always)]
    pub fn deposit_free_quote_lots(&mut self, quote_lots: QuoteLots) {
        self.quote_lots_free += quote_lots;
    }

    #[inline(always)]
    pub fn deposit_free_base_lots(&mut self, base_lots: BaseLots) {
        self.base_lots_free += base_lots;
    }
}
