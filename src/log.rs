macro_rules! phoenix_log {
    ($message:literal, $($arg:tt)*) => {
        #[cfg(target_os = "solana")]
        solana_program::msg!($message, $($arg)*);
        #[cfg(not(target_os = "solana"))]
        println!($message, $($arg)*);
    };
    ($message:literal) => {
        #[cfg(target_os = "solana")]
        solana_program::msg!($message);
        #[cfg(not(target_os = "solana"))]
        println!($message);
    };
}
