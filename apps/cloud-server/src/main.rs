//! Cloud server entry point.
pub mod app;
pub mod features;
pub mod platform;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("serve") => app::bootstrap::run().await,
        Some("migrate") => app::bootstrap::migrate().await,
        Some("healthcheck") => app::bootstrap::healthcheck(),
        Some("admin-bootstrap") => {
            let username = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("admin-bootstrap requires a username"))?;
            if args.next().is_some() {
                anyhow::bail!("admin-bootstrap accepts exactly one username");
            }
            app::bootstrap::admin_bootstrap(&username).await
        }
        Some(command) => anyhow::bail!(
            "unknown command {command:?}; expected serve, migrate, healthcheck, or admin-bootstrap <username>"
        ),
    }
}
