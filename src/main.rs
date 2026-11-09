use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use handout::{Config, app, service};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "handout",
    version,
    about = "Share agent-generated files with recipients via links"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the server (reads HANDOUT_* env vars)
    Serve,
    /// Manage owners (tenants)
    Owner {
        #[command(subcommand)]
        cmd: OwnerCmd,
    },
    /// Manage API keys
    Key {
        #[command(subcommand)]
        cmd: KeyCmd,
    },
}

#[derive(Subcommand)]
enum OwnerCmd {
    /// Create an owner; prompts for a password unless --password / HANDOUT_OWNER_PASSWORD is set
    Add {
        slug: String,
        #[arg(long)]
        name: String,
        #[arg(long, env = "HANDOUT_OWNER_PASSWORD", hide_env_values = true)]
        password: Option<String>,
    },
    List,
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Create an API key for an owner (printed once)
    Add {
        owner: String,
        #[arg(long)]
        name: String,
    },
    List {
        owner: String,
    },
    Revoke {
        owner: String,
        id: i64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("handout=info,tower_http=info")),
        )
        .init();
    let cli = Cli::parse();
    let cfg = Config::from_env()?;
    let state = app::build_state(cfg.clone()).await?;

    match cli.cmd {
        Cmd::Serve => {
            let listener = tokio::net::TcpListener::bind(cfg.listen)
                .await
                .with_context(|| format!("bind {}", cfg.listen))?;
            tracing::info!(listen = %cfg.listen, public = %cfg.public_url, content = %cfg.content_url, "handout listening");
            tokio::spawn(service::blob_sweep_loop(state.clone()));
            app::serve(state, listener, async {
                let _ = tokio::signal::ctrl_c().await;
                tracing::info!("shutting down");
            })
            .await
        }
        Cmd::Owner { cmd } => match cmd {
            OwnerCmd::Add {
                slug,
                name,
                password,
            } => {
                let password = match password {
                    Some(p) => p,
                    None => rpassword::prompt_password("Password: ").context("read password")?,
                };
                let owner = service::create_owner(&state.db, &slug, &name, &password)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("created owner {} ({})", owner.slug, owner.name);
                Ok(())
            }
            OwnerCmd::List => {
                let rows =
                    sqlx::query_as::<_, handout::models::Owner>("SELECT * FROM owners ORDER BY id")
                        .fetch_all(&state.db)
                        .await?;
                for o in rows {
                    println!("{}\t{}\t{}", o.id, o.slug, o.name);
                }
                Ok(())
            }
        },
        Cmd::Key { cmd } => match cmd {
            KeyCmd::Add { owner, name } => {
                let owner = service::owner_by_slug(&state.db, &owner)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .context("owner not found")?;
                let (k, raw) = service::create_api_key(&state, &owner, &name)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("{raw}");
                eprintln!(
                    "API key #{} '{}' created for {}. Shown once.",
                    k.id, k.name, owner.slug
                );
                Ok(())
            }
            KeyCmd::List { owner } => {
                let owner = service::owner_by_slug(&state.db, &owner)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .context("owner not found")?;
                for k in service::list_api_keys(&state, &owner)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                {
                    println!(
                        "{}\t{}\t{}…\tlast used {}\t{}",
                        k.id,
                        k.name,
                        k.prefix,
                        k.last_used_at.unwrap_or_else(|| "never".into()),
                        if k.revoked_at.is_some() {
                            "REVOKED"
                        } else {
                            ""
                        }
                    );
                }
                Ok(())
            }
            KeyCmd::Revoke { owner, id } => {
                let owner = service::owner_by_slug(&state.db, &owner)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .context("owner not found")?;
                service::revoke_api_key(&state, &owner, id)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                println!("revoked key {id}");
                Ok(())
            }
        },
    }
}
