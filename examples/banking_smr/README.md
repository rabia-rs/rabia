# Banking SMR Example

This example demonstrates how to build a sophisticated financial ledger using State Machine Replication (SMR) with the Rabia consensus protocol. It showcases complex business logic, validation, and transaction management in a distributed system.

## What This Example Shows

The Banking SMR demonstrates advanced SMR concepts for real-world applications:

1. **Complex Business Logic**: Account management with validation and business rules
2. **Multi-Entity State**: Managing accounts, transactions, and relationships
3. **Atomic Operations**: Transfer operations that must succeed or fail completely
4. **Validation and Error Handling**: Comprehensive input validation and business rule enforcement
5. **Audit Trail**: Complete transaction history for compliance and debugging
6. **Consistent State**: Financial invariants maintained across all replicas

## State Machine Implementation

The banking system implements these operations:

### Account Management
- `CreateAccount { account_id, initial_balance }` - Create new account with validation
- `GetAccount { account_id }` - Get complete account information
- `ListAccounts` - Get all accounts (admin operation)

### Financial Operations  
- `Deposit { account_id, amount }` - Add funds to an account
- `Withdraw { account_id, amount }` - Remove funds with balance checking
- `Transfer { from_account, to_account, amount }` - Atomic transfer between accounts
- `GetBalance { account_id }` - Get current account balance

### Reporting and Audit
- `GetTransactionHistory { account_id, limit }` - Get transaction history with filtering

## Key SMR Features Demonstrated

### Complex State Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankingState {
    /// All accounts indexed by account ID
    pub accounts: HashMap<String, Account>,
    /// Complete transaction history for audit trail
    pub transactions: Vec<Transaction>,
    /// Operation counter for metrics
    pub operation_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub account_id: String,
    pub balance: i64,  // Using cents to avoid floating point issues
    pub created_at: u64,
    pub last_transaction_at: u64,
    pub transaction_count: u64,
}
```

### Business Logic Validation
```rust
async fn apply_command(&mut self, command: Self::Command) -> Self::Response {
    self.state.operation_count += 1;
    
    match command {
        BankingCommand::Transfer { from_account, to_account, amount } => {
            // Validate transfer amount
            if let Err(e) = Self::validate_amount(amount) {
                return BankingResponse::error(e);
            }
            
            // Prevent self-transfers
            if from_account == to_account {
                return BankingResponse::error("Cannot transfer to same account".to_string());
            }
            
            // Verify both accounts exist
            let from_balance = match self.state.accounts.get(&from_account) {
                Some(account) => account.balance,
                None => return BankingResponse::error("Source account not found".to_string()),
            };
            
            if !self.state.accounts.contains_key(&to_account) {
                return BankingResponse::error("Destination account not found".to_string());
            }
            
            // Check sufficient funds
            if from_balance < amount {
                return BankingResponse::error("Insufficient funds".to_string());
            }
            
            // Atomically update both accounts
            self.state.accounts.get_mut(&from_account).unwrap()
                .update_balance(from_balance - amount);
            self.state.accounts.get_mut(&to_account).unwrap()
                .update_balance(self.state.accounts[&to_account].balance + amount);
            
            // Record transaction for audit trail
            let transaction = Transaction {
                transaction_id: Self::generate_transaction_id(),
                from_account: Some(from_account),
                to_account: Some(to_account),
                amount,
                timestamp: current_timestamp(),
                transaction_type: TransactionType::Transfer,
            };
            self.state.transactions.push(transaction);
            
            BankingResponse::success_with_transaction(None, transaction_id)
        }
        // ... other operations
    }
}
```

### Audit Trail and Compliance
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    pub transaction_id: String,       // Unique identifier  
    pub from_account: Option<String>, // None for deposits
    pub to_account: Option<String>,   // None for withdrawals
    pub amount: i64,                  // Amount in cents
    pub timestamp: u64,               // UTC timestamp
    pub transaction_type: TransactionType,
}

// Every financial operation creates an immutable audit record
BankingCommand::GetTransactionHistory { account_id, limit } => {
    let mut transactions: Vec<Transaction> = if let Some(account_id) = account_id {
        // Filter transactions for specific account
        self.state.transactions.iter()
            .filter(|tx| tx.involves_account(&account_id))
            .cloned().collect()
    } else {
        // Return all transactions
        self.state.transactions.clone()
    };
    
    // Sort by timestamp (newest first) for consistent ordering
    transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    // Apply limit for pagination
    if let Some(limit) = limit {
        transactions.truncate(limit);
    }
    
    BankingResponse::success(Some(BankingData::Transactions(transactions)))
}
```

### Financial Invariants
The banking SMR maintains critical financial invariants:

1. **Conservation of Money**: Total balance across all accounts never changes unless money is deposited or withdrawn
2. **Non-negative Balances**: Accounts cannot have negative balances (no overdrafts)
3. **Atomic Transfers**: Transfers either complete fully or fail completely
4. **Audit Trail**: Every balance change is recorded in the transaction history

## Running the Example

```bash
# Run the banking SMR example
cargo run --bin banking_smr_example

# Run with multiple replicas to see consensus
cargo run --bin banking_smr_cluster  

# Run comprehensive tests
cargo test -p banking_smr

# Run stress tests with concurrent operations
cargo test -p banking_smr -- --test-threads=1 test_concurrent_operations
```

