//! Tests for the improved Gotcha builder API

use gotcha::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
struct TestState {
    counter: u32,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct TestConfig {
    setting: String,
}

#[test]
fn test_new_api_with_types() {
    // Should compile without needing <(), ()>
    let _app = Gotcha::with_types::<TestState, TestConfig>()
        .state(TestState::default())
        .get("/", || async { "test" });
}

#[test]
fn test_new_api_with_state() {
    // Should compile with just state type
    let _app = Gotcha::with_state::<TestState>()
        .state(TestState::default())
        .get("/", || async { "test" });
}

#[test]
fn test_new_api_with_config() {
    // Should compile with just config type
    let _app = Gotcha::with_config::<TestConfig>()
        .get("/", || async { "test" });
}

#[test]
fn test_traditional_api_still_works() {
    // Traditional API should still work
    let _app = Gotcha::new()
        .get("/", || async { "test" });
}

#[test]
fn test_chaining_works() {
    // Test that method chaining works correctly
    let _app = Gotcha::with_types::<TestState, TestConfig>()
        .state(TestState::default())
        .host("0.0.0.0")
        .port(8080)
        .get("/", || async { "home" })
        .post("/users", || async { "create user" })
        .put("/users/:id", || async { "update user" })
        .delete("/users/:id", || async { "delete user" });
}