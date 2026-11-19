// SPDX-License-Identifier: GPL-2.0-only
use std::{
    io::{Read, Write},
    sync::LazyLock,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, ensure};
use reqwest::{StatusCode, Url, blocking::Client, redirect::Policy};

static CLIENT: LazyLock<reqwest::Result<Client>> = LazyLock::new(|| {
    Client::builder()
        .https_only(true)
        // Only the connection is time-limited: a multi-gigabyte download may take as long as it takes.
        .connect_timeout(Duration::from_secs(30))
        .user_agent(concat!("wiiform/", env!("CARGO_PKG_VERSION")))
        .referer(false)
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() > 5 {
                attempt.error("download exceeded five redirects")
            } else if allowed_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("download redirected to a disallowed URL")
            }
        }))
        .build()
});

fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

/// Downloads a file held in memory, up to `limit` bytes.
pub(crate) fn fetch(url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    response(url)?
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .context("incomplete HTTPS download")?;
    ensure!(bytes.len() <= limit, "download exceeds size limit");
    Ok(bytes)
}

/// Streams a file of any size into `sink`, without holding it in memory.
pub(crate) fn download(url: &str, sink: &mut dyn Write) -> Result<()> {
    std::io::copy(&mut response(url)?, sink).context("incomplete HTTPS download")?;
    Ok(())
}

fn response(url: &str) -> Result<reqwest::blocking::Response> {
    let url = Url::parse(url).context("invalid package URL")?;
    ensure!(
        allowed_url(&url),
        "download URL must use HTTPS without credentials or a fragment"
    );
    let client = CLIENT
        .as_ref()
        .map_err(|error| anyhow!("cannot initialize HTTPS client: {error}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(reqwest::Error::without_url)
        .context("HTTPS download failed")?;
    ensure!(
        response.status() == StatusCode::OK,
        "HTTP {} fetching HTTPS resource",
        response.status().as_u16()
    );
    Ok(response)
}