## Use Cases

This pattern is ideal for:

- **Financial Systems**: Banks, payment processors, digital wallets
- **Trading Platforms**: Order books, settlement systems, clearing houses  
- **Accounting Systems**: General ledgers, bookkeeping, financial reporting
- **Resource Management**: Credits, quotas, resource allocation
- **Gaming Economies**: Virtual currencies, item trading, player balances
- **Supply Chain**: Inventory tracking, asset transfers, ownership records

## Advanced Features

### Concurrent Operations with Consistency
```rust
// All replicas process these operations in the same order
let batch_operations = vec![
    BankingCommand::Transfer { 
        from_account: "alice".to_string(), 
        to_account: "bob".to_string(), 
        amount: 100 
    },
    BankingCommand::Transfer { 
        from_account: "bob".to_string(), 
        to_account: "charlie".to_string(), 
        amount: 50 
    },
    BankingCommand::Deposit { 
        account_id: "alice".to_string(), 
        amount: 200 
    },
];

// All operations applied atomically in order across all replicas
let results = banking_smr.apply_commands(batch_operations).await;
```

### Financial Reporting and Analytics
```rust
// The banking state provides rich analytics
impl BankingSMR {
    pub fn total_value(&self) -> i64 {
        self.state.accounts.values().map(|a| a.balance).sum()
    }
    
    pub fn account_count(&self) -> usize {
        self.state.accounts.len()
    }
    
    pub fn transaction_volume(&self) -> i64 {
        self.state.transactions.iter().map(|t| t.amount).sum()
    }
    
    pub fn most_active_accounts(&self) -> Vec<(String, u64)> {
        let mut accounts: Vec<_> = self.state.accounts.iter()
            .map(|(id, account)| (id.clone(), account.transaction_count))
            .collect();
        accounts.sort_by(|a, b| b.1.cmp(&a.1));
        accounts
    }
}
```

### Compliance and Monitoring
```rust
// Transaction monitoring for compliance
impl Transaction {
    pub fn is_large_transaction(&self) -> bool {
        self.amount > 1_000_000 // $10,000+ requires reporting
    }
    
    pub fn involves_account(&self, account_id: &str) -> bool {
        self.from_account.as_ref() == Some(account_id) || 
        self.to_account.as_ref() == Some(account_id)
    }
    
    pub fn transaction_age_days(&self) -> u64 {
        let now = current_timestamp();
        (now - self.timestamp) / (24 * 60 * 60 * 1000)
    }
}
```

## Performance Considerations

### Memory Management
- **Account Storage**: Efficient HashMap storage for fast account lookups
- **Transaction History**: Consider archiving old transactions for large systems
- **State Snapshots**: Compressed serialization for efficient persistence

### Throughput Optimization  
- **Batch Processing**: Group multiple operations for higher throughput
- **Read Replicas**: Balance queries can be served from any replica
- **Operation Ordering**: Independent operations can be processed concurrently

### Consistency Guarantees
- **Strong Consistency**: All replicas see the same account balances
- **Linearizable Operations**: Operations appear to execute atomically
- **Durable Transactions**: All committed transactions survive node failures

## Security Considerations

### Input Validation
```rust
fn validate_amount(amount: i64) -> Result<(), String> {
    if amount <= 0 {
        return Err("Amount must be positive".to_string());
    }
    if amount > 1_000_000_000 { // $10M limit
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
    if !account_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err("Invalid characters in account ID".to_string());
    }
    Ok(())
}
```

### Audit Requirements
- **Immutable History**: Transaction records are never modified or deleted
- **Complete Trail**: Every balance change is recorded with timestamps
- **Deterministic IDs**: Transaction IDs are generated deterministically for consistency

## Implementation Notes

### Why This Works Well for SMR

1. **Clear State Model**: Accounts and transactions have well-defined relationships
2. **Deterministic Operations**: All operations produce predictable results
3. **Strong Invariants**: Financial rules are enforced consistently
4. **Atomic Operations**: Complex operations (transfers) succeed or fail completely
5. **Rich Audit Trail**: Complete history enables debugging and compliance

### SMR Considerations

1. **State Size**: Large transaction histories require careful management
2. **Snapshot Strategy**: Balance between recovery time and storage overhead
3. **Validation Logic**: All business rules must be deterministic across replicas
4. **Error Handling**: Failed operations must not corrupt the state machine

## Testing Strategy

### Unit Tests
- Individual operation correctness
- Edge case handling (insufficient funds, invalid accounts)
- State serialization/deserialization

### Integration Tests  
- Multi-operation scenarios
- Concurrent transfer testing
- State consistency verification

### Stress Tests
- High-volume transaction processing
- Memory usage under load
- Performance benchmarking

## Next Steps

After understanding the banking example, explore:
- [SMR Developer Guide](../../docs/SMR_GUIDE.md) - Comprehensive SMR development guide
- [Performance Benchmarks](../../benchmarks/) - Banking SMR performance optimization
- [Custom State Machine](../custom_state_machine.rs) - Template for your own SMR applications

This banking example demonstrates how SMR can provide the strong consistency and fault tolerance required for financial applications while maintaining high performance and scalability.