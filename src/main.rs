// ============================================================================
// E2Manage POS Terminal - Main Entry Point
// ============================================================================

slint::include_modules!();

mod dev_harness;
mod locale_detect;

use e2manage_pos_terminal::api::ApiClient;
use e2manage_pos_terminal::api::PairingStatus;
use e2manage_pos_terminal::db::{init_database, Database};
use e2manage_pos_terminal::services::{
    AuthService, CartService, DraftService, FeatureService, PairingService, ProductService,
    SharedDraftService, SyncEvent, SyncService, SystemService,
};
use e2manage_pos_terminal::ui::navigation::{NavigationResult, Navigator};
use parking_lot::Mutex;
use rust_decimal::prelude::ToPrimitive;
use slint::{ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// Polling interval for pairing status (in milliseconds)
const PAIRING_POLL_INTERVAL_MS: u64 = 3000;

/// Sync interval in minutes (how often to pull data from backend)
const SYNC_INTERVAL_MINUTES: u64 = 5;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    // Developer harness mode — bypass full app startup
    if std::env::args().any(|a| a == "--theme-harness") {
        return dev_harness::run();
    }

    info!("===========================================");
    info!("  E2Manage POS Terminal v{}", env!("CARGO_PKG_VERSION"));
    info!("===========================================");

    // Create tokio runtime for async operations
    let runtime = Arc::new(Runtime::new()?);

    // Initialize database in data/ directory
    let data_dir = PathBuf::from("data");
    let db = Arc::new(init_database(&data_dir)?);
    info!("Database initialized in {:?}", data_dir);

    // Get server URL from environment or use default
    let server_url =
        std::env::var("E2M_API_URL").unwrap_or_else(|_| "http://178.156.135.235:3000".to_string());
    info!("API Server: {}", server_url);

    // Create API client
    let api = Arc::new(ApiClient::new(&server_url));

    // Load saved session token and set on API client
    let auth_service = Arc::new(AuthService::new(Arc::clone(&api), Arc::clone(&db)));
    let saved_session = auth_service.load_saved_session().ok().flatten();
    if let Some(ref session) = saved_session {
        if !session.session_token.is_empty() {
            info!(
                "Loaded saved session token for terminal: {}",
                session.terminal_code
            );
            runtime.block_on(api.set_token(session.session_token.clone()));
        }
    }

    // Create services
    let pairing_service = Arc::new(PairingService::new(Arc::clone(&api), Arc::clone(&db)));
    let feature_service = FeatureService::new(Arc::clone(&db));
    let navigator = Arc::new(Mutex::new(Navigator::new(feature_service)));

    // Get terminal info for shared drafts (if registered)
    // Note: warehouse_id would typically come from terminal_config table or backend
    let registration = pairing_service.get_registration().ok().flatten();
    let terminal_id_for_drafts = registration
        .as_ref()
        .and_then(|r| r.terminal_id.clone())
        .or_else(|| registration.as_ref().and_then(|r| r.terminal_code.clone()))
        .unwrap_or_else(|| "TERM-001".to_string());

    // Get warehouse_id from terminal_config if available, otherwise use tenant_id or default
    let warehouse_id_for_drafts = registration
        .as_ref()
        .and_then(|r| r.tenant_id.clone())
        .unwrap_or_else(|| "default-warehouse".to_string());

    // Create shared draft service for cloud-synced drafts
    let shared_draft_service = Arc::new(SharedDraftService::new(
        Arc::clone(&api),
        Arc::clone(&db),
        warehouse_id_for_drafts,
        terminal_id_for_drafts,
    ));

    // Create sync service with shared draft service
    let mut sync_service =
        SyncService::new(Arc::clone(&api), Arc::clone(&db), SYNC_INTERVAL_MINUTES);
    sync_service.set_shared_draft_service(Arc::clone(&shared_draft_service));
    let sync_service = Arc::new(sync_service);

    let cart_service = Arc::new(CartService::new());
    let draft_service = Arc::new(DraftService::new(Arc::clone(&db)));

    // Create broadcast channel for sync events
    let (sync_tx, _sync_rx) = broadcast::channel::<SyncEvent>(16);
    let sync_tx = Arc::new(sync_tx);

    // Check if terminal is registered and get terminal code and company name
    let is_registered = pairing_service.is_registered().unwrap_or(false);
    let registration = pairing_service.get_registration().ok().flatten();
    let terminal_code = registration
        .as_ref()
        .and_then(|r| r.terminal_code.clone())
        .unwrap_or_else(|| "غير مسجل".to_string());
    let company_name = registration
        .as_ref()
        .and_then(|r| r.company_name.clone())
        .unwrap_or_default();
    info!(
        "Terminal registered: {}, code: {}, company: {}",
        is_registered, terminal_code, company_name
    );

    // Create the main window
    let window = MainWindow::new()?;

    // === Apply detected locale to UI globals (Plan 2 Task 1) ===========
    {
        let (locale_code, rtl) = locale_detect::detect_locale();
        window.set_rtl(rtl);
        window.set_locale(SharedString::from(locale_code));
        info!(
            locale = locale_code,
            rtl, "applied detected locale to UI globals"
        );
    }

    // Set terminal code and company name in AppState via window bindings
    window.set_app_terminal_code(terminal_code.clone().into());
    if !company_name.is_empty() {
        window.set_app_company_name(company_name.into());
    }

    // Propagate currency and tax config from saved session to UI
    if let Some(ref session) = saved_session {
        let decimals =
            e2manage_pos_terminal::utils::formatting::currency_decimals(&session.currency);
        let zero = format!("{:.*}", decimals as usize, 0.0f64);
        window.set_app_currency(SharedString::from(&session.currency));
        window.set_app_currency_decimals(decimals as i32);
        window.set_app_zero_amount(SharedString::from(&zero));

        let tax_rate = session.tax_rate;
        let tax_label = if tax_rate == tax_rate.floor() {
            format!("{}%", tax_rate as i64)
        } else {
            format!("{}%", tax_rate)
        };
        window.set_app_tax_rate_value(tax_rate as f32);
        window.set_app_tax_rate_label(SharedString::from(&tax_label));
        window.set_app_tax_inclusive(session.tax_inclusive);

        info!(
            "Currency: {}, decimals: {}, tax_rate: {}%",
            session.currency, decimals, tax_rate
        );
    }

    // Store current server URL for callbacks
    let current_server_url = Arc::new(RwLock::new(server_url.clone()));

    // Set up pairing callbacks
    setup_pairing_callbacks(
        &window,
        Arc::clone(&pairing_service),
        Arc::clone(&runtime),
        Arc::clone(&db),
        Arc::clone(&current_server_url),
    );

    // Set up feature navigation callbacks
    setup_navigation_callbacks(&window, Arc::clone(&navigator));

    // Set up shutdown dialog callbacks
    setup_shutdown_callbacks(&window);

    // Set up sync dialog callbacks (triggered from settings sync screen)
    setup_sync_callbacks(
        &window,
        Arc::clone(&sync_service),
        Arc::clone(&sync_tx),
        Arc::clone(&runtime),
    );

    // Set up catalog callbacks (category selection, add to cart)
    let session_currency = saved_session
        .as_ref()
        .map(|s| s.currency.clone())
        .unwrap_or_else(|| "LYD".to_string());
    setup_catalog_callbacks(
        &window,
        Arc::clone(&db),
        Arc::clone(&cart_service),
        session_currency.clone(),
    );

    // Set up price check callbacks (Phase 3 — Price Check Mode)
    setup_price_check_callbacks(&window, Arc::clone(&db), session_currency.clone());

    // Set up cart item operation callbacks (Phase 2: increment, decrement, remove, clear, void-last, qty-multiply)
    setup_cart_item_callbacks(
        &window,
        Arc::clone(&cart_service),
        session_currency.clone(),
        Arc::clone(&db),
    );

    // Set up draft save callback
    setup_draft_callbacks(
        &window,
        Arc::clone(&draft_service),
        Arc::clone(&cart_service),
    );

    // Set up sync callback (triggered on shift start)
    let sync_service_for_callback = Arc::clone(&sync_service);
    let sync_tx_for_callback = Arc::clone(&sync_tx);
    let runtime_for_callback = Arc::clone(&runtime);
    window.on_request_sync(move || {
        trigger_manual_sync(
            Arc::clone(&sync_service_for_callback),
            Arc::clone(&sync_tx_for_callback),
            Arc::clone(&runtime_for_callback),
        );
    });

    // Set up operator selection callback (looks up operator from DB and sets UI properties)
    let db_for_operator = Arc::clone(&db);
    let window_weak_for_operator = window.as_weak();
    window.on_on_operator_selected(move |operator_id: SharedString| {
        let operator_id_str: String = operator_id.into();
        info!("Operator selected: {}", operator_id_str);

        // Look up operator from database
        match db_for_operator.get_operator_by_id(&operator_id_str) {
            Ok(Some(op)) => {
                // Avatar colors (same as in populate_operators_data)
                let avatar_colors: [(u8, u8, u8); 8] = [
                    (37, 99, 235),  // Blue
                    (139, 92, 246), // Purple
                    (34, 197, 94),  // Green
                    (245, 158, 11), // Amber
                    (239, 68, 68),  // Red
                    (6, 182, 212),  // Cyan
                    (236, 72, 153), // Pink
                    (249, 115, 22), // Orange
                ];

                // Generate color from ID hash
                let color_idx = operator_id_str
                    .bytes()
                    .fold(0usize, |acc, b| acc.wrapping_add(b as usize))
                    % avatar_colors.len();
                let color = avatar_colors[color_idx];

                // Use Arabic name if available
                let display_name = op.name_ar.as_deref().unwrap_or(&op.name);

                // Generate initials
                let initials: String = display_name
                    .split_whitespace()
                    .filter_map(|word| word.chars().next())
                    .take(2)
                    .collect();

                // Map role to Arabic
                let role_ar = match op.role.to_uppercase().as_str() {
                    "CASHIER" => "كاشير",
                    "SUPERVISOR" | "MANAGER" => "مشرف",
                    "ADMIN" => "مدير",
                    _ => "كاشير",
                };

                // Update UI properties
                if let Some(w) = window_weak_for_operator.upgrade() {
                    w.set_selected_operator_id(SharedString::from(&op.id));
                    w.set_selected_operator_name(SharedString::from(display_name));
                    w.set_selected_operator_role(SharedString::from(role_ar));
                    w.set_selected_operator_initials(SharedString::from(&initials));
                    w.set_selected_operator_color(slint::Color::from_rgb_u8(
                        color.0, color.1, color.2,
                    ));
                    w.set_pin_error(false);
                    w.set_pin_error_message(SharedString::from(""));
                    w.set_pin_attempts_remaining(3);
                    w.set_current_screen(SharedString::from("pin-entry"));
                    info!("Navigating to PIN entry for operator: {}", display_name);
                }
            }
            Ok(None) => {
                warn!("Operator not found: {}", operator_id_str);
            }
            Err(e) => {
                warn!("Failed to look up operator {}: {}", operator_id_str, e);
            }
        }
    });

    // Set up search operators callback (filters operator grid)
    let db_for_search = Arc::clone(&db);
    let window_weak_for_search = window.as_weak();
    window.on_search_operators(move |query: SharedString| {
        let query_str: String = query.into();
        let db = Arc::clone(&db_for_search);
        let window_weak = window_weak_for_search.clone();

        info!("Search operators: '{}'", query_str);

        let operators = if query_str.is_empty() {
            db.get_operators().unwrap_or_default()
        } else {
            db.search_operators(&query_str, 20).unwrap_or_default()
        };

        let avatar_colors: [(u8, u8, u8); 8] = [
            (37, 99, 235),  // Blue
            (139, 92, 246), // Purple
            (34, 197, 94),  // Green
            (245, 158, 11), // Amber
            (239, 68, 68),  // Red
            (6, 182, 212),  // Cyan
            (236, 72, 153), // Pink
            (249, 115, 22), // Orange
        ];

        let slint_operators: Vec<Operator> = operators
            .into_iter()
            .map(|op| {
                let display_name = op.name_ar.as_deref().unwrap_or(&op.name);
                let initials = generate_arabic_initials(display_name);
                let role_ar = match op.role.to_uppercase().as_str() {
                    "CASHIER" => "كاشير",
                    "SUPERVISOR" | "MANAGER" => "مشرف",
                    "ADMIN" => "مدير",
                    _ => "كاشير",
                };
                let color_idx = op
                    .id
                    .bytes()
                    .fold(0usize, |acc, b| acc.wrapping_add(b as usize))
                    % avatar_colors.len();
                let color = avatar_colors[color_idx];

                Operator {
                    id: SharedString::from(&op.id),
                    name: SharedString::from(display_name),
                    role: SharedString::from(role_ar),
                    initials: SharedString::from(&initials),
                    color: slint::Color::from_rgb_u8(color.0, color.1, color.2),
                }
            })
            .collect();

        slint::invoke_from_event_loop(move || {
            if let Some(w) = window_weak.upgrade() {
                let model = ModelRc::new(VecModel::from(slint_operators));
                w.set_operators(model);
            }
        })
        .ok();
    });

    // Set up employee ID submitted callback (looks up operator by employee number)
    let db_for_empid = Arc::clone(&db);
    let window_weak_for_empid = window.as_weak();
    window.on_employee_id_submitted(move |employee_number: SharedString| {
        let emp_num_str: String = employee_number.into();
        info!("Employee ID submitted: {}", emp_num_str);

        let avatar_colors: [(u8, u8, u8); 8] = [
            (37, 99, 235),  // Blue
            (139, 92, 246), // Purple
            (34, 197, 94),  // Green
            (245, 158, 11), // Amber
            (239, 68, 68),  // Red
            (6, 182, 212),  // Cyan
            (236, 72, 153), // Pink
            (249, 115, 22), // Orange
        ];

        match db_for_empid.get_operator_by_employee_number(&emp_num_str) {
            Ok(Some(op)) => {
                let display_name = op.name_ar.as_deref().unwrap_or(&op.name).to_string();
                let initials: String = display_name
                    .split_whitespace()
                    .filter_map(|word| word.chars().next())
                    .take(2)
                    .collect();
                let role_ar = match op.role.to_uppercase().as_str() {
                    "CASHIER" => "كاشير",
                    "SUPERVISOR" | "MANAGER" => "مشرف",
                    "ADMIN" => "مدير",
                    _ => "كاشير",
                };
                let color_idx = op
                    .id
                    .bytes()
                    .fold(0usize, |acc, b| acc.wrapping_add(b as usize))
                    % avatar_colors.len();
                let color = avatar_colors[color_idx];
                let op_id = op.id.clone();

                if let Some(w) = window_weak_for_empid.upgrade() {
                    w.set_employee_id_error(SharedString::from(""));
                    w.set_selected_operator_id(SharedString::from(&op_id));
                    w.set_selected_operator_name(SharedString::from(&display_name));
                    w.set_selected_operator_role(SharedString::from(role_ar));
                    w.set_selected_operator_initials(SharedString::from(&initials));
                    w.set_selected_operator_color(slint::Color::from_rgb_u8(
                        color.0, color.1, color.2,
                    ));
                    w.set_pin_error(false);
                    w.set_pin_error_message(SharedString::from(""));
                    w.set_pin_attempts_remaining(3);
                    w.set_current_screen(SharedString::from("pin-entry"));
                    info!(
                        "Employee ID login: navigating to PIN entry for {}",
                        display_name
                    );
                }
            }
            Ok(None) => {
                warn!("No operator found with employee number: {}", emp_num_str);
                if let Some(w) = window_weak_for_empid.upgrade() {
                    w.set_employee_id_error(SharedString::from("رقم الموظف غير موجود"));
                }
            }
            Err(e) => {
                warn!("Failed to look up employee number {}: {}", emp_num_str, e);
                if let Some(w) = window_weak_for_empid.upgrade() {
                    w.set_employee_id_error(SharedString::from("خطأ في البحث"));
                }
            }
        }
    });

    // Set up employee ID backspace callback (removes last character - Slint can't slice strings)
    let window_weak_for_backspace = window.as_weak();
    window.on_employee_id_backspace(move || {
        if let Some(w) = window_weak_for_backspace.upgrade() {
            let current: String = w.get_employee_id_text().into();
            if !current.is_empty() {
                let new_val: String = current.chars().take(current.chars().count() - 1).collect();
                w.set_employee_id_text(SharedString::from(&new_val));
            }
        }
    });

    // Set up PIN backspace callback (removes last character - Slint can't slice strings)
    let window_weak_for_pin_backspace = window.as_weak();
    window.on_pin_backspace(move || {
        if let Some(w) = window_weak_for_pin_backspace.upgrade() {
            let current: String = w.get_pin_text().into();
            if !current.is_empty() {
                let new_val: String = current.chars().take(current.chars().count() - 1).collect();
                w.set_pin_text(SharedString::from(&new_val));
            }
        }
    });

    // Set up PIN verification callback — calls AuthService.verify_pin()
    let window_weak_for_pin = window.as_weak();
    let auth_for_pin = Arc::clone(&auth_service);
    let rt_for_pin = Arc::clone(&runtime);
    window.on_verify_pin(move |operator_id, pin| {
        let op_id: String = operator_id.into();
        let pin_str: String = pin.into();
        let auth = Arc::clone(&auth_for_pin);
        let rt = Arc::clone(&rt_for_pin);
        let w = window_weak_for_pin.clone();

        std::thread::spawn(move || {
            let result = rt.block_on(auth.verify_pin(&op_id, &pin_str));

            slint::invoke_from_event_loop(move || {
                if let Some(window) = w.upgrade() {
                    match result {
                        Ok(ref r) if r.valid => {
                            info!("PIN verified for {}", r.operator_name);
                            window.set_pin_error(false);
                            window.set_pin_text(SharedString::default());
                            window.set_app_current_user(SharedString::from(&r.operator_name));
                            window.set_current_screen("start-shift".into());
                        }
                        _ => {
                            let remaining = window.get_pin_attempts_remaining() - 1;
                            warn!("PIN verification failed, attempts remaining: {}", remaining);
                            window.set_pin_error(true);
                            window.set_pin_error_message(SharedString::from("رمز PIN خاطئ"));
                            window.set_pin_attempts_remaining(remaining);
                            window.set_pin_text(SharedString::default());
                        }
                    }
                }
            })
            .ok();
        });
    });

    // Get weak reference for the loading thread
    let window_weak = window.as_weak();
    let pairing_service_clone = Arc::clone(&pairing_service);
    let runtime_clone = Arc::clone(&runtime);
    let db_clone = Arc::clone(&db);
    let sync_service_clone = Arc::clone(&sync_service);
    let sync_tx_clone = Arc::clone(&sync_tx);

    // Spawn thread to handle splash screen and initial routing
    let startup_currency = session_currency.clone();
    std::thread::spawn(move || {
        run_startup_sequence(StartupContext {
            window_weak,
            pairing_service: pairing_service_clone,
            runtime: runtime_clone,
            db: db_clone,
            sync_service: sync_service_clone,
            sync_tx: sync_tx_clone,
            is_registered,
            currency: startup_currency,
        });
    });

    // Intercept window close button (X) to show shutdown dialog
    let window_weak_for_close = window.as_weak();
    window.window().on_close_requested(move || {
        if let Some(w) = window_weak_for_close.upgrade() {
            // Show shutdown dialog instead of closing immediately
            w.set_show_shutdown_dialog(true);
        }
        slint::CloseRequestResponse::KeepWindowShown
    });

    // Start clock timer — HH:MM only changes every 60s, so 30s is plenty
    let window_weak_for_clock = window.as_weak();
    let clock_timer = slint::Timer::default();
    clock_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(30),
        move || {
            if let Some(w) = window_weak_for_clock.upgrade() {
                let now = chrono::Local::now().format("%H:%M").to_string();
                w.set_app_current_time(SharedString::from(&now));
            }
        },
    );

    // Set initial time immediately
    {
        let now = chrono::Local::now().format("%H:%M").to_string();
        window.set_app_current_time(SharedString::from(&now));
    }

    // Start connectivity check timer — polls every 30 seconds
    let window_weak_for_online = window.as_weak();
    let api_for_online = Arc::clone(&api);
    let runtime_for_online = Arc::clone(&runtime);
    let db_for_online = Arc::clone(&db);
    let online_timer = slint::Timer::default();
    online_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(30),
        move || {
            let api = Arc::clone(&api_for_online);
            let rt = Arc::clone(&runtime_for_online);
            let db = Arc::clone(&db_for_online);
            let w = window_weak_for_online.clone();
            std::thread::spawn(move || {
                let status = rt.block_on(api.is_online());
                let is_online = status.is_online();
                let pending_count = db.get_pending_transaction_count().unwrap_or(0);
                slint::invoke_from_event_loop(move || {
                    if let Some(window) = w.upgrade() {
                        window.set_app_is_online(is_online);
                        window.set_app_pending_sync_count(pending_count as i32);
                    }
                })
                .ok();
            });
        },
    );

    // Set initial online status
    {
        let status = runtime.block_on(api.is_online());
        let pending_count = db.get_pending_transaction_count().unwrap_or(0);
        window.set_app_is_online(status.is_online());
        window.set_app_pending_sync_count(pending_count as i32);
    }

    // Run the event loop
    window.run()?;

    info!("E2Manage POS Terminal shutting down");
    Ok(())
}

