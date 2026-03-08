mod app;
mod auth;
mod cache;
mod config;
mod library;
mod model;
mod playback;
mod state;
mod subsonic;

use library::LibraryCache;
use subsonic::SubsonicClient;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let creds = auth::bootstrap()?;
    let client = SubsonicClient::new(
        &creds.config.server_url,
        &creds.config.username,
        &creds.password,
    )?;

    let initial_cache = if creds.config.always_hard_refresh_on_launch {
        LibraryCache::load(&client)?
    } else if let Some(snapshot) =
        cache::load_library_snapshot(&creds.config.server_url, &creds.config.username)
    {
        LibraryCache::from_snapshot(snapshot)
    } else {
        LibraryCache::load(&client)?
    };

    let final_cache = app::run(
        client,
        initial_cache,
        creds.config.expand_on_search_collapse,
        creds.config.show_identity_label,
        creds.config.keybinds.clone(),
    )?;
    if let Err(err) = cache::save_library_snapshot(
        &creds.config.server_url,
        &creds.config.username,
        &final_cache.snapshot(),
    ) {
        eprintln!("warning: failed to save library cache: {err:#}");
    }

    drop(creds.password);
    Ok(())
}
