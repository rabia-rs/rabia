//! # Banking SMR Example
//!
//! Demonstrates how to use the Banking SMR implementation with Rabia consensus.

use banking_smr::{BankingCommand, BankingData, BankingSMR};
use rabia_core::smr::StateMachine;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting Banking SMR Example");

    // Create a new Banking SMR instance
    let mut bank = BankingSMR::new();

    info!("Banking SMR created successfully");

    // Create some accounts
    info!("Creating accounts...");
    let account_commands = vec![
        BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 100000, // $1000.00
        },
        BankingCommand::CreateAccount {
            account_id: "bob".to_string(),
            initial_balance: 50000, // $500.00
        },
        BankingCommand::CreateAccount {
            account_id: "charlie".to_string(),
            initial_balance: 25000, // $250.00
        },
    ];

    for command in account_commands {
        let response = bank.apply_command(command.clone()).await;
        if response.success {
            info!("✓ Account created: {:?}", command);
        } else {
            info!("✗ Failed to create account: {:?}", response.error);
        }
    }

    // Show all accounts
    let list_response = bank.apply_command(BankingCommand::ListAccounts).await;
    if let Some(BankingData::Accounts(accounts)) = list_response.data {
        info!("Current accounts:");
        for account in accounts {
            info!(
                "  - {}: ${:.2} (created: {})",
                account.account_id,
                account.balance as f64 / 100.0,
                account.created_at
            );
        }
    }

    // Perform various banking operations
    info!("Performing banking operations...");

    let operations = vec![
        BankingCommand::Deposit {
            account_id: "alice".to_string(),
            amount: 20000, // $200.00
        },
        BankingCommand::Withdraw {
            account_id: "bob".to_string(),
            amount: 10000, // $100.00
        },
        BankingCommand::Transfer {
            from_account: "alice".to_string(),
            to_account: "charlie".to_string(),
            amount: 30000, // $300.00
        },
        BankingCommand::Transfer {
            from_account: "bob".to_string(),
            to_account: "alice".to_string(),
            amount: 15000, // $150.00
        },
    ];

    for (i, command) in operations.into_iter().enumerate() {
        let response = bank.apply_command(command.clone()).await;
        if response.success {
            info!("✓ Operation {}: {:?}", i + 1, command);
            if let Some(tx_id) = response.transaction_id {
                info!("  Transaction ID: {}", tx_id);
            }
        } else {
            info!(
                "✗ Operation {} failed: {:?} - {}",
                i + 1,
                command,
                response.error.unwrap_or_default()
            );
        }
    }

    // Check balances
    info!("Checking account balances...");
    for account_id in ["alice", "bob", "charlie"] {
        let balance_response = bank
            .apply_command(BankingCommand::GetBalance {
                account_id: account_id.to_string(),
            })
            .await;

        if let Some(BankingData::Balance(balance)) = balance_response.data {
            info!("  {}: ${:.2}", account_id, balance as f64 / 100.0);
        }
    }

    // Test error conditions
    info!("Testing error conditions...");

    // Try to transfer more than available
    let insufficient_funds = bank
        .apply_command(BankingCommand::Transfer {
            from_account: "charlie".to_string(),
            to_account: "alice".to_string(),
            amount: 1000000, // $10,000.00 (more than Charlie has)
        })
        .await;
    info!(
        "Insufficient funds test: success={}, error={:?}",
        insufficient_funds.success, insufficient_funds.error
    );

    // Try to create duplicate account
    let duplicate_account = bank
        .apply_command(BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 10000,
        })
        .await;
    info!(
        "Duplicate account test: success={}, error={:?}",
        duplicate_account.success, duplicate_account.error
    );

    // Show transaction history
    info!("Transaction history for Alice:");
    let history_response = bank
        .apply_command(BankingCommand::GetTransactionHistory {
            account_id: Some("alice".to_string()),
            limit: Some(10),
        })
        .await;

    if let Some(BankingData::Transactions(transactions)) = history_response.data {
        for (i, tx) in transactions.iter().enumerate() {
            info!(
                "  {}. {} ${:.2} ({:?}) at {}",
                i + 1,
                match &tx.transaction_type {
                    banking_smr::TransactionType::Deposit => "Deposit",
                    banking_smr::TransactionType::Withdrawal => "Withdrawal",
                    banking_smr::TransactionType::Transfer => "Transfer",
                },
                tx.amount as f64 / 100.0,
                tx.transaction_type,
                tx.timestamp
            );
        }
    }

    // Demonstrate state serialization and restoration
    info!("Testing state serialization...");
    let serialized_state = bank.serialize_state();
    info!("State serialized to {} bytes", serialized_state.len());

    // Create a new instance and restore state
    let mut new_bank = BankingSMR::new();
    new_bank.deserialize_state(&serialized_state)?;
    info!("State restored to new Banking instance");

    // Verify state was restored correctly
    info!(
        "Restored state: {} accounts, {} transactions, {} operations",
        new_bank.account_count(),
        new_bank.transaction_count(),
        new_bank.operation_count()
    );

    let total_value = new_bank.total_value();
    info!(
        "Total value across all accounts: ${:.2}",
        total_value as f64 / 100.0
    );

    // Verify specific account balance
    let alice_balance = new_bank
        .apply_command(BankingCommand::GetBalance {
            account_id: "alice".to_string(),
        })
        .await;
    if let Some(BankingData::Balance(balance)) = alice_balance.data {
        info!(
            "Alice's balance in restored state: ${:.2}",
            balance as f64 / 100.0
        );
    }

    // Test batch operations
    info!("Testing batch operations...");
    let batch_commands = vec![
        BankingCommand::CreateAccount {
            account_id: "dave".to_string(),
            initial_balance: 75000,
        },
        BankingCommand::Deposit {
            account_id: "dave".to_string(),
            amount: 25000,
        },
        BankingCommand::Transfer {
            from_account: "dave".to_string(),
            to_account: "alice".to_string(),
            amount: 50000,
        },
    ];

    let batch_responses = new_bank.apply_commands(batch_commands).await;
    info!("Batch operation results:");
    for (i, response) in batch_responses.iter().enumerate() {
        info!(
            "  Response {}: success={}, tx_id={:?}",
            i + 1,
            response.success,
            response.transaction_id
        );
    }

    // Final state summary
    let final_state = new_bank.get_state();
    info!("Final state summary:");
    info!("  - Accounts: {}", final_state.accounts.len());
    info!("  - Transactions: {}", final_state.transactions.len());
    info!("  - Operations: {}", final_state.operation_count);
    info!(
        "  - Total value: ${:.2}",
        new_bank.total_value() as f64 / 100.0
    );

    info!("Banking SMR Example completed successfully!");

    Ok(())
}
