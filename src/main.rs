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
use build::{build_site, build_site_for_dev};
use clap::Parser;
use cli::{Cli, Commands};
use config::{cfg, init_config, SiteConfig};
use deploy::deploy_site;
use generator::{rss::build_rss, sitemap::build_sitemap};
use gix::ThreadSafeRepository;
use init::new_site;
use serve::serve_site;

fn main() -> Result<()> {
    let cli: &'static Cli = Box::leak(Box::new(Cli::parse()));
    init_config(SiteConfig::load(cli)?);

    match &cli.command {
        Commands::Init { name } => new_site(&cfg(), name.is_some()),
        Commands::Build { .. } => build_all().map(|_| ()),
        Commands::Deploy { .. } => {
            let repo = build_all()?;
            deploy_site(&repo, &cfg())
        }
        Commands::Serve { .. } => {
            // Use dev mode build for serve to emit data-tola-id attributes
            build_all_for_dev()?;
            serve_site()
        }
    }
}

/// Build site and optionally generate rss/sitemap in parallel.
///
/// rss generation is controlled by `config.build.rss.enable`.
/// Sitemap generation is controlled by `config.build.sitemap.enable`.
/// Output cleanup is controlled by `config.build.clean`.
fn build_all() -> Result<ThreadSafeRepository> {
    let c = cfg();
    // Build site first, collecting page metadata
    let (repo, pages) = build_site(&c, false)?;

    // Generate rss and sitemap in parallel using collected pages
    let (rss_result, sitemap_result) = rayon::join(
        || build_rss(&c, &pages),
        || build_sitemap(&c, &pages),
    );

    rss_result?;
    sitemap_result?;
    Ok(repo)
}

/// Build site for development mode with hot reload support.
///
/// Same as `build_all()` but emits `data-tola-id` attributes on all elements
/// for VDOM diffing. RSS and sitemap are still generated normally.
fn build_all_for_dev() -> Result<ThreadSafeRepository> {
    let c = cfg();
    // Build site in dev mode (with data-tola-id attributes)
    let (repo, pages) = build_site_for_dev(&c, false)?;

    // Generate rss and sitemap in parallel using collected pages
    let (rss_result, sitemap_result) = rayon::join(
        || build_rss(&c, &pages),
        || build_sitemap(&c, &pages),
    );

    rss_result?;
    sitemap_result?;
    Ok(repo)
}

