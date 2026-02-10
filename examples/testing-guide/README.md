# Gotcha 框架测试示例

展示如何为 Gotcha 应用编写集成测试。

## 核心区别

与 Axum 相比，Gotcha 的主要区别：

1. **Handler 状态类型**
   ```rust
   // Gotcha 使用
   State(ctx): State<GotchaContext<AppState, AppConfig>>

   // 而不是 Axum 的
   State(state): State<AppState>
   ```

2. **错误处理返回 HTTP 状态码**
   ```rust
   // 返回正确的状态码
   Result<Json<T>, (StatusCode, Json<ErrorResponse>)>
   ```

3. **测试设置**
   ```rust
   pub async fn create_test_app() -> axum::Router {
       let app = App;
       let config = ConfigWrapper { /* ... */ };
       let state = app.state(&config).await.unwrap();
       let context = GotchaContext { state, config };
       app.build_router(context).await.unwrap()
   }
   ```

## 运行测试

```bash
cargo test --test api_integration_test
```

## 文件结构

- `src/lib.rs` - 示例 CRUD API 实现
- `tests/api_integration_test.rs` - 7 个集成测试场景
- `tests/integration_tests.rs` - 另一组集成测试（可选）

## 依赖

```toml
[dev-dependencies]
axum-test = "16"
```

就这样，其他跟 Axum 测试一样。