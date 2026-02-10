// Simplified integration test focusing on real API testing value

use axum_test::TestServer;
use http::StatusCode;
use serde_json::json;
use testing_guide::create_test_app;
use uuid::Uuid;

// ========== Test Setup ==========
async fn setup() -> TestServer {
    TestServer::new(create_test_app().await).unwrap()
}

// ========== Core API Tests ==========

#[tokio::test]
async fn test_complete_user_lifecycle() {
    let server = setup().await;

    // CREATE user
    let create_res = server
        .post("/users")
        .json(&json!({
            "name": "John Doe",
            "email": "john@example.com",
            "age": 30
        }))
        .await;

    create_res.assert_status_ok();
    let created: serde_json::Value = create_res.json();
    let user_id = created["data"]["id"].as_str().unwrap();

    // GET created user
    let get_res = server
        .get(&format!("/users/{}", user_id))
        .await;

    get_res.assert_status_ok();
    let fetched: serde_json::Value = get_res.json();
    assert_eq!(fetched["data"]["name"], "John Doe");

    // UPDATE user
    let update_res = server
        .put(&format!("/users/{}", user_id))
        .json(&json!({
            "name": "John Updated",
            "age": 31
        }))
        .await;

    update_res.assert_status_ok();
    let updated: serde_json::Value = update_res.json();
    assert_eq!(updated["data"]["name"], "John Updated");
    assert_eq!(updated["data"]["age"], 31);

    // LIST users (should contain our user)
    let list_res = server.get("/users").await;
    list_res.assert_status_ok();
    let list: serde_json::Value = list_res.json();
    let users = list["data"].as_array().unwrap();
    assert!(users.iter().any(|u| u["id"] == user_id));

    // DELETE user
    let delete_res = server
        .delete(&format!("/users/{}", user_id))
        .await;

    delete_res.assert_status_ok();

    // VERIFY deletion (should return 404)
    let verify_res = server
        .get(&format!("/users/{}", user_id))
        .expect_failure()
        .await;

    let error: serde_json::Value = verify_res.json();
    assert_eq!(error["code"], 404);
}

#[tokio::test]
async fn test_pagination_and_filtering() {
    let server = setup().await;

    // Create test data
    for i in 0..15 {
        server
            .post("/users")
            .json(&json!({
                "name": format!("User {}", i),
                "email": format!("user{}@test.com", i),
                "age": 20 + i
            }))
            .await
            .assert_status_ok();
    }

    // Test pagination - page 1
    let page1 = server
        .get("/users?page=1&size=5")
        .await;

    page1.assert_status_ok();
    let data: serde_json::Value = page1.json();
    let users = data["data"].as_array().unwrap();
    assert_eq!(users.len(), 5);

    // Test pagination - page 2
    let page2 = server
        .get("/users?page=2&size=5")
        .await;

    page2.assert_status_ok();
    let data: serde_json::Value = page2.json();
    let users = data["data"].as_array().unwrap();
    assert_eq!(users.len(), 5);

    // Test pagination - beyond available data
    let page_beyond = server
        .get("/users?page=10&size=5")
        .await;

    page_beyond.assert_status_ok();
    let data: serde_json::Value = page_beyond.json();
    let users = data["data"].as_array().unwrap();
    assert_eq!(users.len(), 0);
}

