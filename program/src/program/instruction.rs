use num_enum::TryFromPrimitive;
use shank::ShankInstruction;

#[repr(u8)]
#[derive(TryFromPrimitive, Debug, Copy, Clone, ShankInstruction, PartialEq, Eq)]
#[rustfmt::skip]
pub enum PhoenixInstruction {
    // Market instructions
    /// Send a swap (no limit orders allowed) order
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    Swap = 0,

    /// Send a swap (no limit orders allowed) order using only deposited funds
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    SwapWithFreeFunds = 1,

    /// Place a limit order on the book. The order can cross if the supplied order type is Limit
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    #[account(5, writable, name = "base_account", desc = "Trader base token account")]
    #[account(6, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(7, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(8, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(9, name = "token_program", desc = "Token program")]
    PlaceLimitOrder = 2,

    /// Place a limit order on the book using only deposited funds.
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    PlaceLimitOrderWithFreeFunds = 3,

    /// Reduce the size of an existing order on the book 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    ReduceOrder = 4,

    /// Reduce the size of an existing order on the book 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, writable, signer, name = "trader")]
    ReduceOrderWithFreeFunds = 5,


    /// Cancel all orders 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    CancelAllOrders = 6,

    /// Cancel all orders (no token transfers) 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    CancelAllOrdersWithFreeFunds = 7,

    /// Cancel all orders more aggressive than a specified price
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    CancelUpTo = 8,


    /// Cancel all orders more aggressive than a specified price (no token transfers) 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    CancelUpToWithFreeFunds = 9,

    /// Cancel multiple orders by ID 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    CancelMultipleOrdersById = 10,

    /// Cancel multiple orders by ID (no token transfers) 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    CancelMultipleOrdersByIdWithFreeFunds = 11,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, writable, name = "base_account", desc = "Trader base token account")]
    #[account(5, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "token_program", desc = "Token program")]
    WithdrawFunds = 12,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    #[account(5, writable, name = "base_account", desc = "Trader base token account")]
    #[account(6, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(7, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(8, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(9, name = "token_program", desc = "Token program")]
    DepositFunds = 13,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, writable, signer, name = "payer")]
    #[account(4, writable, name = "seat")]
    #[account(5, name = "system_program", desc = "System program")]
    RequestSeat = 14,

    #[account(0, signer, name = "log_authority", desc = "Log authority")]
    Log = 15,

    /// Place multiple post only orders on the book.
    /// Similar to single post only orders, these can either be set to be rejected or amended to top of book if they cross.
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    #[account(5, writable, name = "base_account", desc = "Trader base token account")]
    #[account(6, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(7, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(8, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(9, name = "token_program", desc = "Token program")]
    PlaceMultiplePostOnlyOrders = 16,
        
    /// Place multiple post only orders on the book using only deposited funds.
    /// Similar to single post only orders, these can either be set to be rejected or amended to top of book if they cross.
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "trader")]
    #[account(4, name = "seat")]
    PlaceMultiplePostOnlyOrdersWithFreeFunds = 17,


    // Admin instructions
    /// Create a market 
    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, writable, signer, name = "market_creator", desc = "The market_creator account must sign for the creation of new vaults")]
    #[account(4, name = "base_mint", desc = "Base mint account")]
    #[account(5, name = "quote_mint", desc = "Quote mint account")]
    #[account(6, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(7, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(8, name = "system_program", desc = "System program")]
    #[account(9, name = "token_program", desc = "Token program")]
    InitializeMarket = 100,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    ClaimAuthority = 101,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    NameSuccessor = 102,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    ChangeMarketStatus = 103,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    #[account(4, writable, name = "seat")]
    ChangeSeatStatus = 104,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    #[account(4, writable, signer, name = "payer")]
    #[account(5, name = "trader")]
    #[account(6, writable, name = "seat")]
    #[account(7, name = "system_program", desc = "System program")]
    RequestSeatAuthorized = 105,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    #[account(4, name = "trader")]
    #[account(5, name = "seat", desc = "The trader's PDA seat account, seeds are [b'seat', market_address, trader_address]")]
    #[account(6, writable, name = "base_account")]
    #[account(7, writable, name = "quote_account")]
    #[account(8, writable, name = "base_vault")]
    #[account(9, writable, name = "quote_vault")]
    #[account(10, name = "token_program", desc = "Token program")]
    EvictSeat = 106,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to claim authority")]
    #[account(4, name = "trader")]
    #[account(5, name = "seat", desc = "The trader's PDA seat account, seeds are [b'seat', market_address, trader_address]")]
    #[account(6, writable, name = "base_account", desc = "Trader base token account")]
    #[account(7, writable, name = "quote_account", desc = "Trader quote token account")]
    #[account(8, writable, name = "base_vault", desc = "Base vault PDA, seeds are [b'vault', market_address, base_mint_address]")]
    #[account(9, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(10, name = "token_program", desc = "Token program")]
    ForceCancelOrders = 107,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "sweeper", desc = "Signer of collect fees instruction")]
    #[account(4, writable, name = "fee_recipient", desc = "Fee collector quote token account")]
    #[account(5, writable, name = "quote_vault", desc = "Quote vault PDA, seeds are [b'vault', market_address, quote_mint_address]")]
    #[account(6, name = "token_program", desc = "Token program")]
    CollectFees = 108,

    #[account(0, name = "phoenix_program", desc = "Phoenix program")]
    #[account(1, name = "log_authority", desc = "Phoenix log authority")]
    #[account(2, writable, name = "market", desc = "This account holds the market state")]
    #[account(3, signer, name = "market_authority", desc = "The market_authority account must sign to change the free recipient")]
    #[account(4, name = "new_fee_recipient", desc = "New fee recipient")]
    ChangeFeeRecipient = 109,
}

impl PhoenixInstruction {
    pub fn to_vec(&self) -> Vec<u8> {
        vec![*self as u8]
    }
}

#[test]
fn test_instruction_serialization() {
    for i in 0..=108 {
        let instruction = match PhoenixInstruction::try_from(i) {
            Ok(j) => j,
            Err(_) => {
                assert!(i < 100);
                // This needs to be changed if new instructions are added
                assert!(i > 17);
                continue;
            }
        };
        assert_eq!(instruction as u8, i);
    }
}