/// Sets up the pairing-related callbacks
fn setup_pairing_callbacks(
    window: &MainWindow,
    pairing_service: Arc<PairingService>,
    runtime: Arc<Runtime>,
    _db: Arc<Database>,
    url_state: Arc<RwLock<String>>,
) {
    // Refresh pairing code
    let ps = Arc::clone(&pairing_service);
    let rt = Arc::clone(&runtime);
    let window_weak = window.as_weak();

    window.on_setup_refresh_requested(move || {
        info!("Refresh pairing code requested");
        let ps_clone = Arc::clone(&ps);
        let window_weak_clone = window_weak.clone();
        let rt_clone = Arc::clone(&rt);

        // Check if we're on the setup screen due to an error (zero operators,
        // auth rejected, etc.) — if so, we need to clear the broken registration
        // first so the backend issues a fresh pairing code instead of recovering
        // the same dead registration.
        let is_error_recovery = window_weak
            .upgrade()
            .map(|w| w.get_setup_status().as_str() == "error")
            .unwrap_or(false);

        std::thread::spawn(move || {
            if is_error_recovery {
                warn!("Error recovery: clearing broken registration before re-pair");
                if let Err(e) = ps_clone.clear_registration() {
                    error!("Failed to clear registration: {}", e);
                }
                rt_clone.block_on(ps_clone.clear_api_token());
            }

            let recovered = request_pairing_code(window_weak_clone.clone(), ps_clone, rt_clone);
            if recovered {
                info!("Registration recovered during refresh, navigating to login");
                slint::invoke_from_event_loop(move || {
                    if let Some(window) = window_weak_clone.upgrade() {
                        window.set_current_screen("login".into());
                    }
                })
                .ok();
            }
        });
    });

    // Pairing completed callback
    let window_weak = window.as_weak();
    let ps_for_complete = Arc::clone(&pairing_service);
    window.on_setup_pairing_completed(move |terminal_id, _secret| {
        info!("Pairing completed: terminal_id={}", terminal_id);
        if let Some(w) = window_weak.upgrade() {
            // Update AppState with terminal code and company name from registration
            w.set_app_terminal_code(terminal_id.clone());
            if let Ok(Some(reg)) = ps_for_complete.get_registration() {
                if let Some(company) = reg.company_name {
                    info!("Setting company name: {}", company);
                    w.set_app_company_name(company.into());
                }
            }
            w.set_current_screen("login".into());
        }
    });

    // Server URL changed callback
    let rt_clone = Arc::clone(&runtime);
    let url_state_clone = Arc::clone(&url_state);
    window.on_setup_server_url_changed(move |new_url| {
        let url_str: String = new_url.into();
        info!("Server URL changed to: {}", url_str);

        let url_state = Arc::clone(&url_state_clone);
        let url_str_clone = url_str.clone();
        rt_clone.spawn(async move {
            let mut state = url_state.write().await;
            *state = url_str_clone;
        });
    });
}

