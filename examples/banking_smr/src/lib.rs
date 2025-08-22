//! # Banking SMR Example
//!
//! A banking ledger implementation that demonstrates how to create a complex State Machine
//! Replication (SMR) application using the Rabia consensus protocol.
//!
//! This example shows a more sophisticated SMR implementation with:
//! - Account management
//! - Transfer operations with validation
//! - Transaction history
//! - Balance tracking
//! - Error handling for insufficient funds
//!
//! ## Example Usage
//!
//! ```rust
//! use rabia_banking_example::{BankingSMR, BankingCommand};
//! use rabia_core::smr::StateMachine;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut bank = BankingSMR::new();
//!     
//!     // Create accounts
//!     let create_cmd = BankingCommand::CreateAccount {
//!         account_id: "alice".to_string(),
//!         initial_balance: 1000,
//!     };
//!     let response = bank.apply_command(create_cmd).await;
//!     println!("Account created: {:?}", response);
//!     
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use rabia_core::smr::StateMachine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    /// Account identifier
    pub account_id: String,
    /// Current balance in cents (to avoid floating point precision issues)
    pub balance: i64,
    /// Account creation timestamp
    pub created_at: u64,
    /// Last transaction timestamp
    pub last_transaction_at: u64,
    /// Total number of transactions
    pub transaction_count: u64,
}

impl Account {
    pub fn new(account_id: String, initial_balance: i64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            account_id,
            balance: initial_balance,
            created_at: now,
            last_transaction_at: now,
            transaction_count: 0,
        }
    }

    pub fn update_balance(&mut self, new_balance: i64) {
        self.balance = new_balance;
        self.last_transaction_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.transaction_count += 1;
    }
}

/// Transaction record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    /// Unique transaction ID
    pub transaction_id: String,
    /// Source account (None for deposits)
    pub from_account: Option<String>,
    /// Destination account (None for withdrawals)
    pub to_account: Option<String>,
    /// Amount in cents
    pub amount: i64,
    /// Transaction timestamp
    pub timestamp: u64,
    /// Transaction type
    pub transaction_type: TransactionType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Transfer,
}

/// Commands that can be applied to the banking system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BankingCommand {
    /// Create a new account
    CreateAccount {
        account_id: String,
        initial_balance: i64,
    },
    /// Deposit money into an account
    Deposit { account_id: String, amount: i64 },
    /// Withdraw money from an account
    Withdraw { account_id: String, amount: i64 },
    /// Transfer money between accounts
    Transfer {
        from_account: String,
        to_account: String,
        amount: i64,
    },
    /// Get account balance
    GetBalance { account_id: String },
    /// Get account information
    GetAccount { account_id: String },
    /// List all accounts
    ListAccounts,
    /// Get transaction history
    GetTransactionHistory {
        account_id: Option<String>,
        limit: Option<usize>,
    },
}

/// Response from banking operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankingResponse {
    /// Whether the operation was successful
    pub success: bool,
    /// Result data (account info, balance, etc.)
    pub data: Option<BankingData>,
    /// Error message if operation failed
    pub error: Option<String>,
    /// Transaction ID for operations that create transactions
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BankingData {
    Account(Account),
    Balance(i64),
    Accounts(Vec<Account>),
    Transactions(Vec<Transaction>),
}

impl BankingResponse {
    pub fn success(data: Option<BankingData>) -> Self {
        Self {
            success: true,
            data,
            error: None,
            transaction_id: None,
        }
    }

    pub fn success_with_transaction(data: Option<BankingData>, transaction_id: String) -> Self {
        Self {
            success: true,
            data,
            error: None,
            transaction_id: Some(transaction_id),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            transaction_id: None,
        }
    }
}

/// State of the banking state machine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BankingState {
    /// All accounts indexed by account ID
    pub accounts: HashMap<String, Account>,
    /// Transaction history
    pub transactions: Vec<Transaction>,
    /// Total number of operations performed
    pub operation_count: u64,
}

/// Banking state machine implementation
#[derive(Debug, Clone)]
pub struct BankingSMR {
    state: BankingState,
}

impl BankingSMR {
    /// Create a new banking state machine
    pub fn new() -> Self {
        Self {
            state: BankingState::default(),
        }
    }

    /// Get the total number of accounts
    pub fn account_count(&self) -> usize {
        self.state.accounts.len()
    }

