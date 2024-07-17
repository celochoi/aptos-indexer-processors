use anyhow::Context;
use aptos_protos::transaction::v1::Transaction;
use std::path::PathBuf;
use crate::TestCaseTransactionBatch;
use dirs::home_dir;

const JSON_FILE_EXTENSION: &str = "json";

// Environment variable to specify the aptos core folder.
fn get_generated_transaction_folder() -> PathBuf {
    let home_dir = home_dir().expect("Failed to get the home directory");
    match std::env::var("APTOS_CORE_FOLDER") {
        Ok(val) => PathBuf::from(val),
        Err(_) => home_dir.join("aptos-core"),
    }
}

const GENERATED_TRANSACTION_FOLDER: &str =
    "ecosystem/indexer-grpc/indexer-transaction-generator/generated_transactions";

pub struct TransactionLoader {}

impl TransactionLoader {
    pub fn for_test(test_case_name: String) -> anyhow::Result<Vec<TestCaseTransactionBatch>> {
        // Get the generated transaction folder from the environment variable.
        let aptos_core_folder = get_generated_transaction_folder();
        let generated_transaction_folder = aptos_core_folder.join(GENERATED_TRANSACTION_FOLDER);
        let mut result = Vec::new();
        // Iterate over the version folders, i.e., `main`, `1.16`.
        for version_folder in
            std::fs::read_dir(&generated_transaction_folder).with_context(|| {
                format!(
                    "Failed to read the generated transaction folder: {:?}",
                    generated_transaction_folder
                )
            })?
        {
            // if the entry is not a directory, skip it.
            let version_folder = version_folder?;
            if !version_folder.path().is_dir() {
                // skip non-directory entries.
                continue;
            }
            // read the test case folder based on the test case name.
            let test_case_folder = version_folder.path().join(&test_case_name);
            if !test_case_folder.exists() {
                anyhow::bail!(
                    "Test case folder does not exist: {}",
                    test_case_folder.display()
                );
            }
            // read the transaction files in sequence, 1.json, 2.json, 3.json, ...
            let mut txns = Vec::new();
            for i in 1.. {
                let txn_file = test_case_folder.join(format!("{}.{}", i, JSON_FILE_EXTENSION));
                // If there are no more transactios to read, break the loop.
                if !txn_file.exists() {
                    break;
                }
                let txn = std::fs::read_to_string(&txn_file).with_context(|| {
                    format!("Failed to read the transaction file: {:?}", txn_file)
                })?;
                // let txn: Transaction = Transaction::decode(txn.as_bytes()).with_context(|| {
                //     format!("Failed to decode the transaction: {}", txn_file.display())
                // })?;
                let txn: Transaction = serde_json::from_str(&txn).with_context(|| {
                    format!("Failed to decode the transaction: {:?}", txn_file)
                })?;
                txns.push(txn);
            }
            if txns.is_empty() {
                anyhow::bail!(
                    "No transactions found in the test case folder: {}",
                    test_case_folder.display()
                );
            }
            result.push(TestCaseTransactionBatch {
                version: version_folder.file_name().to_string_lossy().to_string(),
                transactions: txns,
            });
        }

        Ok(result)
    }
}