#[tokio::test]
async fn test_error_handling() {
    let server = setup().await;

    // Test 404 - non-existent user
    let not_found = server
        .get(&format!("/users/{}", Uuid::new_v4()))
        .expect_failure()
        .await;

    let error: serde_json::Value = not_found.json();
    assert_eq!(error["error"], "User not found");
    assert_eq!(error["code"], 404);

    // Test 400 - invalid UUID format
    server
        .get("/users/invalid-uuid")
        .expect_failure()
        .await
        .assert_status(StatusCode::BAD_REQUEST);

    // Test 415 - malformed JSON (axum returns Unsupported Media Type for bad JSON)
    server
        .post("/users")
        .content_type("application/json")
        .text("{invalid json}")
        .expect_failure()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE);

    // Test 422 - missing required fields (axum returns 422 for missing JSON fields)
    server
        .post("/users")
        .json(&json!({"name": "No Email"}))  // missing email and age
        .expect_failure()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_concurrent_operations() {
    let server = setup().await;

    // Create initial user
    let create_res = server
        .post("/users")
        .json(&json!({
            "name": "Concurrent Test",
            "email": "concurrent@test.com",
            "age": 25
        }))
        .await;

    let user: serde_json::Value = create_res.json();
    let user_id = user["data"]["id"].as_str().unwrap();

    // Simulate concurrent updates by running them sequentially
    // (axum-test TestServer doesn't support true concurrent operations)
    for i in 0..10 {
        let response = server
            .put(&format!("/users/{}", user_id))
            .json(&json!({"age": 25 + i}))
            .await;

        response.assert_status_ok();
    }

    // Verify final state is consistent
    let final_res = server
        .get(&format!("/users/{}", user_id))
        .await;

    final_res.assert_status_ok();
    let final_user: serde_json::Value = final_res.json();
    let age = final_user["data"]["age"].as_u64().unwrap();

    // Age should be 34 (the last update)
    assert_eq!(age, 34);
}

#[tokio::test]
async fn test_state_persistence_across_requests() {
    let server = setup().await;

    // Get initial counter from health endpoint
    let health1 = server.get("/health").await;
    health1.assert_status_ok();
    let data1: serde_json::Value = health1.json();
    let counter_str1 = data1["data"].as_str().unwrap();
    let initial_count: i32 = counter_str1
        .split("Counter: ")
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();

    // Create 3 users
    for i in 0..3 {
        server
            .post("/users")
            .json(&json!({
                "name": format!("User {}", i),
                "email": format!("user{}@test.com", i),
                "age": 20
            }))
            .await
            .assert_status_ok();
    }

    // Check counter increased by 3
    let health2 = server.get("/health").await;
    health2.assert_status_ok();
    let data2: serde_json::Value = health2.json();
    let counter_str2 = data2["data"].as_str().unwrap();
    let final_count: i32 = counter_str2
        .split("Counter: ")
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();

    assert_eq!(final_count, initial_count + 3);
}

#[tokio::test]
async fn test_partial_updates() {
    let server = setup().await;

    // Create user with all fields
    let create_res = server
        .post("/users")
        .json(&json!({
            "name": "Original Name",
            "email": "original@test.com",
            "age": 30
        }))
        .await;

    let user: serde_json::Value = create_res.json();
    let user_id = user["data"]["id"].as_str().unwrap();

    // Update only name
    let update1 = server
        .put(&format!("/users/{}", user_id))
        .json(&json!({"name": "New Name"}))
        .await;

    update1.assert_status_ok();
    let updated1: serde_json::Value = update1.json();
    assert_eq!(updated1["data"]["name"], "New Name");
    assert_eq!(updated1["data"]["email"], "original@test.com");  // unchanged
    assert_eq!(updated1["data"]["age"], 30);  // unchanged

    // Update only age
    let update2 = server
        .put(&format!("/users/{}", user_id))
        .json(&json!({"age": 31}))
        .await;

    update2.assert_status_ok();
    let updated2: serde_json::Value = update2.json();
    assert_eq!(updated2["data"]["name"], "New Name");  // keeps previous update
    assert_eq!(updated2["data"]["email"], "original@test.com");  // still unchanged
    assert_eq!(updated2["data"]["age"], 31);  // new value
}

#[tokio::test]
async fn test_bulk_operations() {
    let server = setup().await;
    let start = std::time::Instant::now();

    // Create 50 users sequentially
    for i in 0..50 {
        let response = server
            .post("/users")
            .json(&json!({
                "name": format!("Bulk User {}", i),
                "email": format!("bulk{}@test.com", i),
                "age": 20 + (i % 50)
            }))
            .await;

        response.assert_status_ok();
    }

    let duration = start.elapsed();

    // Verify all users were created
    let list_res = server
        .get("/users?size=100")
        .await;

    list_res.assert_status_ok();
    let data: serde_json::Value = list_res.json();
    let users = data["data"].as_array().unwrap();

    // Should have at least 50 users (51 including default)
    assert!(users.len() >= 50);

    // Basic performance check (should complete in reasonable time)
    assert!(duration.as_secs() < 30, "Bulk operations took too long");
}