/// Sets up feature-based navigation callbacks
fn setup_navigation_callbacks(window: &MainWindow, navigator: Arc<Mutex<Navigator>>) {
    // Check if a feature is enabled
    let nav_clone = Arc::clone(&navigator);
    window.on_is_feature_enabled(move |feature_id: slint::SharedString| {
        let feature_id_str: String = feature_id.into();
        let nav = nav_clone.lock();
        let enabled = nav.is_feature_enabled(&feature_id_str);
        debug!("Feature '{}' enabled: {}", feature_id_str, enabled);
        enabled
    });

    // Check if navigation to a screen is allowed
    let nav_clone = Arc::clone(&navigator);
    window.on_can_navigate_to(move |screen_id: slint::SharedString| {
        let screen_id_str: String = screen_id.into();
        let nav = nav_clone.lock();
        let can_navigate = nav.is_screen_available(&screen_id_str);
        debug!("Can navigate to '{}': {}", screen_id_str, can_navigate);
        can_navigate
    });

    // Navigate to screen with validation
    let nav_clone = Arc::clone(&navigator);
    let window_weak = window.as_weak();
    window.on_navigate_to_screen(move |screen_id: slint::SharedString| {
        let screen_id_str: String = screen_id.into();
        let mut nav = nav_clone.lock();

        match nav.navigate_to(&screen_id_str) {
            NavigationResult::Success { screen_id } => {
                info!("Navigation successful: {}", screen_id);
                if let Some(w) = window_weak.upgrade() {
                    w.set_current_screen(screen_id.into());
                }
                true
            }
            NavigationResult::Blocked { reason } => {
                warn!("Navigation blocked: {}", reason);
                false
            }
            NavigationResult::NotFound { screen_id } => {
                warn!("Screen not found: {}", screen_id);
                false
            }
        }
    });

    // Navigate to feature entry point
    let nav_clone = Arc::clone(&navigator);
    let window_weak = window.as_weak();
    window.on_navigate_to_feature(move |feature_id: slint::SharedString| {
        let feature_id_str: String = feature_id.into();
        let mut nav = nav_clone.lock();

        match nav.navigate_to_feature(&feature_id_str) {
            NavigationResult::Success { screen_id } => {
                info!("Feature navigation successful: {}", screen_id);
                if let Some(w) = window_weak.upgrade() {
                    w.set_current_screen(screen_id.into());
                }
                true
            }
            NavigationResult::Blocked { reason } => {
                warn!("Feature navigation blocked: {}", reason);
                false
            }
            NavigationResult::NotFound { .. } => {
                warn!("Feature entry point not found: {}", feature_id_str);
                false
            }
        }
    });
}

/// Sets up shutdown dialog callbacks
fn setup_shutdown_callbacks(window: &MainWindow) {
    // Close app
    window.on_request_close_app(move || {
        info!("User requested to close app");
        slint::quit_event_loop().ok();
    });

    // Restart app
    let system_service_restart = SystemService::new();
    window.on_request_restart_app(move || {
        info!("User requested to restart app");
        match system_service_restart.restart_app() {
            Ok(_) => {
                slint::quit_event_loop().ok();
            }
            Err(e) => {
                error!("Failed to restart app: {}", e);
            }
        }
    });

    // Restart machine
    let system_service_reboot = SystemService::new();
    window.on_request_restart_machine(move || {
        info!("User requested to restart machine");
        if let Err(e) = system_service_reboot.restart_machine() {
            error!("Failed to restart machine: {}", e);
        }
    });

    // Shutdown machine
    let system_service_shutdown = SystemService::new();
    window.on_request_shutdown_machine(move || {
        info!("User requested to shutdown machine");
        if let Err(e) = system_service_shutdown.shutdown_machine() {
            error!("Failed to shutdown machine: {}", e);
        }
    });
}

/// Sets up sync dialog callbacks for the settings sync screen
fn setup_sync_callbacks(
    window: &MainWindow,
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    runtime: Arc<Runtime>,
) {
    // Quick sync callback (from settings screen "Sync Now" button)
    let sync_service_now = Arc::clone(&sync_service);
    let sync_tx_now = Arc::clone(&sync_tx);
    let runtime_now = Arc::clone(&runtime);
    let window_weak_now = window.as_weak();

    window.on_sync_now(move || {
        info!("Quick sync requested from settings screen");
        let window_weak = window_weak_now.clone();
        let sync_service = Arc::clone(&sync_service_now);
        let sync_tx = Arc::clone(&sync_tx_now);
        let runtime = Arc::clone(&runtime_now);

        std::thread::spawn(move || {
            run_sync_with_ui_updates(window_weak, sync_service, sync_tx, runtime, false);
        });
    });

    // Full sync callback (from settings screen "Full Sync" button)
    let sync_service_full = Arc::clone(&sync_service);
    let sync_tx_full = Arc::clone(&sync_tx);
    let runtime_full = Arc::clone(&runtime);
    let window_weak_full = window.as_weak();

    window.on_full_sync(move || {
        info!("Full sync requested from settings screen");
        let window_weak = window_weak_full.clone();
        let sync_service = Arc::clone(&sync_service_full);
        let sync_tx = Arc::clone(&sync_tx_full);
        let runtime = Arc::clone(&runtime_full);

        std::thread::spawn(move || {
            run_sync_with_ui_updates(window_weak, sync_service, sync_tx, runtime, true);
        });
    });
}

/// Runs sync operation and updates the UI dialog with progress and results
fn run_sync_with_ui_updates(
    window_weak: slint::Weak<MainWindow>,
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    runtime: Arc<Runtime>,
    is_full_sync: bool,
) {
    // Subscribe to sync events
    let mut rx = sync_tx.subscribe();
    let tx = (*sync_tx).clone();

    // Track sync results
    let mut products_count = 0i32;
    let mut categories_count = 0i32;
    let mut operators_count = 0i32;
    let mut offline_synced = 0i32;
    let mut offline_failed = 0i32;
    let mut catalog_not_modified = false;
    let mut operators_not_modified = false;
    let mut sync_success = true;
    let mut error_message = String::new();

    // Spawn the actual sync
    let sync_service_clone = Arc::clone(&sync_service);
    let tx_clone = tx.clone();
    let runtime_clone = Arc::clone(&runtime);

    std::thread::spawn(move || {
        runtime_clone.block_on(async move {
            if is_full_sync {
                // For full sync, clear ETags first to force re-download
                if let Err(e) = sync_service_clone.force_full_sync(&tx_clone).await {
                    warn!("Full sync failed: {}", e);
                    let _ = tx_clone.send(SyncEvent::Failed(e.to_string()));
                } else {
                    let _ = tx_clone.send(SyncEvent::Completed);
                }
            } else if let Err(e) = sync_service_clone.sync_all(&tx_clone).await {
                warn!("Quick sync failed: {}", e);
                let _ = tx_clone.send(SyncEvent::Failed(e.to_string()));
            } else {
                let _ = tx_clone.send(SyncEvent::Completed);
            }
        });
    });

    // Process events and update UI
    let mut progress = 0.0f32;
    let status_steps = if is_full_sync {
        vec![
            (0.1, "جاري مسح الذاكرة المؤقتة..."),
            (0.2, "جاري تحميل المنتجات..."),
            (0.5, "جاري تحميل المشغلين..."),
            (0.7, "جاري معالجة المعاملات المعلقة..."),
            (0.9, "جاري الإنهاء..."),
        ]
    } else {
        vec![
            (0.2, "جاري تحميل المنتجات..."),
            (0.5, "جاري تحميل المشغلين..."),
            (0.7, "جاري معالجة المعاملات المعلقة..."),
            (0.9, "جاري الإنهاء..."),
        ]
    };
    let mut step_index = 0;

    runtime.block_on(async {
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(event)) => {
                    match event {
                        SyncEvent::Started => {
                            info!("Sync started");
                        }
                        SyncEvent::CatalogUpdated { products_count: p, categories_count: c } => {
                            info!("Catalog updated: {} products, {} categories", p, c);
                            products_count = p as i32;
                            categories_count = c as i32;
                            step_index = step_index.max(1);
                        }
                        SyncEvent::CatalogDeltaSynced { products_updated, products_deleted, categories_updated, categories_deleted } => {
                            info!("Catalog delta synced: {} updated, {} deleted, {} categories updated, {} categories deleted",
                                products_updated, products_deleted, categories_updated, categories_deleted);
                            products_count = (products_updated + products_deleted) as i32;
                            categories_count = (categories_updated + categories_deleted) as i32;
                            step_index = step_index.max(1);
                        }
                        SyncEvent::CatalogNotModified => {
                            info!("Catalog not modified");
                            catalog_not_modified = true;
                            step_index = step_index.max(1);
                        }
                        SyncEvent::OperatorsUpdated { count } => {
                            info!("Operators updated: {}", count);
                            operators_count = count as i32;
                            step_index = step_index.max(2);
                        }
                        SyncEvent::OperatorsNotModified => {
                            info!("Operators not modified");
                            operators_not_modified = true;
                            step_index = step_index.max(2);
                        }
                        SyncEvent::OfflineQueueProcessed { synced, failed, conflicts } => {
                            info!("Offline queue: {} synced, {} failed, {} conflicts", synced, failed, conflicts);
                            offline_synced = synced as i32;
                            offline_failed = failed as i32;
                            step_index = step_index.max(3);
                        }
                        SyncEvent::OfflineQueueEmpty => {
                            info!("Offline queue empty");
                            step_index = step_index.max(3);
                        }
                        SyncEvent::ConflictDetected { transaction_id, error } => {
                            warn!("Conflict detected: {} - {}", transaction_id, error);
                        }
                        SyncEvent::ConflictsDetected { count } => {
                            warn!("{} conflicts detected - manager resolution required", count);
                        }
                        SyncEvent::FeaturesUpdated { count: _ } | SyncEvent::FeaturesNotModified | SyncEvent::ScreensUpdated { count: _ } => {
                            // Ignore these events for UI
                        }
                        SyncEvent::DraftsSynced { synced, failed } => {
                            info!("Drafts synced: {} synced, {} failed", synced, failed);
                            // Drafts sync is handled in background, no UI update needed
                        }
                        SyncEvent::DraftsQueueEmpty => {
                            debug!("No pending draft sync operations");
                        }
                        SyncEvent::Completed => {
                            info!("Sync completed successfully");
                            progress = 1.0;
                            break;
                        }
                        SyncEvent::Failed(err) => {
                            error!("Sync failed: {}", err);
                            sync_success = false;
                            error_message = err;
                            break;
                        }
                        SyncEvent::Offline => {
                            warn!("Terminal is offline");
                            sync_success = false;
                            error_message = "الجهاز غير متصل بالإنترنت".to_string();
                            break;
                        }
                        SyncEvent::TerminalNotRegistered => {
                            error!("Terminal not registered - needs re-pairing");
                            sync_success = false;
                            error_message = "الجهاز غير مسجل - يرجى إعادة الربط".to_string();
                            break;
                        }
                    }

                    // Update progress based on step
                    if step_index < status_steps.len() {
                        progress = status_steps[step_index].0;
                        let status = status_steps[step_index].1;

                        // Update UI
                        let window_weak_clone = window_weak.clone();
                        let status_str = status.to_string();
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = window_weak_clone.upgrade() {
                                w.set_sync_dialog_progress(progress);
                                w.set_sync_dialog_status(status_str.into());
                            }
                        }).ok();
                    }
                }
                Ok(Err(_)) => {
                    // Channel closed
                    break;
                }
                Err(_) => {
                    // Timeout - continue waiting
                }
            }
        }
    });

    // Update UI with final results
    let window_weak_final = window_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(w) = window_weak_final.upgrade() {
            w.set_sync_dialog_in_progress(false);
            w.set_sync_dialog_success(sync_success);
            w.set_sync_dialog_error(error_message.into());
            w.set_sync_products_count(products_count);
            w.set_sync_categories_count(categories_count);
            w.set_sync_operators_count(operators_count);
            w.set_sync_offline_synced(offline_synced);
            w.set_sync_offline_failed(offline_failed);
            w.set_sync_catalog_not_modified(catalog_not_modified);
            w.set_sync_operators_not_modified(operators_not_modified);
            w.set_sync_dialog_progress(1.0);
            w.set_sync_dialog_status(if sync_success {
                "اكتملت المزامنة".into()
            } else {
                "فشلت المزامنة".into()
            });
        }
    })
    .ok();

    info!("Sync UI update complete");
}

