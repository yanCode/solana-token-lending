#![allow(clippy::arithmetic_side_effects)]
#![cfg(feature = "test-sbf")]

mod helpers;
use {
    helpers::{
        add_lending_market, add_reserve, add_sol_oracle, add_usdc_mint, add_usdc_oracle,
        AddReserveArgs, FRACTIONAL_TO_USDC, LAMPORTS_TO_SOL, TEST_RESERVE_CONFIG,
    },
    solana_program_test::*,
    solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction},
    spl_token_lending::{
        instruction::builder::refresh_reserve,
        math::{Decimal, Rate, TryAdd, TryDiv, TryMul},
        processor::process_instruction,
        state::SLOTS_PER_YEAR,
    },
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    const SOL_RESERVE_LIQUIDITY_LAMPORTS: u64 = 100 * LAMPORTS_TO_SOL;
    const USDC_RESERVE_LIQUIDITY_FRACTIONAL: u64 = 100 * FRACTIONAL_TO_USDC;
    const BORROW_AMOUNT: u64 = 100;
    let user_accounts_owner = Keypair::new();
    let lending_market = add_lending_market(&mut test);
    let mut reserve_config = TEST_RESERVE_CONFIG;
    reserve_config.loan_to_value_ratio = 80;

    // Configure reserve to a fixed borrow rate of 1%
    const BORROW_RATE: u8 = 1;
    reserve_config.min_borrow_rate = BORROW_RATE;
    reserve_config.optimal_borrow_rate = BORROW_RATE;
    reserve_config.optimal_utilization_rate = 100;

    let usdc_mint = add_usdc_mint(&mut test);
    let usdc_oracle = add_usdc_oracle(&mut test);
    let usdc_test_reserve = add_reserve(
        &mut test,
        &lending_market,
        &usdc_oracle,
        &user_accounts_owner,
        AddReserveArgs {
            borrow_amount: BORROW_AMOUNT,
            liquidity_amount: USDC_RESERVE_LIQUIDITY_FRACTIONAL,
            liquidity_mint_decimals: usdc_mint.decimals,
            liquidity_mint_pubkey: usdc_mint.pubkey,
            config: reserve_config,
            slots_elapsed: 1, // elapsed from 1; clock.slot = 2
            ..AddReserveArgs::default()
        },
    );
    let sol_oracle = add_sol_oracle(&mut test);
    let sol_test_reserve = add_reserve(
        &mut test,
        &lending_market,
        &sol_oracle,
        &user_accounts_owner,
        AddReserveArgs {
            borrow_amount: BORROW_AMOUNT,
            liquidity_amount: SOL_RESERVE_LIQUIDITY_LAMPORTS,
            liquidity_mint_decimals: 9,
            liquidity_mint_pubkey: spl_token::native_mint::id(),
            config: reserve_config,
            slots_elapsed: 1, // elapsed from 1; clock.slot = 2
            ..AddReserveArgs::default()
        },
    );
    let mut test_context = test.start_with_context().await;
    test_context.warp_to_slot(3).unwrap(); // clock.slot = 3
    let ProgramTestContext {
        banks_client,
        payer,
        last_blockhash: recent_blockhash,
        ..
    } = test_context;
    let mut transaction = Transaction::new_with_payer(
        &[
            refresh_reserve(
                spl_token_lending::id(),
                usdc_test_reserve.pubkey,
                usdc_oracle.price_pubkey,
            ),
            refresh_reserve(
                spl_token_lending::id(),
                sol_test_reserve.pubkey,
                sol_oracle.price_pubkey,
            ),
        ],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);
    assert!(banks_client.process_transaction(transaction).await.is_ok());
    let sol_reserve = sol_test_reserve.get_state(&banks_client).await;
    let usdc_reserve = usdc_test_reserve.get_state(&banks_client).await;
    let slot_rate = Rate::from_percent(BORROW_RATE)
        .try_div(SLOTS_PER_YEAR)
        .unwrap();
    let compound_rate = Rate::one().try_add(slot_rate).unwrap();
    let compound_borrow = Decimal::from(BORROW_AMOUNT).try_mul(compound_rate).unwrap();
    assert_eq!(
        sol_reserve.liquidity.cumulative_borrow_rate_wads,
        compound_rate.into()
    );
    assert_eq!(
        sol_reserve.liquidity.cumulative_borrow_rate_wads,
        usdc_reserve.liquidity.cumulative_borrow_rate_wads
    );
    assert_eq!(sol_reserve.liquidity.borrowed_amount_wads, compound_borrow);
    assert_eq!(
        sol_reserve.liquidity.borrowed_amount_wads,
        usdc_reserve.liquidity.borrowed_amount_wads
    );
    assert_eq!(
        sol_reserve.liquidity.market_price,
        sol_test_reserve.market_price
    );
    assert_eq!(
        usdc_reserve.liquidity.market_price,
        usdc_test_reserve.market_price
    );
}
