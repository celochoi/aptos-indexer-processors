pub mod transaction_loader;
use anyhow::Context;
use itertools::Itertools; 
use aptos_protos::transaction::v1::Transaction;
use processor::processors::{Processor, ProcessorTrait};
use std::{future::Future, sync::Arc};

mod test_case_1;

pub const INTEGRATION_TESTS_PACKAGE_PREFIX: &str = "integration_tests::";

/// Returns the name of the current function. This macro is used to derive the
/// name for the golden file of each test case. We remove the API version
/// (e.g. v0) from the path.
/// This function is moved and modified from aptos-labs/aptos-core folder.
#[macro_export]
macro_rules! current_test_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        let mut strip = 3;
        if name.contains("::{{closure}}") {
            strip += 13;
        }
        use crate::INTEGRATION_TESTS_PACKAGE_PREFIX;
        // Trim off the `integration_tests::` at the beginning and the `::f` at the end. 
        let processed_name = name[INTEGRATION_TESTS_PACKAGE_PREFIX.len()..name.len() - strip].to_string();
        // processed_name is in the format of `test_case_1::run`
        let parts: Vec<&str> = processed_name.split("::").collect();
        // Last part is the function name, ignore. Join the rest with `/`.
        parts.last().unwrap().to_string()
    }};
}

/// The `TestCaseTransactions` struct represents a test case with a list of transactions.
/// The `version` field is the version of the test case.
#[derive(Debug)]
pub struct TestCaseTransactionBatch {
    pub version: String,
    pub transactions: Vec<Transaction>,
}

/// The test context struct holds the test name and the transaction batches.
pub struct TestContext {
    pub test_name: String,
    pub transaction_batches: Vec<TestCaseTransactionBatch>,
}


impl TestContext {
    // TODO: move this to builder pattern to allow chaining.
    pub fn new(test_name: String) -> Self {
        println!("Creating test context for test: {}", test_name);
        let transaction_batches = transaction_loader::TransactionLoader::for_test(test_name.clone()).unwrap();
        Self { test_name, transaction_batches }
    }

    // `run` functions takes a closure that is executed after the test context is created.
    // The closure will be executed multiple times with different permutations of the transactions.
    // For example:
    //   test.run(async move | context | {
    //       // Runs after every permutatation
    //       let res = Diesel::raw_sql(conn, "select amount from balances where user = '0x1'");
    //       assert_eq!(res["amount"], 100, "winner balance incorrect when txn order: {:?}", context.txn_order);
    //   }).await;
    pub async fn run(&self, processor: Arc<dyn ProcessorTrait>) -> anyhow::Result<()>
    {
        // For each versioned batch, get the permutations of the transactions.
        for batch in &self.transaction_batches {
            let transactions = &batch.transactions;
            let release_version = &batch.version;

            // TODO: setup a new processor instead of using the same one.

            // Get the permutations of the transactions.
            for perm in transactions.iter().permutations(transactions.len()) {
                // Spawn a new task to process each transaction. 
                // This is important to make sure in all cases, processor can achieve
                // eventual consistency.
                let mut tasks = Vec::new();
                let versions = transactions.iter().map(|txn| txn.version.clone()).collect::<Vec<u64>>();
                for txn in perm {
                    let txn = txn.clone();
                    let current_processor = processor.clone();
                    tasks.push(tokio::spawn(async move {
                        // Process the transaction.
                        // processor.process(txn).await;
                        let start_version = txn.version;
                        let end_version = txn.version;
                        current_processor.process_transactions(
                            vec![txn],
                            start_version,
                            end_version,
                            None,
                        ).await
                    }));
                    // Wait and yield to new task.
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
                // Wait for all tasks to finish.
                for (idx, task) in tasks.into_iter().enumerate() {
                    task.await.with_context(|| {
                        format!("[Release version {}] Test failed for txn permutation: {:?} at txn version {}", release_version, versions, idx + 1)
                    })??;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::current_test_name;

    #[test]
    fn test_current_test_name() {
        let current_name = current_test_name!();
        assert_eq!(current_name, "test_current_test_name");
    }
}