/// Sets up catalog callbacks for category selection and product add-to-cart
fn setup_catalog_callbacks(
    window: &MainWindow,
    db: Arc<Database>,
    cart_service: Arc<CartService>,
    currency: String,
) {
    // Build shared category color map for use in all product conversions
    let category_color_map: Arc<HashMap<String, (u8, u8, u8)>> = {
        let product_service = ProductService::new(Arc::clone(&db));
        Arc::new(match product_service.categories() {
            Ok(cats) => cats
                .iter()
                .enumerate()
                .map(|(idx, c)| {
                    let hex = c
                        .color
                        .as_deref()
                        .filter(|h| !h.is_empty())
                        .unwrap_or_else(|| {
                            supermarket_category_color(
                                &c.name,
                                c.name_ar.as_deref().unwrap_or(""),
                                idx,
                            )
                        });
                    (c.id.clone(), parse_hex_color(hex))
                })
                .collect(),
            Err(_) => HashMap::new(),
        })
    };

    // Category selection callback - filters products by category
    let window_weak = window.as_weak();
    let db_for_category = Arc::clone(&db);
    let currency_for_category = currency.clone();
    let color_map_for_category = Arc::clone(&category_color_map);
    window.on_select_category(move |category_id: SharedString| {
        let category_id_str: String = category_id.into();
        info!(
            "Category selected: {}",
            if category_id_str.is_empty() {
                "All"
            } else {
                &category_id_str
            }
        );

        let window_weak_clone = window_weak.clone();
        let product_service = ProductService::new(Arc::clone(&db_for_category));
        let color_map = Arc::clone(&color_map_for_category);

        // Filter products by category (or get all if empty)
        let products = if category_id_str.is_empty() {
            product_service.all_products(500, 0)
        } else {
            product_service.by_category(&category_id_str, 500, 0)
        };

        match products {
            Ok(products) => {
                let product_count = products.len();
                info!("Filtered to {} products", product_count);

                // Convert to Slint ProductData
                let cur = &currency_for_category;
                let slint_products: Vec<ProductData> = products
                    .into_iter()
                    .map(|prod| {
                        let stock = prod.stock_qty;
                        let min_stock = prod.min_stock;
                        let (r, g, b) = color_map
                            .get(prod.category_id.as_deref().unwrap_or(""))
                            .copied()
                            .unwrap_or((59, 130, 246));

                        ProductData {
                            id: SharedString::from(&prod.id),
                            name: SharedString::from(&prod.name),
                            name_ar: SharedString::from(
                                prod.name_ar.as_deref().unwrap_or(&prod.name),
                            ),
                            sku: SharedString::from(&prod.sku),
                            barcode: SharedString::from(prod.barcode.as_deref().unwrap_or("")),
                            price: SharedString::from(fmt_decimal(prod.price, cur)),
                            stock,
                            low_stock: stock > 0 && stock <= min_stock,
                            out_of_stock: stock <= 0,
                            has_image: prod.image_url.is_some(),
                            image_url: SharedString::from(prod.image_url.as_deref().unwrap_or("")),
                            category_id: SharedString::from(
                                prod.category_id.as_deref().unwrap_or(""),
                            ),
                            unit: SharedString::from(format!("{:?}", prod.unit).to_lowercase()),
                            category_color: slint::Color::from_rgb_u8(r, g, b),
                            is_weighable: prod.is_weighable,
                            product_type: SharedString::from(prod.product_type.as_str()),
                            is_service: !prod.track_inventory,
                        }
                    })
                    .collect();

                // Update UI on main thread
                slint::invoke_from_event_loop(move || {
                    if let Some(w) = window_weak_clone.upgrade() {
                        let model = ModelRc::new(VecModel::from(slint_products));
                        w.set_products(model);
                    }
                })
                .ok();
            }
            Err(e) => {
                warn!("Failed to filter products by category: {}", e);
            }
        }
    });

    // Add to cart callback - adds product to cart by ID
    // For weighable items, opens the weight pad instead of adding directly
    let window_weak_for_cart = window.as_weak();
    let cart_service_for_add = Arc::clone(&cart_service);
    let db_for_add = Arc::clone(&db);
    let currency_for_cart = currency.clone();
    window.on_add_to_cart(move |product_id: SharedString| {
        let product_id_str: String = product_id.into();
        info!("Adding product to cart: {}", product_id_str);

        let product_service = ProductService::new(Arc::clone(&db_for_add));
        let cur = &currency_for_cart;

        // Get the product by ID
        match product_service.get_by_id(&product_id_str) {
            Ok(Some(product)) => {
                // Check if product requires weighing
                if product.is_weighable {
                    info!(
                        "Product {} requires weighing — opening weight pad",
                        product.name
                    );

                    let display_name = product
                        .name_ar
                        .as_deref()
                        .unwrap_or(&product.name)
                        .to_string();
                    let unit_label = product.unit.label("ar").to_string();
                    let price_str =
                        format!("{} {}/{}", fmt_decimal(product.price, cur), cur, unit_label);
                    let prod_id = product.id.clone();

                    let ww = window_weak_for_cart.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_weight_pad_product_name(SharedString::from(&display_name));
                            w.set_weight_pad_price_per_unit(SharedString::from(&price_str));
                            w.set_weight_pad_product_unit(SharedString::from(&unit_label));
                            w.set_weight_pad_product_id(SharedString::from(&prod_id));
                            w.set_weight_pad_manual_value(SharedString::from(""));
                            w.set_weight_pad_error(SharedString::from(""));
                            w.set_weight_pad_weight_display(SharedString::from("0.000"));
                            w.set_weight_pad_computed_total(SharedString::from("0.000"));
                            w.set_weight_pad_weight_stable(false);
                            // Use manual mode if scale is not connected
                            w.set_weight_pad_manual_mode(!w.get_weight_pad_scale_connected());
                            w.set_weight_pad_open(true);
                        }
                    })
                    .ok();
                    return;
                }

                // Non-weighable: add to cart with quantity 1
                match cart_service_for_add.add_item(&product, rust_decimal::Decimal::ONE) {
                    Ok(_) => {
                        info!("Added {} to cart", product.name);

                        // Last scanned item info for display feedback
                        let last_name = product
                            .name_ar
                            .as_deref()
                            .unwrap_or(&product.name)
                            .to_string();
                        let last_price = fmt_decimal(product.price, cur);

                        // Refresh cart UI using shared helper
                        refresh_cart_ui(&window_weak_for_cart, &cart_service_for_add, cur);

                        // Set last scanned item display (clear void feedback first)
                        let window_weak_clone = window_weak_for_cart.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = window_weak_clone.upgrade() {
                                w.set_show_void_feedback(false);
                                w.set_last_scanned_name(SharedString::from(&last_name));
                                w.set_last_scanned_price(SharedString::from(&last_price));
                                w.set_show_last_scanned(true);
                            }
                        })
                        .ok();

                        // Auto-clear last scanned display after 5 seconds
                        let window_weak_timer = window_weak_for_cart.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_secs(5));
                            slint::invoke_from_event_loop(move || {
                                if let Some(w) = window_weak_timer.upgrade() {
                                    w.set_show_last_scanned(false);
                                }
                            })
                            .ok();
                        });
                    }
                    Err(e) => {
                        warn!("Failed to add product to cart: {}", e);
                    }
                }
            }
            Ok(None) => {
                warn!("Product not found: {}", product_id_str);
            }
            Err(e) => {
                warn!("Failed to get product: {}", e);
            }
        }
    });

    // PLU lookup callback — searches by PLU/barcode code and sets result properties
    let window_weak_plu = window.as_weak();
    let db_for_plu = Arc::clone(&db);
    let cart_service_for_plu = Arc::clone(&cart_service);
    let currency_for_plu = currency.clone();
    window.on_plu_lookup(move |plu_code: SharedString| {
        let code: String = plu_code.into();
        info!("PLU lookup: {}", code);

        let window_weak_clone = window_weak_plu.clone();
        let product_service = ProductService::new(Arc::clone(&db_for_plu));
        let cart_svc = Arc::clone(&cart_service_for_plu);
        let cur = currency_for_plu.clone();

        // Set searching state
        let ww = window_weak_plu.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(w) = ww.upgrade() {
                w.set_plu_searching(true);
                w.set_plu_error_message(SharedString::from(""));
                w.set_plu_result_count(0);
            }
        })
        .ok();

        // Perform search (barcode exact match first, then FTS5)
        match product_service.search(&code, 5) {
            Ok(result) => {
                let count = result.products.len();
                info!("PLU search found {} products for code '{}'", count, code);

                if count == 0 {
                    // No match — show error
                    let error_msg = format!("PLU {} — Product not found", code);
                    let ww2 = window_weak_clone.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww2.upgrade() {
                            w.set_plu_searching(false);
                            w.set_plu_result_count(0);
                            w.set_plu_error_message(SharedString::from(&error_msg));
                        }
                    })
                    .ok();
                } else if count == 1 {
                    // Exact match — auto-add to cart
                    let product = &result.products[0];
                    let product_name = product
                        .name_ar
                        .as_deref()
                        .unwrap_or(&product.name)
                        .to_string();
                    let product_price = fmt_decimal(product.price, &cur);

                    // Add to cart
                    match cart_svc.add_item(product, rust_decimal::Decimal::ONE) {
                        Ok(_) => {
                            info!("PLU: Added {} to cart", product.name);

                            // Refresh cart UI
                            refresh_cart_ui(&window_weak_clone, &cart_svc, &cur);

                            // Set result display + last scanned feedback
                            let name = product_name.clone();
                            let price = product_price.clone();
                            let ww2 = window_weak_clone.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(w) = ww2.upgrade() {
                                    w.set_plu_searching(false);
                                    w.set_plu_result_count(1);
                                    w.set_plu_result_name(SharedString::from(&name));
                                    w.set_plu_result_price(SharedString::from(&price));
                                    // Show last scanned feedback (clear void feedback first)
                                    w.set_show_void_feedback(false);
                                    w.set_last_scanned_name(SharedString::from(&name));
                                    w.set_last_scanned_price(SharedString::from(&price));
                                    w.set_show_last_scanned(true);
                                }
                            })
                            .ok();

                            // Auto-clear last scanned after 5 seconds
                            let ww_timer = window_weak_clone.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_secs(5));
                                slint::invoke_from_event_loop(move || {
                                    if let Some(w) = ww_timer.upgrade() {
                                        w.set_show_last_scanned(false);
                                    }
                                })
                                .ok();
                            });
                        }
                        Err(e) => {
                            warn!("PLU: Failed to add product to cart: {}", e);
                            let error_msg = format!("Failed to add: {}", e);
                            let ww2 = window_weak_clone.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(w) = ww2.upgrade() {
                                    w.set_plu_searching(false);
                                    w.set_plu_result_count(0);
                                    w.set_plu_error_message(SharedString::from(&error_msg));
                                }
                            })
                            .ok();
                        }
                    }
                } else {
                    // Multiple matches — show first result with count
                    let first = &result.products[0];
                    let product_name = first.name_ar.as_deref().unwrap_or(&first.name).to_string();
                    let product_price = fmt_decimal(first.price, &cur);
                    let product_id = first.id.clone();
                    let result_count = count as i32;

                    let ww2 = window_weak_clone.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww2.upgrade() {
                            w.set_plu_searching(false);
                            w.set_plu_result_count(result_count);
                            w.set_plu_result_name(SharedString::from(&product_name));
                            w.set_plu_result_price(SharedString::from(&product_price));
                            w.set_plu_result_product_id(SharedString::from(&product_id));
                        }
                    })
                    .ok();
                }
            }
            Err(e) => {
                warn!("PLU search failed: {}", e);
                let error_msg = format!("Search error: {}", e);
                let ww2 = window_weak_clone.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww2.upgrade() {
                        w.set_plu_searching(false);
                        w.set_plu_result_count(0);
                        w.set_plu_error_message(SharedString::from(&error_msg));
                    }
                })
                .ok();
            }
        }
    });

    // PLU add-product callback — adds a specific product by ID (for multi-result selection)
    let window_weak_plu_add = window.as_weak();
    let db_for_plu_add = Arc::clone(&db);
    let cart_service_for_plu_add = Arc::clone(&cart_service);
    let currency_for_plu_add = currency.clone();
    window.on_plu_add_product(move |product_id: SharedString| {
        let product_id_str: String = product_id.into();
        info!("PLU add product: {}", product_id_str);

        let product_service = ProductService::new(Arc::clone(&db_for_plu_add));
        let cur = &currency_for_plu_add;

        match product_service.get_by_id(&product_id_str) {
            Ok(Some(product)) => {
                match cart_service_for_plu_add.add_item(&product, rust_decimal::Decimal::ONE) {
                    Ok(_) => {
                        info!("PLU: Added {} to cart (multi-result)", product.name);

                        let last_name = product
                            .name_ar
                            .as_deref()
                            .unwrap_or(&product.name)
                            .to_string();
                        let last_price = fmt_decimal(product.price, cur);

                        refresh_cart_ui(&window_weak_plu_add, &cart_service_for_plu_add, cur);

                        // Set last scanned feedback (clear void feedback first)
                        let ww = window_weak_plu_add.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_show_void_feedback(false);
                                w.set_last_scanned_name(SharedString::from(&last_name));
                                w.set_last_scanned_price(SharedString::from(&last_price));
                                w.set_show_last_scanned(true);
                            }
                        })
                        .ok();

                        // Auto-clear last scanned after 5 seconds
                        let ww_timer = window_weak_plu_add.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_secs(5));
                            slint::invoke_from_event_loop(move || {
                                if let Some(w) = ww_timer.upgrade() {
                                    w.set_show_last_scanned(false);
                                }
                            })
                            .ok();
                        });
                    }
                    Err(e) => {
                        warn!("PLU: Failed to add product: {}", e);
                    }
                }
            }
            Ok(None) => {
                warn!("PLU: Product not found: {}", product_id_str);
            }
            Err(e) => {
                warn!("PLU: Failed to get product: {}", e);
            }
        }
    });

    info!("Catalog callbacks set up successfully");
}

