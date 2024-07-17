use crate::current_test_name;
use crate::TestContext;
use diesel::pg::PgConnection;
use diesel::Connection;
use diesel::sql_query;
use diesel::RunQueryDsl;

#[tokio::test]
async fn test_case_1() {
    let current_name = current_test_name!();
    let test_context = TestContext::new(current_name).await.unwrap();
    assert_eq!(test_context.transaction_batches.len(), 1);
    let database_url = test_context.get_db_url().await;
    println!("database_url: {}", database_url);

    assert!(test_context.run(move || {
        let mut conn = PgConnection::establish(&database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

        let result = sql_query("SELECT 1").execute(&mut conn);
        assert_eq!(result, Ok(1));
        Ok(())
    }).await.is_ok());
}