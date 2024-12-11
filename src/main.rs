use anyhow::{Context, Result};
use log::{error, info};
use reqwest::Client;
use rodio::{source::Source, Decoder, OutputStream};
use serde::Deserialize;
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};
use tokio::time;

#[derive(Debug, Deserialize)]
struct KrakenResponse {
    error: Vec<String>,
    result: KrakenResult,
}

#[derive(Debug, Deserialize)]
struct KrakenResult {
    #[serde(rename = "XXBTZUSD")]
    btc_usd: BtcUsdPair,
}

#[derive(Debug, Deserialize)]
struct BtcUsdPair {
    c: Vec<String>, // c = last trade closed price
}

struct PriceMonitor {
    client: Client,
    last_price: Option<f64>,
    last_alert: Instant,
    alert_threshold: f64,
}

impl PriceMonitor {
    fn new(threshold: f64) -> Self {
        Self {
            client: Client::new(),
            last_price: None,
            last_alert: Instant::now(),
            alert_threshold: threshold,
        }
    }

    async fn fetch_price(&self) -> Result<f64> {
        let response: KrakenResponse = self
            .client
            .get("https://api.kraken.com/0/public/Ticker?pair=XBTUSD")
            .send()
            .await?
            .json()
            .await?;

        if !response.error.is_empty() {
            anyhow::bail!("Kraken API error: {:?}", response.error);
        }

        let price = response.result.btc_usd.c[0]
            .parse::<f64>()
            .context("Failed to parse price")?;

        Ok(price)
    }

    fn should_alert(&self, current_price: f64) -> bool {
        if let Some(last_price) = self.last_price {
            let price_change = (current_price - last_price).abs() / last_price;
            price_change >= self.alert_threshold
        } else {
            false
        }
    }

    fn play_alert(&self) -> Result<()> {
        let (_stream, stream_handle) = OutputStream::try_default()?;
        let file = File::open(Path::new("src/alert.mp3"))?;
        let source = Decoder::new(BufReader::new(file))?;
        stream_handle.play_raw(source.convert_samples())?;
        std::thread::sleep(Duration::from_secs(1)); // Wait for sound to play
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting Bitcoin Price Monitor");

    let mut monitor = PriceMonitor::new(0.00001); // 0.5% threshold
    let interval = Duration::from_secs(5); // 5 minutes

    let mut interval_timer = time::interval(interval);
    loop {
        interval_timer.tick().await;

        match monitor.fetch_price().await {
            Ok(current_price) => {
                info!("Current BTC price: ${:.2}", current_price);

                if monitor.should_alert(current_price) {
                    info!("Price change threshold reached! Playing alert...");
                    if let Err(e) = monitor.play_alert() {
                        error!("Failed to play alert sound: {}", e);
                    }
                    monitor.last_alert = Instant::now();
                }

                monitor.last_price = Some(current_price);
            }
            Err(e) => {
                error!("Failed to fetch price: {}", e);
            }
        }
    }
}