/// Sets up price check callbacks — read-only product lookup without cart interaction
fn setup_price_check_callbacks(window: &MainWindow, db: Arc<Database>, currency: String) {
    let window_weak = window.as_weak();
    let db_clone = Arc::clone(&db);
    let cur = currency.clone();

    window.on_price_check_lookup(move |query: SharedString| {
        let query_str: String = query.into();
        info!("Price check lookup: {}", query_str);

        let product_service = ProductService::new(Arc::clone(&db_clone));
        let currency = cur.clone();

        // Set searching state
        let ww = window_weak.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(w) = ww.upgrade() {
                w.set_price_check_searching(true);
                w.set_price_check_error(SharedString::from(""));
                w.set_price_check_name(SharedString::from(""));
                w.set_price_check_open(true);
            }
        })
        .ok();

        // Try lookup: by ID first, then barcode, then search
        let product = product_service
            .get_by_id(&query_str)
            .ok()
            .flatten()
            .or_else(|| product_service.lookup_barcode(&query_str).ok().flatten())
            .or_else(|| {
                product_service
                    .search(&query_str, 1)
                    .ok()
                    .and_then(|r| r.products.into_iter().next())
            });

        let ww2 = window_weak.clone();

        match product {
            Some(product) => {
                let name = product
                    .name_ar
                    .as_deref()
                    .unwrap_or(&product.name)
                    .to_string();
                let price = format!("{} {}", fmt_decimal(product.price, &currency), currency);
                let barcode = product.barcode.clone().unwrap_or_default();
                let sku = product.sku.clone();
                let category = product.category_name.clone().unwrap_or_default();
                let stock = product.stock_qty;
                let unit_label = product.unit.label("en").to_string();
                let in_stock = product.is_in_stock();
                let low_stock = stock > 0 && stock <= product.min_stock;

                // Format tax info
                let tax_info = if product.tax_inclusive {
                    format!("{}% (incl.)", product.tax_rate)
                } else {
                    format!("{}%", product.tax_rate)
                };

                info!("Price check found: {} — {}", name, price);

                slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww2.upgrade() {
                        w.set_price_check_searching(false);
                        w.set_price_check_name(SharedString::from(&name));
                        w.set_price_check_price(SharedString::from(&price));
                        w.set_price_check_barcode(SharedString::from(&barcode));
                        w.set_price_check_sku(SharedString::from(&sku));
                        w.set_price_check_category(SharedString::from(&category));
                        w.set_price_check_stock(stock);
                        w.set_price_check_unit(SharedString::from(&unit_label));
                        w.set_price_check_in_stock(in_stock);
                        w.set_price_check_low_stock(low_stock);
                        w.set_price_check_tax_info(SharedString::from(&tax_info));
                        w.set_price_check_error(SharedString::from(""));
                    }
                })
                .ok();
            }
            None => {
                let error_msg = if query_str.chars().all(|c| c.is_ascii_digit()) {
                    format!("{} — Product not found", query_str)
                } else {
                    "Product not found".to_string()
                };
                warn!("Price check: product not found for '{}'", query_str);

                slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww2.upgrade() {
                        w.set_price_check_searching(false);
                        w.set_price_check_name(SharedString::from(""));
                        w.set_price_check_error(SharedString::from(&error_msg));
                    }
                })
                .ok();
            }
        }
    });

    info!("Price check callbacks set up successfully");
}

/// Sets up cart item operation callbacks (increment, decrement, remove, clear, void-last, qty-multiply)
fn setup_cart_item_callbacks(
    window: &MainWindow,
    cart_service: Arc<CartService>,
    currency: String,
    db: Arc<Database>,
) {
    // --- Increment item quantity ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_cart_item_increment(move |item_id: SharedString| {
        let id: String = item_id.into();
        info!("Incrementing cart item: {}", id);
        match cart_svc.increment(&id) {
            Ok(()) => {
                refresh_cart_ui(&window_weak, &cart_svc, &cur);
            }
            Err(e) => warn!("Failed to increment item {}: {}", id, e),
        }
    });

    // --- Decrement item quantity ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_cart_item_decrement(move |item_id: SharedString| {
        let id: String = item_id.into();
        info!("Decrementing cart item: {}", id);
        match cart_svc.decrement(&id) {
            Ok(()) => {
                refresh_cart_ui(&window_weak, &cart_svc, &cur);
            }
            Err(e) => warn!("Failed to decrement item {}: {}", id, e),
        }
    });

    // --- Remove item from cart ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_cart_item_remove(move |item_id: SharedString| {
        let id: String = item_id.into();
        info!("Removing cart item: {}", id);
        match cart_svc.remove_item(&id) {
            Ok(()) => {
                refresh_cart_ui(&window_weak, &cart_svc, &cur);
            }
            Err(e) => warn!("Failed to remove item {}: {}", id, e),
        }
    });

    // --- Clear entire cart ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_clear_cart(move || {
        info!("Clearing cart");
        cart_svc.clear();
        refresh_cart_ui(&window_weak, &cart_svc, &cur);
    });

    // --- Void last item (remove the most recently added item) ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_void_last_item(move || {
        let cart = cart_svc.get_cart();
        if let Some(last_item) = cart.items.last() {
            // Capture voided item info BEFORE removal
            let voided_name = last_item
                .product_name_ar
                .clone()
                .unwrap_or_else(|| last_item.product_name.clone());
            let voided_price = fmt_decimal(last_item.unit_price, &cur);
            let last_id = last_item.id.clone();
            info!(
                "Voiding last item: {} ({})",
                last_item.product_name, last_id
            );
            match cart_svc.remove_item(&last_id) {
                Ok(()) => {
                    refresh_cart_ui(&window_weak, &cart_svc, &cur);

                    // Show void feedback (clear any "Added" bar first)
                    let ww = window_weak.clone();
                    let name = voided_name;
                    let price = voided_price;
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_show_last_scanned(false);
                            w.set_void_item_name(SharedString::from(&name));
                            w.set_void_item_price(SharedString::from(&price));
                            w.set_show_void_feedback(true);
                        }
                    })
                    .ok();

                    // Auto-clear void feedback after 3 seconds
                    let ww_timer = window_weak.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(3));
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww_timer.upgrade() {
                                w.set_show_void_feedback(false);
                            }
                        })
                        .ok();
                    });
                }
                Err(e) => warn!("Failed to void last item: {}", e),
            }
        } else {
            debug!("Void last item: cart is empty, nothing to void");
        }
    });

    // --- Quantity multiply (opens quantity pad with last item's context) ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_quantity_multiply(move || {
        let cart = cart_svc.get_cart();
        if let Some(last_item) = cart.items.last() {
            let name = last_item
                .product_name_ar
                .clone()
                .unwrap_or_else(|| last_item.product_name.clone());
            let price = fmt_decimal(last_item.unit_price, &cur);
            let qty = last_item.quantity.to_i32().unwrap_or(1);
            let id = last_item.id.clone();

            info!(
                "Opening quantity pad for last item: {} ({})",
                last_item.product_name, id
            );

            if let Some(w) = window_weak.upgrade() {
                w.set_qty_pad_item_id(SharedString::from(&id));
                w.set_qty_pad_item_name(SharedString::from(&name));
                w.set_qty_pad_item_price(SharedString::from(&price));
                w.set_qty_pad_current_qty(qty);
                w.set_qty_pad_value(SharedString::from(""));
                w.set_qty_pad_error(SharedString::from(""));
                w.set_qty_pad_open(true);
            }
        } else {
            debug!("Quantity multiply: cart is empty, nothing to multiply");
        }
    });

    // --- Cart item edit (opens quantity pad for a specific item) ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_cart_item_edit(move |item_id: SharedString| {
        let id: String = item_id.into();
        let cart = cart_svc.get_cart();
        if let Some(item) = cart.items.iter().find(|i| i.id == id) {
            let name = item
                .product_name_ar
                .clone()
                .unwrap_or_else(|| item.product_name.clone());
            let price = fmt_decimal(item.unit_price, &cur);
            let qty = item.quantity.to_i32().unwrap_or(1);

            info!(
                "Opening quantity pad for item: {} ({})",
                item.product_name, id
            );

            if let Some(w) = window_weak.upgrade() {
                w.set_qty_pad_item_id(SharedString::from(&id));
                w.set_qty_pad_item_name(SharedString::from(&name));
                w.set_qty_pad_item_price(SharedString::from(&price));
                w.set_qty_pad_current_qty(qty);
                w.set_qty_pad_value(SharedString::from(""));
                w.set_qty_pad_error(SharedString::from(""));
                w.set_qty_pad_open(true);
            }
        } else {
            warn!("Cart item edit: item {} not found", id);
        }
    });

    // --- Quantity confirmed (updates item quantity from quantity pad) ---
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let cur = currency.clone();
    window.on_quantity_confirmed(move |item_id: SharedString, qty_str: SharedString| {
        let id: String = item_id.into();
        let qty_s: String = qty_str.into();

        info!("Quantity confirmed: item={}, qty={}", id, qty_s);

        match qty_s.parse::<i32>() {
            Ok(qty) if qty > 0 && qty <= 999 => {
                let qty_dec = rust_decimal::Decimal::from(qty);
                match cart_svc.update_quantity(&id, qty_dec) {
                    Ok(()) => {
                        info!("Updated item {} quantity to {}", id, qty);
                        refresh_cart_ui(&window_weak, &cart_svc, &cur);
                        // Close the pad
                        let ww = window_weak.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_qty_pad_open(false);
                                w.set_qty_pad_value(SharedString::from(""));
                                w.set_qty_pad_error(SharedString::from(""));
                            }
                        })
                        .ok();
                    }
                    Err(e) => {
                        warn!("Failed to update quantity: {}", e);
                        let ww = window_weak.clone();
                        let error_msg = format!("{}", e);
                        slint::invoke_from_event_loop(move || {
                            if let Some(w) = ww.upgrade() {
                                w.set_qty_pad_error(SharedString::from(&error_msg));
                            }
                        })
                        .ok();
                    }
                }
            }
            _ => {
                let ww = window_weak.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(w) = ww.upgrade() {
                        w.set_qty_pad_error(SharedString::from("Invalid quantity (1-999)"));
                    }
                })
                .ok();
            }
        }
    });

    // Weight confirmed callback — adds a weighable item to cart with the given weight
    let window_weak = window.as_weak();
    let cart_svc = Arc::clone(&cart_service);
    let db_for_weight = Arc::clone(&db);
    let cur = currency.to_string();
    window.on_weight_confirmed(move |weight_str: SharedString| {
        let ws: String = weight_str.into();
        info!("Weight confirmed: {}", ws);

        let window_weak = window_weak.clone();
        let cart_svc = Arc::clone(&cart_svc);
        let db = Arc::clone(&db_for_weight);
        let cur = cur.clone();

        // Slint callbacks run on the main thread, so we can upgrade directly
        if let Some(w) = window_weak.upgrade() {
            let product_id: String = w.get_weight_pad_product_id().into();

            match ws.parse::<rust_decimal::Decimal>() {
                Ok(weight) if weight > rust_decimal::Decimal::ZERO => {
                    let product_service = ProductService::new(Arc::clone(&db));
                    match product_service.get_by_id(&product_id) {
                        Ok(Some(product)) => {
                            match cart_svc.add_item(&product, weight) {
                                Ok(_) => {
                                    info!(
                                        "Added weighable item {} with weight {} to cart",
                                        product.name, weight
                                    );

                                    let last_name = product
                                        .name_ar
                                        .as_deref()
                                        .unwrap_or(&product.name)
                                        .to_string();
                                    let last_price = fmt_decimal(product.price * weight, &cur);

                                    // Refresh cart UI
                                    refresh_cart_ui(&window_weak, &cart_svc, &cur);

                                    // Close weight pad and show feedback
                                    let ww = window_weak.clone();
                                    slint::invoke_from_event_loop(move || {
                                        if let Some(w) = ww.upgrade() {
                                            w.set_weight_pad_open(false);
                                            w.set_weight_pad_manual_value(SharedString::from(""));
                                            w.set_show_void_feedback(false);
                                            w.set_last_scanned_name(SharedString::from(&last_name));
                                            w.set_last_scanned_price(SharedString::from(
                                                &last_price,
                                            ));
                                            w.set_show_last_scanned(true);
                                        }
                                    })
                                    .ok();

                                    // Auto-clear last scanned display after 5 seconds
                                    let ww_timer = window_weak.clone();
                                    std::thread::spawn(move || {
                                        std::thread::sleep(Duration::from_secs(5));
                                        slint::invoke_from_event_loop(move || {
                                            if let Some(w) = ww_timer.upgrade() {
                                                w.set_show_last_scanned(false);
                                            }
                                        })
                                        .ok();
                                    });
                                }
                                Err(e) => {
                                    warn!("Failed to add weighable item to cart: {}", e);
                                    let ww = window_weak.clone();
                                    let error_msg = format!("{}", e);
                                    slint::invoke_from_event_loop(move || {
                                        if let Some(w) = ww.upgrade() {
                                            w.set_weight_pad_error(SharedString::from(&error_msg));
                                        }
                                    })
                                    .ok();
                                }
                            }
                        }
                        Ok(None) => {
                            warn!("Weighable product not found: {}", product_id);
                        }
                        Err(e) => {
                            warn!("Failed to get weighable product: {}", e);
                        }
                    }
                }
                Ok(_) => {
                    // Weight is zero or negative
                    let ww = window_weak.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_weight_pad_error(SharedString::from(
                                "Weight must be greater than zero",
                            ));
                        }
                    })
                    .ok();
                }
                Err(_) => {
                    let ww = window_weak.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = ww.upgrade() {
                            w.set_weight_pad_error(SharedString::from("Invalid weight value"));
                        }
                    })
                    .ok();
                }
            }
        }
    });

    // Weight tare requested callback
    window.on_weight_tare_requested(move || {
        info!("Scale tare requested");
        // Scale tare is handled by ScaleService (when hardware feature is enabled)
        // For now, just log the request - ScaleService integration hooks in at init time
    });

    info!("Cart item callbacks set up successfully");
}