    /// Get the total number of transactions
    pub fn transaction_count(&self) -> usize {
        self.state.transactions.len()
    }

    /// Get the total number of operations performed
    pub fn operation_count(&self) -> u64 {
        self.state.operation_count
    }

    /// Get total value across all accounts
    pub fn total_value(&self) -> i64 {
        self.state
            .accounts
            .values()
            .map(|account| account.balance)
            .sum()
    }

    fn generate_transaction_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn validate_amount(amount: i64) -> Result<(), String> {
        if amount <= 0 {
            return Err("Amount must be positive".to_string());
        }
        if amount > 1_000_000_000 {
            // Max $10M per transaction
            return Err("Amount exceeds maximum limit".to_string());
        }
        Ok(())
    }

    fn validate_account_id(account_id: &str) -> Result<(), String> {
        if account_id.is_empty() {
            return Err("Account ID cannot be empty".to_string());
        }
        if account_id.len() > 50 {
            return Err("Account ID too long".to_string());
        }
        Ok(())
    }
}

impl Default for BankingSMR {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StateMachine for BankingSMR {
    type Command = BankingCommand;
    type Response = BankingResponse;
    type State = BankingState;

    async fn apply_command(&mut self, command: Self::Command) -> Self::Response {
        self.state.operation_count += 1;

        match command {
            BankingCommand::CreateAccount {
                account_id,
                initial_balance,
            } => {
                if let Err(e) = Self::validate_account_id(&account_id) {
                    return BankingResponse::error(e);
                }

                if initial_balance < 0 {
                    return BankingResponse::error(
                        "Initial balance cannot be negative".to_string(),
                    );
                }

                if self.state.accounts.contains_key(&account_id) {
                    return BankingResponse::error("Account already exists".to_string());
                }

                let account = Account::new(account_id.clone(), initial_balance);
                self.state.accounts.insert(account_id, account.clone());

                BankingResponse::success(Some(BankingData::Account(account)))
            }

            BankingCommand::Deposit { account_id, amount } => {
                if let Err(e) = Self::validate_amount(amount) {
                    return BankingResponse::error(e);
                }

                let account = match self.state.accounts.get_mut(&account_id) {
                    Some(account) => account,
                    None => return BankingResponse::error("Account not found".to_string()),
                };

                match account.balance.checked_add(amount) {
                    Some(new_balance) => {
                        account.update_balance(new_balance);

                        let transaction_id = Self::generate_transaction_id();
                        let transaction = Transaction {
                            transaction_id: transaction_id.clone(),
                            from_account: None,
                            to_account: Some(account_id),
                            amount,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64,
                            transaction_type: TransactionType::Deposit,
                        };
                        self.state.transactions.push(transaction);

                        BankingResponse::success_with_transaction(
                            Some(BankingData::Balance(new_balance)),
                            transaction_id,
                        )
                    }
                    None => BankingResponse::error("Deposit would cause overflow".to_string()),
                }
            }

            BankingCommand::Withdraw { account_id, amount } => {
                if let Err(e) = Self::validate_amount(amount) {
                    return BankingResponse::error(e);
                }

                let account = match self.state.accounts.get_mut(&account_id) {
                    Some(account) => account,
                    None => return BankingResponse::error("Account not found".to_string()),
                };

                if account.balance < amount {
                    return BankingResponse::error("Insufficient funds".to_string());
                }

                let new_balance = account.balance - amount;
                account.update_balance(new_balance);

                let transaction_id = Self::generate_transaction_id();
                let transaction = Transaction {
                    transaction_id: transaction_id.clone(),
                    from_account: Some(account_id),
                    to_account: None,
                    amount,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    transaction_type: TransactionType::Withdrawal,
                };
                self.state.transactions.push(transaction);

                BankingResponse::success_with_transaction(
                    Some(BankingData::Balance(new_balance)),
                    transaction_id,
                )
            }

            BankingCommand::Transfer {
                from_account,
                to_account,
                amount,
            } => {
                if let Err(e) = Self::validate_amount(amount) {
                    return BankingResponse::error(e);
                }

                if from_account == to_account {
                    return BankingResponse::error("Cannot transfer to same account".to_string());
                }

                // Check if both accounts exist and get their current balances
                let from_balance = match self.state.accounts.get(&from_account) {
                    Some(account) => account.balance,
                    None => return BankingResponse::error("Source account not found".to_string()),
                };

                if !self.state.accounts.contains_key(&to_account) {
                    return BankingResponse::error("Destination account not found".to_string());
                }

                if from_balance < amount {
                    return BankingResponse::error("Insufficient funds".to_string());
                }

                // Perform the transfer
                let from_account_ref = self.state.accounts.get_mut(&from_account).unwrap();
                from_account_ref.update_balance(from_balance - amount);

                let to_account_ref = self.state.accounts.get_mut(&to_account).unwrap();
                let to_new_balance = to_account_ref.balance + amount;
                to_account_ref.update_balance(to_new_balance);

                let transaction_id = Self::generate_transaction_id();
                let transaction = Transaction {
                    transaction_id: transaction_id.clone(),
                    from_account: Some(from_account),
                    to_account: Some(to_account),
                    amount,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    transaction_type: TransactionType::Transfer,
                };
                self.state.transactions.push(transaction);

                BankingResponse::success_with_transaction(None, transaction_id)
            }

            BankingCommand::GetBalance { account_id } => {
                match self.state.accounts.get(&account_id) {
                    Some(account) => {
                        BankingResponse::success(Some(BankingData::Balance(account.balance)))
                    }
                    None => BankingResponse::error("Account not found".to_string()),
                }
            }

            BankingCommand::GetAccount { account_id } => {
                match self.state.accounts.get(&account_id) {
                    Some(account) => {
                        BankingResponse::success(Some(BankingData::Account(account.clone())))
                    }
                    None => BankingResponse::error("Account not found".to_string()),
                }
            }

            BankingCommand::ListAccounts => {
                let accounts: Vec<Account> = self.state.accounts.values().cloned().collect();
                BankingResponse::success(Some(BankingData::Accounts(accounts)))
            }

            BankingCommand::GetTransactionHistory { account_id, limit } => {
                let mut transactions: Vec<Transaction> = if let Some(account_id) = account_id {
                    self.state
                        .transactions
                        .iter()
                        .filter(|tx| {
                            tx.from_account.as_ref() == Some(&account_id)
                                || tx.to_account.as_ref() == Some(&account_id)
                        })
                        .cloned()
                        .collect()
                } else {
                    self.state.transactions.clone()
                };

                // Sort by timestamp (newest first)
                transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                // Apply limit if specified
                if let Some(limit) = limit {
                    transactions.truncate(limit);
                }

                BankingResponse::success(Some(BankingData::Transactions(transactions)))
            }
        }
    }

