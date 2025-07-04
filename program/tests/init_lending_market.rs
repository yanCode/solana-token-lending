#![cfg(feature = "test-sbf")]
mod helpers;
use {
    helpers::{add_lending_market, TestLendingMarket},
    solana_program_test::*,
    solana_sdk::{
        instruction::InstructionError,
        signature::Signer,
        transaction::{Transaction, TransactionError},
    },
    spl_token_lending::{
        error::LendingError, instruction::builder::init_lending_market,
        processor::process_instruction,
    },
};

#[tokio::test]
async fn test_success() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    test.set_compute_max_units(20_000);
    let (banks_client, payer, _recent_blockhash) = test.start().await;
    let test_lending_market = TestLendingMarket::init(&banks_client, &payer, None).await;
    test_lending_market.validate_state(&banks_client).await;
}

#[tokio::test]
async fn test_already_initialized() {
    let mut test = ProgramTest::new(
        "spl_token_lending",
        spl_token_lending::id(),
        processor!(process_instruction),
    );
    let existing_market = add_lending_market(&mut test);
    let (banks_client, payer, recent_blockhash) = test.start().await;
    let mut transaction = Transaction::new_with_payer(
        &[init_lending_market(
            spl_token_lending::id(),
            existing_market.owner.pubkey(),
            existing_market.quote_currency,
            existing_market.pubkey,
            existing_market.oracle_program_id,
        )],
        Some(&payer.pubkey()),
    );
    transaction.sign(&[&payer], recent_blockhash);

    assert_eq!(
        banks_client
            .process_transaction(transaction)
            .await
            .unwrap_err()
            .unwrap(),
        TransactionError::InstructionError(
            0,
            InstructionError::Custom(LendingError::AlreadyInitialized as u32)
        )
    );
}