/// Sets up draft save/recall callbacks
fn setup_draft_callbacks(
    window: &MainWindow,
    draft_service: Arc<DraftService>,
    cart_service: Arc<CartService>,
) {
    // Save draft callback - saves current cart as a draft
    let window_weak = window.as_weak();
    let draft_service_for_save = Arc::clone(&draft_service);
    let cart_service_for_save = Arc::clone(&cart_service);

    window.on_save_draft(
        move |name: SharedString, operator_id: SharedString, shift_id: SharedString| {
            let name_str: String = name.into();
            let operator_id_str: String = operator_id.into();
            let shift_id_str: String = shift_id.into();

            info!(
                "Save draft requested: name='{}', operator='{}', shift='{}'",
                if name_str.is_empty() {
                    "<none>"
                } else {
                    &name_str
                },
                if operator_id_str.is_empty() {
                    "<none>"
                } else {
                    &operator_id_str
                },
                if shift_id_str.is_empty() {
                    "<none>"
                } else {
                    &shift_id_str
                }
            );

            // Get current cart
            let cart = cart_service_for_save.get_cart();

            if cart.is_empty() {
                warn!("Cannot save empty cart as draft");
                // Update UI to show error
                let window_weak_clone = window_weak.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(w) = window_weak_clone.upgrade() {
                        w.set_draft_is_saving(false);
                        // Keep dialog open to show error - user can cancel
                    }
                })
                .ok();
                return;
            }

            // Use actual operator/shift IDs from UI, with fallbacks
            let operator_id = if operator_id_str.is_empty() {
                "operator-default"
            } else {
                &operator_id_str
            };
            let shift_id = if shift_id_str.is_empty() {
                "shift-default"
            } else {
                &shift_id_str
            };

            let draft_name = if name_str.is_empty() {
                None
            } else {
                Some(name_str)
            };

            match draft_service_for_save.save_cart(&cart, draft_name, operator_id, shift_id) {
                Ok(draft) => {
                    info!(
                        "Draft saved successfully: {} ({})",
                        draft.display_name(),
                        draft.id
                    );

                    // Clear the cart after successful save
                    cart_service_for_save.clear();

                    // Update UI
                    let window_weak_clone = window_weak.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = window_weak_clone.upgrade() {
                            // Update draft count
                            let current_count = w.get_draft_count();
                            w.set_draft_count(current_count + 1);

                            // Close dialog and reset state
                            w.set_show_save_draft_dialog(false);
                            w.set_draft_is_saving(false);
                            w.set_draft_name(SharedString::from(""));

                            // Clear cart UI
                            let empty_items: Vec<CartItemData> = vec![];
                            w.set_cart_items(ModelRc::new(VecModel::from(empty_items)));
                            w.set_cart_subtotal(SharedString::from("0.000"));
                            w.set_cart_tax(SharedString::from("0.000"));
                            w.set_cart_discount(SharedString::from("0.000"));
                            w.set_cart_total(SharedString::from("0.000"));
                        }
                    })
                    .ok();
                }
                Err(e) => {
                    error!("Failed to save draft: {}", e);

                    // Update UI to show error
                    let window_weak_clone = window_weak.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(w) = window_weak_clone.upgrade() {
                            w.set_draft_is_saving(false);
                            // Keep dialog open so user can retry or cancel
                        }
                    })
                    .ok();
                }
            }
        },
    );

    info!("Draft callbacks set up successfully");
}

/// Inputs required to run the startup sequence on the splash thread.
struct StartupContext {
    window_weak: slint::Weak<MainWindow>,
    pairing_service: Arc<PairingService>,
    runtime: Arc<Runtime>,
    db: Arc<Database>,
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    is_registered: bool,
    currency: String,
}

/// Runs the startup sequence with splash screen
fn run_startup_sequence(ctx: StartupContext) {
    let StartupContext {
        window_weak,
        pairing_service,
        runtime,
        db,
        sync_service,
        sync_tx,
        is_registered,
        currency,
    } = ctx;

    // Loading steps
    let steps = [
        (0.0, "Starting..."),
        (0.2, "Connecting to server..."),
        (0.4, "Loading settings..."),
        (0.6, "Syncing products..."),
        (0.8, "Preparing interface..."),
        (1.0, "Ready!"),
    ];

    for (progress, status) in steps {
        std::thread::sleep(Duration::from_millis(400));

        let window_weak_clone = window_weak.clone();
        let status_str = status.to_string();

        slint::invoke_from_event_loop(move || {
            if let Some(window) = window_weak_clone.upgrade() {
                window.set_splash_progress(progress);
                window.set_splash_status(status_str.into());
            }
        })
        .ok();
    }

    // Navigate to appropriate screen
    let window_weak_clone = window_weak.clone();
    if is_registered {
        info!("Terminal registered, navigating to login screen");

        // Load catalog data from database (initial load)
        populate_catalog_data(window_weak.clone(), Arc::clone(&db), currency.clone());

        // Load operators from database (for login screen)
        populate_operators_data(window_weak.clone(), Arc::clone(&db));

        // Start background sync service
        start_background_sync(
            window_weak.clone(),
            Arc::clone(&db),
            Arc::clone(&sync_service),
            Arc::clone(&sync_tx),
            Arc::clone(&runtime),
            Arc::clone(&pairing_service),
            currency.clone(),
        );

        slint::invoke_from_event_loop(move || {
            if let Some(window) = window_weak_clone.upgrade() {
                window.set_current_screen("login".into());
            }
        })
        .ok();
    } else {
        info!("Terminal not registered, starting pairing flow");

        // Request pairing code (may recover existing registration)
        let window_weak_clone2 = window_weak.clone();
        let recovered = request_pairing_code(
            window_weak_clone2,
            Arc::clone(&pairing_service),
            Arc::clone(&runtime),
        );

        if recovered {
            // Registration was recovered — load data and go straight to login
            info!("Registration recovered during pairing request, navigating to login");

            populate_catalog_data(window_weak.clone(), Arc::clone(&db), currency.clone());
            populate_operators_data(window_weak.clone(), Arc::clone(&db));

            start_background_sync(
                window_weak.clone(),
                Arc::clone(&db),
                Arc::clone(&sync_service),
                Arc::clone(&sync_tx),
                Arc::clone(&runtime),
                Arc::clone(&pairing_service),
                currency.clone(),
            );

            // Update terminal code from recovered registration
            let ps_clone = Arc::clone(&pairing_service);
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    if let Ok(Some(reg)) = ps_clone.get_registration() {
                        if let Some(code) = reg.terminal_code {
                            window.set_app_terminal_code(code.into());
                        }
                        if let Some(company) = reg.company_name {
                            window.set_app_company_name(company.into());
                        }
                    }
                    window.set_current_screen("login".into());
                }
            })
            .ok();
        } else {
            // Normal pairing flow — show setup screen and poll
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    window.set_current_screen("setup".into());
                }
            })
            .ok();

            // Start polling for pairing status
            start_pairing_poll(
                window_weak,
                pairing_service,
                runtime,
                db,
                sync_service,
                sync_tx,
            );
        }
    }
}

/// Requests a pairing code from the server.
/// Returns `true` if registration was recovered (already registered),
/// meaning caller should skip setup screen and pairing poll.
fn request_pairing_code(
    window_weak: slint::Weak<MainWindow>,
    pairing_service: Arc<PairingService>,
    runtime: Arc<Runtime>,
) -> bool {
    info!("Requesting pairing code...");

    // Update UI to show pending
    let window_weak_clone = window_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_setup_status("pending".into());
            window.set_setup_pairing_code("---".into());
        }
    })
    .ok();

    // Make the API request
    let result = runtime.block_on(async { pairing_service.request_pairing_code().await });

    match result {
        Ok(state) => {
            // Check if this is a recovered registration
            if state.status == PairingStatus::Completed {
                info!("Registration recovered! Will navigate to login screen.");
                return true;
            }

            info!("Pairing code received: {}", state.pairing_code);

            let pairing_code = state.pairing_code;
            let expires_in = (state.expires_at - chrono::Utc::now()).num_seconds().max(0) as i32;

            let window_weak_clone = window_weak.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    window.set_setup_pairing_code(pairing_code.into());
                    window.set_setup_expires_in_seconds(expires_in);
                    window.set_setup_status("pending".into());
                }
            })
            .ok();
            false
        }
        Err(e) => {
            error!("Failed to request pairing code: {}", e);

            let error_msg = format!("Connection failed: {}", e);
            let window_weak_clone = window_weak.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    window.set_setup_status("error".into());
                    window.set_setup_error_message(error_msg.into());
                }
            })
            .ok();
            false
        }
    }
}

