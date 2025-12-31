//! Tola - A static site generator for Typst blogs.

#[cfg(feature = "actor")]
mod actor;
mod build;
mod cache;
mod cli;
mod compiler;
mod config;
mod data;
mod deploy;
mod driver;
mod generator;
mod hotreload;
mod init;
mod logger;
mod serve;
mod typst_lib;
mod utils;
mod vdom;
mod watch;

use anyhow::Result;
use build::build_site;
use clap::Parser;
use cli::{Cli, Commands};
use config::{cfg, init_config, SiteConfig};
use deploy::deploy_site;
use driver::{Development, Production};
use generator::{rss::build_rss, sitemap::build_sitemap};
use gix::ThreadSafeRepository;
use init::new_site;
use serve::serve_site;

fn main() -> Result<()> {
    let cli: &'static Cli = Box::leak(Box::new(Cli::parse()));
    init_config(SiteConfig::load(cli)?);

    match &cli.command {
        Commands::Init { name } => new_site(&cfg(), name.is_some()),
        Commands::Build { .. } => build_all(Production).map(|_| ()),
        Commands::Deploy { .. } => {
            let repo = build_all(Production)?;
            deploy_site(&repo, &cfg())
        }
        Commands::Serve { .. } => {
            // Use dev mode build for serve to emit data-tola-id attributes
            build_all(Development)?;
            serve_site()
        }
    }
}

/// Build site and optionally generate rss/sitemap in parallel.
///
/// # Type Parameter
/// * `D` - Build driver (Production or Development)
fn build_all<D: driver::BuildDriver + Copy>(driver: D) -> Result<ThreadSafeRepository> {
    let c = cfg();
    // Build site with driver
    let (repo, pages) = build_site(driver, &c, false)?;

    // Generate rss and sitemap in parallel using collected pages
    let (rss_result, sitemap_result) = rayon::join(
        || build_rss(&c, &pages),
        || build_sitemap(&c, &pages),
    );

    rss_result?;
    sitemap_result?;
    Ok(repo)
}