    fn get_state(&self) -> Self::State {
        self.state.clone()
    }

    fn set_state(&mut self, state: Self::State) {
        self.state = state;
    }

    fn serialize_state(&self) -> Vec<u8> {
        bincode::serialize(&self.state).unwrap_or_default()
    }

    fn deserialize_state(&mut self, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.state = bincode::deserialize(data)?;
        Ok(())
    }

    async fn apply_commands(&mut self, commands: Vec<Self::Command>) -> Vec<Self::Response> {
        let mut responses = Vec::with_capacity(commands.len());
        for command in commands {
            responses.push(self.apply_command(command).await);
        }
        responses
    }

    fn is_deterministic(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_banking_account_creation() {
        let mut bank = BankingSMR::new();

        let response = bank
            .apply_command(BankingCommand::CreateAccount {
                account_id: "alice".to_string(),
                initial_balance: 1000,
            })
            .await;

        assert!(response.success);
        assert_eq!(bank.account_count(), 1);

        // Test duplicate account creation
        let response = bank
            .apply_command(BankingCommand::CreateAccount {
                account_id: "alice".to_string(),
                initial_balance: 500,
            })
            .await;

        assert!(!response.success);
        assert!(response.error.as_ref().unwrap().contains("already exists"));
    }

    #[tokio::test]
    async fn test_banking_deposit_withdraw() {
        let mut bank = BankingSMR::new();

        // Create account
        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 1000,
        })
        .await;

        // Test deposit
        let response = bank
            .apply_command(BankingCommand::Deposit {
                account_id: "alice".to_string(),
                amount: 500,
            })
            .await;

        assert!(response.success);
        assert!(response.transaction_id.is_some());

        // Check balance
        let response = bank
            .apply_command(BankingCommand::GetBalance {
                account_id: "alice".to_string(),
            })
            .await;

        if let Some(BankingData::Balance(balance)) = response.data {
            assert_eq!(balance, 1500);
        } else {
            panic!("Expected balance data");
        }