/// Starts polling for pairing status
fn start_pairing_poll(
    window_weak: slint::Weak<MainWindow>,
    pairing_service: Arc<PairingService>,
    runtime: Arc<Runtime>,
    db: Arc<Database>,
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
) {
    use std::sync::mpsc;

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(PAIRING_POLL_INTERVAL_MS));

            // Get current pairing code from UI
            let (tx, rx) = mpsc::channel::<Option<String>>();
            let window_weak_clone = window_weak.clone();

            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    let code: String = window.get_setup_pairing_code().into();
                    if code != "---" && !code.is_empty() {
                        tx.send(Some(code)).ok();
                    } else {
                        tx.send(None).ok();
                    }
                } else {
                    tx.send(None).ok();
                }
            })
            .ok();

            let pairing_code = match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(Some(code)) => code,
                _ => continue,
            };

            // Check pairing status
            let result = runtime
                .block_on(async { pairing_service.check_pairing_status(&pairing_code).await });

            match result {
                Ok(state) => {
                    match state.status {
                        PairingStatus::Completed => {
                            info!("Pairing completed!");

                            let window_weak_clone = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(window) = window_weak_clone.upgrade() {
                                    window.set_setup_status("completed".into());
                                }
                            })
                            .ok();

                            // Load data and navigate to login after short delay
                            let window_weak_clone2 = window_weak.clone();
                            let db_clone = Arc::clone(&db);
                            let sync_service_clone = Arc::clone(&sync_service);
                            let sync_tx_clone = Arc::clone(&sync_tx);
                            let runtime_clone = Arc::clone(&runtime);
                            let pairing_service_clone = Arc::clone(&pairing_service);

                            std::thread::spawn(move || {
                                std::thread::sleep(Duration::from_millis(1500));

                                // Trigger initial sync to get products/operators (non-fatal)
                                info!("Triggering initial sync after pairing...");
                                let tx = (*sync_tx_clone).clone();
                                let sync_ok = runtime_clone.block_on(async {
                                    match tokio::time::timeout(
                                        Duration::from_secs(30),
                                        sync_service_clone.sync_all(&tx),
                                    )
                                    .await
                                    {
                                        Ok(Ok(())) => {
                                            info!("Initial sync completed successfully");
                                            true
                                        }
                                        Ok(Err(e)) => {
                                            warn!("Initial sync failed (non-fatal): {}", e);
                                            false
                                        }
                                        Err(_) => {
                                            warn!("Initial sync timed out after 30s (non-fatal)");
                                            false
                                        }
                                    }
                                });

                                if !sync_ok {
                                    info!("Proceeding to login despite sync failure - data will sync in background");
                                }

                                // Load catalog data from database (may be empty if sync failed)
                                populate_catalog_data(
                                    window_weak_clone2.clone(),
                                    Arc::clone(&db_clone),
                                    "LYD".to_string(),
                                );

                                // Load operators from database (may be empty if sync failed)
                                populate_operators_data(
                                    window_weak_clone2.clone(),
                                    Arc::clone(&db_clone),
                                );

                                // Start background sync service
                                start_background_sync(
                                    window_weak_clone2.clone(),
                                    Arc::clone(&db_clone),
                                    sync_service_clone,
                                    sync_tx_clone,
                                    runtime_clone,
                                    pairing_service_clone,
                                    "LYD".to_string(),
                                );

                                // Navigate to login screen (always, even if sync failed)
                                slint::invoke_from_event_loop(move || {
                                    if let Some(window) = window_weak_clone2.upgrade() {
                                        window.set_current_screen("login".into());
                                    }
                                })
                                .ok();
                            });
                            break;
                        }
                        PairingStatus::Expired => {
                            warn!("Pairing code expired");
                            let window_weak_clone = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(window) = window_weak_clone.upgrade() {
                                    window.set_setup_status("expired".into());
                                }
                            })
                            .ok();
                            break;
                        }
                        PairingStatus::Cancelled => {
                            warn!("Pairing cancelled");
                            let window_weak_clone = window_weak.clone();
                            slint::invoke_from_event_loop(move || {
                                if let Some(window) = window_weak_clone.upgrade() {
                                    window.set_setup_status("error".into());
                                    window.set_setup_error_message("Pairing was cancelled".into());
                                }
                            })
                            .ok();
                            break;
                        }
                        PairingStatus::Pending => {
                            // Continue polling
                        }
                    }
                }
                Err(e) => {
                    warn!("Error checking pairing status: {}", e);
                }
            }
        }
    });
}

/// Populates the UI with categories and products from the database
fn populate_catalog_data(
    window_weak: slint::Weak<MainWindow>,
    db: Arc<Database>,
    currency: String,
) {
    info!("Loading catalog data from database...");

    let product_service = ProductService::new(Arc::clone(&db));

    // Build category color map for product tiles (before consuming categories)
    let category_color_map: HashMap<String, (u8, u8, u8)> = match product_service.categories() {
        Ok(ref cats) => cats
            .iter()
            .enumerate()
            .map(|(idx, c)| {
                let hex = c
                    .color
                    .as_deref()
                    .filter(|h| !h.is_empty())
                    .unwrap_or_else(|| {
                        supermarket_category_color(&c.name, c.name_ar.as_deref().unwrap_or(""), idx)
                    });
                (c.id.clone(), parse_hex_color(hex))
            })
            .collect(),
        Err(_) => HashMap::new(),
    };

    // Load categories
    match product_service.categories() {
        Ok(categories) => {
            let category_count = categories.len();
            info!("Loaded {} categories from database", category_count);

            // Convert to Slint CategoryData
            let slint_categories: Vec<CategoryData> = categories
                .into_iter()
                .enumerate()
                .map(|(idx, cat)| {
                    // Get product count for this category
                    let product_count = product_service
                        .by_category(&cat.id, 1000, 0)
                        .map(|p| p.len() as i32)
                        .unwrap_or(0);

                    let name_ar_str = cat.name_ar.as_deref().unwrap_or(&cat.name);
                    let hex_color = cat
                        .color
                        .as_deref()
                        .filter(|c| !c.is_empty())
                        .unwrap_or_else(|| supermarket_category_color(&cat.name, name_ar_str, idx));
                    let (r, g, b) = parse_hex_color(hex_color);

                    let icon_str = cat
                        .icon
                        .as_deref()
                        .filter(|i| !i.is_empty())
                        .unwrap_or_else(|| supermarket_category_icon(&cat.name, name_ar_str));

                    CategoryData {
                        id: SharedString::from(&cat.id),
                        name: SharedString::from(&cat.name),
                        name_ar: SharedString::from(name_ar_str),
                        color: slint::Color::from_rgb_u8(r, g, b),
                        product_count,
                        icon: SharedString::from(icon_str),
                    }
                })
                .collect();

            // Set on window (create model inside event loop since ModelRc is not Send)
            let window_weak_clone = window_weak.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    let model = ModelRc::new(VecModel::from(slint_categories));
                    window.set_categories(model);
                }
            })
            .ok();
        }
        Err(e) => {
            warn!("Failed to load categories: {}", e);
        }
    }

    // Load all products
    match product_service.all_products(500, 0) {
        Ok(products) => {
            let product_count = products.len();
            info!("Loaded {} products from database", product_count);

            // Convert to Slint ProductData
            let cur = &currency;
            let slint_products: Vec<ProductData> = products
                .into_iter()
                .map(|prod| {
                    let stock = prod.stock_qty;
                    let min_stock = prod.min_stock;
                    let (r, g, b) = category_color_map
                        .get(prod.category_id.as_deref().unwrap_or(""))
                        .copied()
                        .unwrap_or((59, 130, 246)); // Default: primary blue

                    ProductData {
                        id: SharedString::from(&prod.id),
                        name: SharedString::from(&prod.name),
                        name_ar: SharedString::from(prod.name_ar.as_deref().unwrap_or(&prod.name)),
                        sku: SharedString::from(&prod.sku),
                        barcode: SharedString::from(prod.barcode.as_deref().unwrap_or("")),
                        price: SharedString::from(fmt_decimal(prod.price, cur)),
                        stock,
                        low_stock: stock > 0 && stock <= min_stock,
                        out_of_stock: stock <= 0,
                        has_image: prod.image_url.is_some(),
                        image_url: SharedString::from(prod.image_url.as_deref().unwrap_or("")),
                        category_id: SharedString::from(prod.category_id.as_deref().unwrap_or("")),
                        unit: SharedString::from(format!("{:?}", prod.unit).to_lowercase()),
                        category_color: slint::Color::from_rgb_u8(r, g, b),
                        is_weighable: prod.is_weighable,
                        product_type: SharedString::from(prod.product_type.as_str()),
                        is_service: !prod.track_inventory,
                    }
                })
                .collect();

            // Set on window (create model inside event loop since ModelRc is not Send)
            let window_weak_clone = window_weak.clone();
            let total_count = product_count as i32;
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    let model = ModelRc::new(VecModel::from(slint_products));
                    window.set_products(model);
                    window.set_total_product_count(total_count);
                }
            })
            .ok();
        }
        Err(e) => {
            warn!("Failed to load products: {}", e);
        }
    }
}

/// Refresh the cart UI from current CartService state.
/// Reads the cart, converts items to Slint types, and updates all cart properties.
fn refresh_cart_ui(
    window_weak: &slint::Weak<MainWindow>,
    cart_service: &Arc<CartService>,
    currency: &str,
) {
    let cart = cart_service.get_cart();
    let cur = currency;

    let slint_items: Vec<CartItemData> = cart
        .items
        .iter()
        .map(|item| CartItemData {
            id: SharedString::from(&item.id),
            product_id: SharedString::from(&item.product_id),
            name: SharedString::from(&item.product_name),
            name_ar: SharedString::from(
                item.product_name_ar
                    .as_deref()
                    .unwrap_or(&item.product_name),
            ),
            sku: SharedString::from(&item.sku),
            quantity: item.quantity.to_i32().unwrap_or(0),
            unit_price: SharedString::from(fmt_decimal(item.unit_price, cur)),
            line_total: SharedString::from(fmt_decimal(item.line_total, cur)),
            has_discount: item.has_discount(),
            discount_label: SharedString::from(
                if item.discount_percent > rust_decimal::Decimal::ZERO {
                    format!("-{:.0}%", item.discount_percent)
                } else if item.discount_amount > rust_decimal::Decimal::ZERO {
                    format!("-{}", fmt_decimal(item.discount_amount, cur))
                } else {
                    String::new()
                },
            ),
            discount_amount: SharedString::from(fmt_decimal(item.line_discount, cur)),
            original_price: SharedString::from(fmt_decimal(item.unit_price, cur)),
            unit: SharedString::from(format!("{:?}", item.unit).to_lowercase()),
            notes: SharedString::from(item.note.as_deref().unwrap_or("")),
            quantity_display: SharedString::from(if item.unit.allows_decimal() {
                format!("{:.3} {}", item.quantity, item.unit.label("ar"))
            } else {
                String::new()
            }),
        })
        .collect();

    let subtotal = fmt_decimal(cart.subtotal, cur);
    let tax = fmt_decimal(cart.tax_total, cur);
    let discount = fmt_decimal(cart.discount_total, cur);
    let total = fmt_decimal(cart.grand_total, cur);

    let window_weak_clone = window_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(w) = window_weak_clone.upgrade() {
            let model = ModelRc::new(VecModel::from(slint_items));
            w.set_cart_items(model);
            w.set_cart_subtotal(SharedString::from(&subtotal));
            w.set_cart_tax(SharedString::from(&tax));
            w.set_cart_discount(SharedString::from(&discount));
            w.set_cart_total(SharedString::from(&total));
        }
    })
    .ok();
}

/// Format a decimal amount using the configured currency's decimal places.
/// Returns just the number (e.g. "10.000"), not the full currency string.
fn fmt_decimal(amount: rust_decimal::Decimal, currency: &str) -> String {
    let decimals = e2manage_pos_terminal::utils::formatting::currency_decimals(currency);
    format!("{:.prec$}", amount, prec = decimals as usize)
}

/// Parses a hex color string like "#3B82F6" to RGB values
fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(59);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(130);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(246);
        (r, g, b)
    } else {
        (59, 130, 246) // Default blue
    }
}

/// Returns true if `text` (lowercased) contains any of the `keywords`.
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|kw| lower.contains(kw))
}

/// Assigns a color to a category based on its name when backend provides none.
/// Uses standard supermarket department colors.
fn supermarket_category_color(name: &str, name_ar: &str, index: usize) -> &'static str {
    match () {
        _ if contains_any(name_ar, &["ألبان", "حليب"])
            || contains_any(name, &["dairy", "milk"]) =>
        {
            "#3B82F6"
        }
        _ if contains_any(name_ar, &["خضر", "طازج", "فاكه"])
            || contains_any(name, &["fresh", "produce", "vegetable", "fruit"]) =>
        {
            "#22C55E"
        }
        _ if contains_any(name_ar, &["لحوم", "دواجن", "دجاج"])
            || contains_any(name, &["meat", "poultry"]) =>
        {
            "#EF4444"
        }
        _ if contains_any(name_ar, &["مخبوز", "خبز"])
            || contains_any(name, &["bakery", "bread"]) =>
        {
            "#A16207"
        }
        _ if contains_any(name_ar, &["مشروب", "عصير"])
            || contains_any(name, &["beverage", "drink", "juice"]) =>
        {
            "#F97316"
        }
        _ if contains_any(name_ar, &["تنظيف"]) || contains_any(name, &["clean"]) => "#06B6D4",
        _ if contains_any(name_ar, &["بقال", "غذائ"])
            || contains_any(name, &["grocer", "staple"]) =>
        {
            "#6B7280"
        }
        _ if contains_any(name_ar, &["مجمد"]) || contains_any(name, &["frozen"]) => "#8B5CF6",
        _ if contains_any(name_ar, &["عناية", "شخصي"])
            || contains_any(name, &["personal", "care"]) =>
        {
            "#EC4899"
        }
        _ if contains_any(name_ar, &["أطفال", "طفل"]) || contains_any(name, &["baby", "child"]) => {
            "#F59E0B"
        }
        _ if contains_any(name_ar, &["حيوان"]) || contains_any(name, &["pet"]) => "#84CC16",
        _ if contains_any(name_ar, &["صحة", "عافية"])
            || contains_any(name, &["health", "wellness"]) =>
        {
            "#14B8A6"
        }
        _ if contains_any(name_ar, &["منزل"]) || contains_any(name, &["house", "home"]) => {
            "#78716C"
        }
        _ if contains_any(name_ar, &["خفيف", "حلوي", "سناك"])
            || contains_any(name, &["snack", "confect"]) =>
        {
            "#F472B6"
        }
        _ if contains_any(name_ar, &["بحري", "سمك"])
            || contains_any(name, &["seafood", "fish"]) =>
        {
            "#0EA5E9"
        }
        _ => {
            const FALLBACK_PALETTE: &[&str] = &[
                "#6366F1", "#10B981", "#F59E0B", "#EF4444", "#8B5CF6", "#EC4899", "#06B6D4",
                "#84CC16", "#F97316", "#14B8A6",
            ];
            FALLBACK_PALETTE[index % FALLBACK_PALETTE.len()]
        }
    }
}

