use crate::current_test_name;
use crate::TestContext;

#[tokio::test]
async fn test_case_1() {
    let current_name = current_test_name!();
    let test_context = TestContext::new(current_name);
    assert_eq!(test_context.transaction_batches.len(), 1);
    assert!(test_context.run(|txns| {
        assert_eq!(txns.len(), 2);
        Ok(())
    }).await.is_ok());
}