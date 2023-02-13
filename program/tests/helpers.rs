use ellipsis_client::{EllipsisClient, EllipsisClientResult};
use solana_program::native_token::LAMPORTS_PER_SOL;
use solana_sdk::{
    account::Account,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::Signature,
    signer::{keypair::Keypair, Signer},
    system_instruction,
};
use solana_test_client::program_test::*;
use spl_token::state::Mint;
use std::str::FromStr;

pub fn sol(amount: f64) -> u64 {
    (amount * LAMPORTS_PER_SOL as f64) as u64
}

pub fn usdc(amount: f64) -> u64 {
    (amount * 1_000_000_f64) as u64
}

pub async fn get_token_account(
    client: &EllipsisClient,
    token_account: &Pubkey,
) -> EllipsisClientResult<spl_token::state::Account> {
    let account = client.get_account(token_account).await?;
    Ok(spl_token::state::Account::unpack(&account.data).unwrap())
}

pub async fn get_balance(context: &mut ProgramTestContext, pubkey: &Pubkey) -> u64 {
    context.banks_client.get_balance(*pubkey).await.unwrap()
}

pub async fn get_token_balance(context: &EllipsisClient, token_account: &Pubkey) -> u64 {
    get_token_account(context, token_account)
        .await
        .unwrap()
        .amount
}

pub async fn airdrop(
    context: &EllipsisClient,
    receiver: &Pubkey,
    amount: u64,
) -> EllipsisClientResult<Signature> {
    let ixs = vec![system_instruction::transfer(
        &context.payer.pubkey(),
        receiver,
        amount,
    )];

    context.sign_send_instructions(ixs, vec![]).await
}

pub fn clone_keypair(keypair: &Keypair) -> Keypair {
    Keypair::from_bytes(&keypair.to_bytes()).unwrap()
}

pub fn clone_pubkey(pubkey: &Pubkey) -> Pubkey {
    Pubkey::from_str(&pubkey.to_string()).unwrap()
}

pub async fn get_account(context: &mut ProgramTestContext, pubkey: &Pubkey) -> Account {
    context
        .banks_client
        .get_account(*pubkey)
        .await
        .expect("account not found")
        .expect("account empty")
}

pub async fn create_associated_token_account(
    context: &EllipsisClient,
    wallet: &Pubkey,
    token_mint: &Pubkey,
    token_program: &Pubkey,
) -> EllipsisClientResult<Pubkey> {
    let ixs = vec![
        spl_associated_token_account::instruction::create_associated_token_account(
            &context.payer.pubkey(),
            wallet,
            token_mint,
            token_program,
        ),
    ];
    context.sign_send_instructions(ixs, vec![]).await?;

    Ok(spl_associated_token_account::get_associated_token_address(
        wallet, token_mint,
    ))
}

pub async fn create_mint(
    context: &EllipsisClient,
    authority: &Pubkey,
    freeze_authority: Option<&Pubkey>,
    decimals: u8,
    mint: Option<Keypair>,
) -> EllipsisClientResult<Keypair> {
    let mint = mint.unwrap_or_else(Keypair::new);

    let ixs = vec![
        system_instruction::create_account(
            &context.payer.pubkey(),
            &mint.pubkey(),
            context.rent_exempt(Mint::LEN),
            Mint::LEN as u64,
            &spl_token::id(),
        ),
        spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint.pubkey(),
            authority,
            freeze_authority,
            decimals,
        )
        .unwrap(),
    ];

    context
        .sign_send_instructions(ixs, vec![&context.payer, &mint])
        .await
        .unwrap();
    Ok(mint)
}

pub async fn mint_tokens(
    context: &EllipsisClient,
    authority: &Keypair,
    mint: &Pubkey,
    account: &Pubkey,
    amount: u64,
    additional_signer: Option<&Keypair>,
) -> EllipsisClientResult<Signature> {
    let mut signing_keypairs = vec![&context.payer, authority];
    if let Some(signer) = additional_signer {
        signing_keypairs.push(signer);
    }

    let ix = spl_token::instruction::mint_to(
        &spl_token::id(),
        mint,
        account,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();

    context
        .sign_send_instructions(vec![ix], signing_keypairs)
        .await
}