/// Assigns an emoji icon to a category when backend provides none.
fn supermarket_category_icon(name: &str, name_ar: &str) -> &'static str {
    match () {
        _ if contains_any(name_ar, &["ألبان", "حليب"])
            || contains_any(name, &["dairy", "milk"]) =>
        {
            "🥛"
        }
        _ if contains_any(name_ar, &["خضر", "طازج"])
            || contains_any(name, &["fresh", "produce", "vegetable"]) =>
        {
            "🥬"
        }
        _ if contains_any(name_ar, &["لحوم", "دواجن"])
            || contains_any(name, &["meat", "poultry"]) =>
        {
            "🥩"
        }
        _ if contains_any(name_ar, &["مخبوز", "خبز"])
            || contains_any(name, &["bakery", "bread"]) =>
        {
            "🍞"
        }
        _ if contains_any(name_ar, &["مشروب", "عصير"])
            || contains_any(name, &["beverage", "drink"]) =>
        {
            "🥤"
        }
        _ if contains_any(name_ar, &["تنظيف"]) || contains_any(name, &["clean"]) => "🧹",
        _ if contains_any(name_ar, &["بقال", "غذائ"])
            || contains_any(name, &["grocer", "staple"]) =>
        {
            "🛒"
        }
        _ if contains_any(name_ar, &["مجمد"]) || contains_any(name, &["frozen"]) => "🧊",
        _ if contains_any(name_ar, &["عناية", "شخصي"])
            || contains_any(name, &["personal", "care"]) =>
        {
            "🧴"
        }
        _ if contains_any(name_ar, &["أطفال", "طفل"]) || contains_any(name, &["baby", "child"]) => {
            "🍼"
        }
        _ if contains_any(name_ar, &["حيوان"]) || contains_any(name, &["pet"]) => "🐾",
        _ if contains_any(name_ar, &["صحة", "عافية"])
            || contains_any(name, &["health", "wellness"]) =>
        {
            "💊"
        }
        _ if contains_any(name_ar, &["منزل"]) || contains_any(name, &["house", "home"]) => "🏠",
        _ if contains_any(name_ar, &["خفيف", "حلوي"])
            || contains_any(name, &["snack", "confect"]) =>
        {
            "🍫"
        }
        _ if contains_any(name_ar, &["بحري", "سمك"])
            || contains_any(name, &["seafood", "fish"]) =>
        {
            "🐟"
        }
        _ => "📦",
    }
}

/// Populates the UI with operators from the database
fn populate_operators_data(window_weak: slint::Weak<MainWindow>, db: Arc<Database>) {
    info!("Loading operators from database...");

    // Load operators from database
    match db.get_operators() {
        Ok(operators) => {
            let operator_count = operators.len();
            info!("Loaded {} operators from database", operator_count);

            // Avatar colors for operators (cycle through these)
            let avatar_colors: [(u8, u8, u8); 8] = [
                (37, 99, 235),  // #2563EB - Blue
                (139, 92, 246), // #8B5CF6 - Purple
                (34, 197, 94),  // #22C55E - Green
                (245, 158, 11), // #F59E0B - Amber
                (239, 68, 68),  // #EF4444 - Red
                (6, 182, 212),  // #06B6D4 - Cyan
                (236, 72, 153), // #EC4899 - Pink
                (249, 115, 22), // #F97316 - Orange
            ];

            // Convert to Slint Operator struct
            let slint_operators: Vec<Operator> = operators
                .into_iter()
                .map(|op| {
                    // Use Arabic name if available, otherwise English
                    let display_name = op.name_ar.as_deref().unwrap_or(&op.name);

                    // Generate initials from name
                    let initials = generate_arabic_initials(display_name);

                    // Map role to Arabic
                    let role_ar = match op.role.to_uppercase().as_str() {
                        "CASHIER" => "كاشير",
                        "SUPERVISOR" | "MANAGER" => "مشرف",
                        "ADMIN" => "مدير",
                        _ => "كاشير",
                    };

                    // Pick color based on ID hash (consistent with on_operator_selected)
                    let color_idx = op
                        .id
                        .bytes()
                        .fold(0usize, |acc, b| acc.wrapping_add(b as usize))
                        % avatar_colors.len();
                    let color = avatar_colors[color_idx];

                    Operator {
                        id: SharedString::from(&op.id),
                        name: SharedString::from(display_name),
                        role: SharedString::from(role_ar),
                        initials: SharedString::from(&initials),
                        color: slint::Color::from_rgb_u8(color.0, color.1, color.2),
                    }
                })
                .collect();

            // Update UI on main thread
            let window_weak_clone = window_weak.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak_clone.upgrade() {
                    let model = ModelRc::new(VecModel::from(slint_operators));
                    window.set_operators(model);
                    info!("Operators UI updated");
                }
            })
            .ok();
        }
        Err(e) => {
            warn!("Failed to load operators: {}", e);
        }
    }
}

/// Generates initials from an Arabic or English name
fn generate_arabic_initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .collect()
}

/// Starts background sync service with automatic UI refresh
///
/// This function:
/// 1. Spawns a tokio task for periodic sync polling
/// 2. Spawns a listener task that refreshes the UI when catalog is updated
fn start_background_sync(
    window_weak: slint::Weak<MainWindow>,
    db: Arc<Database>,
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    runtime: Arc<Runtime>,
    pairing_service: Arc<PairingService>,
    currency: String,
) {
    info!("Starting background sync service...");

    // Clone what we need for the polling task
    let sync_service_clone = Arc::clone(&sync_service);
    let tx_for_polling = (*sync_tx).clone();

    // Spawn the periodic sync polling task
    let runtime_clone = Arc::clone(&runtime);
    std::thread::spawn(move || {
        runtime_clone.block_on(async move {
            sync_service_clone.start_polling(tx_for_polling).await;
        });
    });

    // Subscribe to sync events and refresh UI when catalog updates
    let mut rx = sync_tx.subscribe();
    let window_weak_for_listener = window_weak.clone();
    let db_for_listener = Arc::clone(&db);
    let currency_for_listener = currency.clone();
    let pairing_service_for_listener = Arc::clone(&pairing_service);
    let runtime_for_listener = Arc::clone(&runtime);

    std::thread::spawn(move || {
        loop {
            match rx.blocking_recv() {
                Ok(event) => {
                    match event {
                        SyncEvent::CatalogUpdated {
                            products_count,
                            categories_count,
                        } => {
                            info!(
                                "Catalog updated: {} products, {} categories - refreshing UI",
                                products_count, categories_count
                            );
                            // Refresh the UI with new data
                            populate_catalog_data(
                                window_weak_for_listener.clone(),
                                Arc::clone(&db_for_listener),
                                currency_for_listener.clone(),
                            );
                        }
                        SyncEvent::Started => {
                            debug!("Background sync started");
                        }
                        SyncEvent::Completed => {
                            info!("Background sync completed");
                        }
                        SyncEvent::Failed(err) => {
                            warn!("Background sync failed: {}", err);
                        }
                        SyncEvent::Offline => {
                            debug!("Sync skipped - terminal offline");
                        }
                        SyncEvent::DraftsSynced { synced, failed } => {
                            if synced > 0 || failed > 0 {
                                info!(
                                    "Background drafts synced: {} synced, {} failed",
                                    synced, failed
                                );
                            }
                        }
                        SyncEvent::OperatorsUpdated { count } => {
                            info!("Operators updated: {} operators - refreshing UI", count);
                            // Refresh the operators list in UI
                            populate_operators_data(
                                window_weak_for_listener.clone(),
                                Arc::clone(&db_for_listener),
                            );

                            // If the company has zero operators, the terminal is unusable.
                            // Show setup screen so the user can re-pair to a valid company.
                            if count == 0 {
                                warn!("Company has zero operators — terminal cannot function, prompting re-pair");
                                let w = window_weak_for_listener.clone();
                                slint::invoke_from_event_loop(move || {
                                    if let Some(window) = w.upgrade() {
                                        window.set_current_screen("setup".into());
                                        window.set_setup_status("error".into());
                                        window.set_setup_error_message(
                                            "لا يوجد مشغلون في هذه الشركة - يرجى إعادة الربط بشركة صحيحة".into(),
                                        );
                                    }
                                }).ok();
                            }
                        }
                        SyncEvent::TerminalNotRegistered => {
                            error!("Terminal not registered on server - clearing local registration and redirecting to pairing");
                            // Clear local registration
                            if let Err(e) = pairing_service_for_listener.clear_registration() {
                                error!("Failed to clear registration: {}", e);
                            }

                            // Navigate to setup screen and request a new pairing code
                            let window_weak_clone = window_weak_for_listener.clone();
                            let ps_clone = Arc::clone(&pairing_service_for_listener);
                            let rt_clone = Arc::clone(&runtime_for_listener);
                            let db_clone = Arc::clone(&db_for_listener);
                            let currency_clone = currency_for_listener.clone();
                            std::thread::spawn(move || {
                                let window_weak_clone2 = window_weak_clone.clone();

                                // Show error on setup screen first
                                slint::invoke_from_event_loop(move || {
                                    if let Some(w) = window_weak_clone2.upgrade() {
                                        w.set_current_screen("setup".into());
                                        w.set_setup_status("error".into());
                                        w.set_setup_error_message(
                                            "الجهاز غير مسجل - يرجى إعادة الربط".into(),
                                        );
                                    }
                                })
                                .ok();

                                // Brief pause for the user to see the error
                                std::thread::sleep(Duration::from_millis(1500));

                                // Request a new pairing code automatically
                                info!("Requesting new pairing code after auth failure...");
                                let recovered = request_pairing_code(
                                    window_weak_clone.clone(),
                                    Arc::clone(&ps_clone),
                                    rt_clone,
                                );
                                if recovered {
                                    info!("Registration recovered during re-pairing, reloading data and navigating to login");

                                    // Reload operators and catalog with fresh data
                                    populate_catalog_data(
                                        window_weak_clone.clone(),
                                        Arc::clone(&db_clone),
                                        currency_clone.clone(),
                                    );
                                    populate_operators_data(
                                        window_weak_clone.clone(),
                                        Arc::clone(&db_clone),
                                    );

                                    // Update terminal code and company from recovered registration
                                    slint::invoke_from_event_loop(move || {
                                        if let Some(w) = window_weak_clone.upgrade() {
                                            if let Ok(Some(reg)) = ps_clone.get_registration() {
                                                if let Some(code) = reg.terminal_code {
                                                    w.set_app_terminal_code(code.into());
                                                }
                                                if let Some(company) = reg.company_name {
                                                    w.set_app_company_name(company.into());
                                                }
                                            }
                                            w.set_current_screen("login".into());
                                        }
                                    })
                                    .ok();
                                }
                            });
                        }
                        _ => {
                            // Other events (features, drafts queue empty, etc.)
                            debug!("Sync event: {:?}", event);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Sync event channel closed, stopping listener");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Sync event listener lagged by {} messages", n);
                }
            }
        }
    });

    info!("Background sync service started successfully");
}

/// Triggers a manual sync (called on operator login/shift start)
fn trigger_manual_sync(
    sync_service: Arc<SyncService>,
    sync_tx: Arc<broadcast::Sender<SyncEvent>>,
    runtime: Arc<Runtime>,
) {
    info!("Triggering manual sync on operator login...");

    let tx = (*sync_tx).clone();

    std::thread::spawn(move || {
        runtime.block_on(async move {
            // Run a single sync cycle
            if let Err(e) = sync_service.sync_all(&tx).await {
                warn!("Manual sync failed: {}", e);
                let _ = tx.send(SyncEvent::Failed(e.to_string()));
            } else {
                let _ = tx.send(SyncEvent::Completed);
            }
        });
    });
}