        // Test withdrawal
        let response = bank
            .apply_command(BankingCommand::Withdraw {
                account_id: "alice".to_string(),
                amount: 200,
            })
            .await;

        assert!(response.success);

        // Test insufficient funds
        let response = bank
            .apply_command(BankingCommand::Withdraw {
                account_id: "alice".to_string(),
                amount: 2000,
            })
            .await;

        assert!(!response.success);
        assert!(response
            .error
            .as_ref()
            .unwrap()
            .contains("Insufficient funds"));
    }

    #[tokio::test]
    async fn test_banking_transfer() {
        let mut bank = BankingSMR::new();

        // Create accounts
        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 1000,
        })
        .await;

        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "bob".to_string(),
            initial_balance: 500,
        })
        .await;

        // Test successful transfer
        let response = bank
            .apply_command(BankingCommand::Transfer {
                from_account: "alice".to_string(),
                to_account: "bob".to_string(),
                amount: 300,
            })
            .await;

        assert!(response.success);
        assert!(response.transaction_id.is_some());

        // Check balances
        let alice_response = bank
            .apply_command(BankingCommand::GetBalance {
                account_id: "alice".to_string(),
            })
            .await;
        let bob_response = bank
            .apply_command(BankingCommand::GetBalance {
                account_id: "bob".to_string(),
            })
            .await;

        if let Some(BankingData::Balance(alice_balance)) = alice_response.data {
            assert_eq!(alice_balance, 700);
        }

        if let Some(BankingData::Balance(bob_balance)) = bob_response.data {
            assert_eq!(bob_balance, 800);
        }

        // Test insufficient funds transfer
        let response = bank
            .apply_command(BankingCommand::Transfer {
                from_account: "alice".to_string(),
                to_account: "bob".to_string(),
                amount: 1000,
            })
            .await;

        assert!(!response.success);
        assert!(response
            .error
            .as_ref()
            .unwrap()
            .contains("Insufficient funds"));
    }

    #[tokio::test]
    async fn test_banking_state_serialization() {
        let mut bank = BankingSMR::new();

        // Create accounts and perform operations
        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 1000,
        })
        .await;

        bank.apply_command(BankingCommand::Deposit {
            account_id: "alice".to_string(),
            amount: 500,
        })
        .await;

        // Serialize state
        let serialized = bank.serialize_state();
        assert!(!serialized.is_empty());

        // Create new bank and deserialize
        let mut new_bank = BankingSMR::new();
        new_bank.deserialize_state(&serialized).unwrap();

        // Verify state was restored
        assert_eq!(new_bank.account_count(), 1);
        assert_eq!(new_bank.transaction_count(), 1);
        assert_eq!(new_bank.operation_count(), bank.operation_count());

        let response = new_bank
            .apply_command(BankingCommand::GetBalance {
                account_id: "alice".to_string(),
            })
            .await;

        if let Some(BankingData::Balance(balance)) = response.data {
            assert_eq!(balance, 1500);
        }
    }

    #[tokio::test]
    async fn test_banking_transaction_history() {
        let mut bank = BankingSMR::new();

        // Create accounts
        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "alice".to_string(),
            initial_balance: 1000,
        })
        .await;

        bank.apply_command(BankingCommand::CreateAccount {
            account_id: "bob".to_string(),
            initial_balance: 500,
        })
        .await;

        // Perform several transactions
        bank.apply_command(BankingCommand::Deposit {
            account_id: "alice".to_string(),
            amount: 200,
        })
        .await;

        bank.apply_command(BankingCommand::Transfer {
            from_account: "alice".to_string(),
            to_account: "bob".to_string(),
            amount: 300,
        })
        .await;

        // Get all transaction history
        let response = bank
            .apply_command(BankingCommand::GetTransactionHistory {
                account_id: None,
                limit: None,
            })
            .await;

        if let Some(BankingData::Transactions(transactions)) = response.data {
            assert_eq!(transactions.len(), 2);
        }

        // Get Alice's transaction history
        let response = bank
            .apply_command(BankingCommand::GetTransactionHistory {
                account_id: Some("alice".to_string()),
                limit: None,
            })
            .await;

        if let Some(BankingData::Transactions(transactions)) = response.data {
            assert_eq!(transactions.len(), 2); // Deposit + Transfer
        }
    }
}
