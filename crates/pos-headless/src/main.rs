//! `pos-headless` — a Slint-free CLI driver for the POS service layer.
//!
//! The Slint UI binary takes ~196s to relink after any change to a service
//! crate, because it carries 65 `.slint` files and 684 transitive deps. This
//! binary uses the same `pos-bootstrap` startup sequence, exposes a handful
//! of subcommands that exercise the services directly, and rebuilds in
//! seconds because it has no UI dependency. It is a developer-iteration
//! tool, not a production binary.
//!
//! ```text
//! pos-headless info
//! pos-headless products list [--limit N]
//! pos-headless products search <query> [--limit N]
//! pos-headless cart demo
//! ```

use std::process::ExitCode;

use anyhow::{anyhow, bail, Context};
use pos_bootstrap::{init, AppContext, InitConfig};
use rust_decimal::Decimal;
use tracing::error;
use tracing_subscriber::EnvFilter;

const HELP: &str = "\
pos-headless — Slint-free CLI driver for the POS service layer

USAGE:
    pos-headless <SUBCOMMAND>

SUBCOMMANDS:
    info                                Print loaded context
    products list [--limit N]           List cached products (default 20)
    products search <query> [--limit N] Full-text search cached products
    cart demo                           Run a smoke-test cart flow
    help                                Show this help

ENVIRONMENT:
    E2M_API_URL    Backend URL (defaults to the dev IP, see pos-bootstrap)
    RUST_LOG       Tracing filter, e.g. `info` or `pos_services=debug`
";

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .init();

    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }

    let cmd = match args.subcommand() {
        Ok(Some(c)) => c,
        Ok(None) => {
            print!("{HELP}");
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("error parsing subcommand: {err}");
            return ExitCode::FAILURE;
        }
    };

    if cmd == "help" {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }

    let ctx = match init(InitConfig::from_env()) {
        Ok(ctx) => ctx,
        Err(err) => {
            error!(error = %err, source = ?std::error::Error::source(&err), "bootstrap failed");
            return ExitCode::from(2);
        }
    };

    let result = match cmd.as_str() {
        "info" => cmd_info(&ctx),
        "products" => dispatch_products(&ctx, args),
        "cart" => dispatch_cart(&ctx, args),
        other => Err(anyhow!("unknown subcommand: {other:?} (try `pos-headless help`)")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!(error = ?err, "command failed");
            ExitCode::FAILURE
        }
    }
}

fn dispatch_products(ctx: &AppContext, mut args: pico_args::Arguments) -> anyhow::Result<()> {
    let action = args
        .subcommand()
        .map_err(|e| anyhow!("parse error: {e}"))?
        .ok_or_else(|| anyhow!("expected `list` or `search`"))?;
    match action.as_str() {
        "list" => {
            let limit: i32 = args
                .opt_value_from_str("--limit")
                .map_err(|e| anyhow!("--limit: {e}"))?
                .unwrap_or(20);
            cmd_products_list(ctx, limit)
        }
        "search" => {
            let limit: i32 = args
                .opt_value_from_str("--limit")
                .map_err(|e| anyhow!("--limit: {e}"))?
                .unwrap_or(20);
            let rest = args.finish();
            let query = rest
                .first()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow!("usage: pos-headless products search <query>"))?
                .to_string();
            cmd_products_search(ctx, &query, limit)
        }
        other => bail!("unknown products action: {other:?}"),
    }
}

fn dispatch_cart(ctx: &AppContext, mut args: pico_args::Arguments) -> anyhow::Result<()> {
    let action = args
        .subcommand()
        .map_err(|e| anyhow!("parse error: {e}"))?
        .ok_or_else(|| anyhow!("expected `demo`"))?;
    match action.as_str() {
        "demo" => cmd_cart_demo(ctx),
        other => bail!("unknown cart action: {other:?}"),
    }
}

fn cmd_info(ctx: &AppContext) -> anyhow::Result<()> {
    println!("server_url:    {}", ctx.config.server_url);
    println!("data_dir:      {}", ctx.config.data_dir.display());
    println!("sync_interval: {} min", ctx.config.sync_interval_minutes);

    match &ctx.registration {
        Some(reg) => {
            println!(
                "registered:    yes (terminal_code={}, tenant_id={})",
                reg.terminal_code.as_deref().unwrap_or("?"),
                reg.tenant_id.as_deref().unwrap_or("?"),
            );
        }
        None => println!("registered:    no"),
    }

    match &ctx.saved_session {
        Some(session) => {
            println!(
                "session:       loaded (terminal={}, currency={}, has_token={})",
                session.terminal_code,
                session.currency,
                !session.session_token.is_empty(),
            );
        }
        None => println!("session:       none"),
    }

    let count = ctx.product.product_count().context("count products")?;
    println!("products:      {count} cached locally");
    Ok(())
}

fn cmd_products_list(ctx: &AppContext, limit: i32) -> anyhow::Result<()> {
    let products = ctx.product.all_products(limit, 0).context("list products")?;
    if products.is_empty() {
        println!("(no products in local cache — sync from the UI first)");
        return Ok(());
    }
    print_products(&products);
    Ok(())
}

fn cmd_products_search(ctx: &AppContext, query: &str, limit: i32) -> anyhow::Result<()> {
    let result = ctx
        .product
        .smart_search(query, limit)
        .context("search products")?;
    if result.products.is_empty() {
        println!("(no matches for {query:?})");
        return Ok(());
    }
    print_products(&result.products);
    Ok(())
}

fn cmd_cart_demo(ctx: &AppContext) -> anyhow::Result<()> {
    let products = ctx
        .product
        .all_products(1, 0)
        .context("fetch demo product")?;
    let Some(product) = products.into_iter().next() else {
        bail!("no products in local cache; cannot run cart demo");
    };

    println!(
        "Picked product: {} (sku={}, price={})",
        product.name, product.sku, product.price
    );

    let line_id = ctx
        .cart
        .add_item(&product, Decimal::from(2))
        .context("add item to cart")?;
    println!("Added line: {line_id}");

    let cart = ctx.cart.get_cart();
    let qty: Decimal = cart.items.iter().map(|i| i.quantity).sum();
    let subtotal: Decimal = cart.items.iter().map(|i| i.unit_price * i.quantity).sum();
    println!(
        "Cart: {} line(s), {qty} item(s), subtotal={subtotal}",
        cart.items.len()
    );
    Ok(())
}

fn print_products(products: &[pos_models::Product]) {
    println!("{:<8} {:<14} {:>10}  {}", "id", "sku", "price", "name");
    println!("{}", "-".repeat(70));
    for p in products {
        let id_short = p.id.chars().take(8).collect::<String>();
        println!("{:<8} {:<14} {:>10}  {}", id_short, p.sku, p.price, p.name);
    }
}
