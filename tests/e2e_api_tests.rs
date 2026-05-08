// E2E API Tests Entry Point
//
// This file serves as the test binary entry point for the comprehensive
// End-to-End API tests for the POS Terminal.
//
// ## Running the Tests
//
// All tests require a running E2Manage backend server. Tests must run in sequence
// because they share state (terminal registration, auth tokens, shift IDs, etc.).
//
// ```bash
// # Set backend URL (defaults to localhost:3000)
// export BACKEND_URL=http://localhost:3000
//
// # Run all E2E tests in sequence
// cargo test --test e2e_api_tests -- --ignored --test-threads=1
//
// # Run specific test phase
// cargo test --test e2e_api_tests terminal_management -- --ignored --test-threads=1
// cargo test --test e2e_api_tests sync_apis -- --ignored --test-threads=1
// cargo test --test e2e_api_tests shift_management -- --ignored --test-threads=1
// cargo test --test e2e_api_tests transaction_management -- --ignored --test-threads=1
// cargo test --test e2e_api_tests return_management -- --ignored --test-threads=1
// cargo test --test e2e_api_tests offline_queue -- --ignored --test-threads=1
// cargo test --test e2e_api_tests cash_drawer -- --ignored --test-threads=1
// cargo test --test e2e_api_tests reports -- --ignored --test-threads=1
// cargo test --test e2e_api_tests fleet_management -- --ignored --test-threads=1
// ```
//
// ## Test Sequence
//
// Tests MUST run in this order due to dependencies:
//
// 1. **Terminal Management** - Register and authenticate terminal
// 2. **Sync APIs** - Sync catalog, operators, screens
// 3. **Shift Management** - Start a shift
// 4. **Transaction Management** - Create sales
// 5. **Return Management** - Process returns
// 6. **Offline Queue** - Upload and process offline transactions
// 7. **Cash Drawer** - Log drawer events
// 8. **Reports** - Generate reports and end shift
// 9. **Fleet Management** - Heartbeat and logout
//
// ## Environment Variables
//
// - `BACKEND_URL` - E2Manage API URL (default: http://localhost:3000)
//
// ## Test State
//
// Tests share state through a global `TestState` struct that stores:
// - Hardware ID, Terminal ID, Terminal Secret
// - Session Token, CSRF Token
// - Shift ID, Transaction ID, Receipt Number
// - Queue ID, Operator ID, Product IDs

mod api_tests;
