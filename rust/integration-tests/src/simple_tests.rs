use crate::current_test_name;
use crate::TestContext;
use crate::TestProcessorConfig;
use diesel::pg::PgConnection;
use diesel::Connection;
use diesel::sql_query;
use diesel::RunQueryDsl;
use processor::utils::database::run_pending_migrations;

#[tokio::test]
async fn test_case_1() {
    let current_name = current_test_name!();
    // TODO: merge all the setup into test_context.
    let test_context = TestContext::new(current_name).await.unwrap();
    assert_eq!(test_context.transaction_batches.len(), 1);
    let database_url = test_context.get_db_url().await;
    let processor_config = TestProcessorConfig{};
    let mut conn = PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
    // TODO: re-enable this after fixing the migration issue.
    // run_pending_migrations(&mut conn);
    // =============

    assert!(test_context.run(processor_config, move || {
        let mut conn = PgConnection::establish(&database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

        // TODO: change this to a more meaningful query.
        let result = sql_query("SELECT 1").execute(&mut conn);
        assert_eq!(result, Ok(1));
        
        Ok(())
    }).await.is_ok());
}