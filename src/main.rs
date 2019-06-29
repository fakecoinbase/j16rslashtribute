#[macro_use]
extern crate serde_derive;
extern crate coinbase_pro_rs;
extern crate csv;
extern crate toml;

use std::error::Error;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::process;

use coinbase_pro_rs::structs::private::*;
use coinbase_pro_rs::structs::public::*;
use coinbase_pro_rs::structs::DateTime;
use coinbase_pro_rs::{Private, Sync, MAIN_URL};

#[derive(Deserialize)]
struct Config {
    exchanges: Vec<Exchange>,
}

#[derive(Deserialize)]
enum Exchange {
    Coinbase {
        key: String,
        secret: String,
        passphrase: String,
    },
}

fn get_rate_at(
    client: &Private<Sync>,
    product_id: &str,
    time_of_trade: DateTime,
) -> Result<f64, Box<Error>> {
    let market_at_trade = client
        .public()
        .get_candles(&product_id, Some(time_of_trade), None, Granularity::M1)
        .unwrap();

    let mut rate = 0.0;
    if let Some(candle) = market_at_trade.first() {
        println!("candle({:?})", candle);
        rate = (candle.1 + candle.2) / 2.0;
    }
    Ok(rate)
}

fn product_rhs(product_id: &str) -> Option<String> {
    product_id
        .split("-")
        .collect::<Vec<&str>>()
        .get(1)
        .map(|v| v.clone().into())
}

#[test]
fn test_product_rhs() {
    assert_eq!(product_rhs("ETH-BTC"), Some("BTC".into()));
    assert_eq!(product_rhs("ETH"), None);
    assert_eq!(product_rhs(""), None);
}

fn get_usd_rate(
    client: &Private<Sync>,
    product_id: &str,
    time_of_trade: DateTime,
) -> Result<f64, Box<Error>> {
    if let Ok(rate) = get_rate_at(client, product_id, time_of_trade) {
        if let Some(product_lhs) = product_rhs(product_id) {
            println!("{} = {}", product_lhs, rate);
            if product_lhs == "USD" {
                return Ok(rate);
            }

            let next_product_id = format!("{}-USD", product_lhs);

            if let Ok(usd_rate) = get_rate_at(client, &next_product_id, time_of_trade) {
                println!("{} = {}", next_product_id, usd_rate);
                return Ok(rate * usd_rate);
            }
        }
    }

    Ok(0.0)
}

fn export_coinbase(key: &str, secret: &str, passphrase: &str) -> Result<(), Box<Error>> {
    let client: Private<Sync> = Private::new(MAIN_URL, key, secret, passphrase);

    let mut writer = csv::Writer::from_writer(io::stdout());

    writer.write_record(&[
        "ID",
        "Token",
        "Market",
        "Amount",
        "Balance",
        "Rate",
        "USD Rate",
        "USD Amount",
        "Created At",
    ])?;

    let accounts = client.get_accounts().unwrap();

    for account in accounts {
        if account.currency != "ETH" {
            continue;
        }

        for trade in client.get_account_hist(account.id).unwrap() {
            if let AccountHistoryDetails::Match { product_id, .. } = trade.details {
                let time_of_trade = trade.created_at;

                let rate = get_rate_at(&client, &product_id, time_of_trade)?;
                let usd_rate = get_usd_rate(&client, &product_id, time_of_trade)?;
                let usd_amount = trade.amount * usd_rate;

                writer.write_record(&[
                    &trade.id.to_string(),
                    &product_id,
                    &account.currency,
                    &trade.amount.to_string(),
                    &trade.balance.to_string(),
                    &rate.to_string(),
                    &usd_rate.to_string(),
                    &usd_amount.to_string(),
                    &trade.created_at.to_string(),
                ])?;
            }
        }
    }

    writer.flush()?;
    Ok(())
}

fn export(exchange: &Exchange) -> Result<(), Box<Error>> {
    match exchange {
        Exchange::Coinbase {
            key,
            secret,
            passphrase,
        } => export_coinbase(key, secret, passphrase),
    }
}

fn load_config() -> io::Result<Config> {
    let mut input = String::new();
    File::open("config.toml").and_then(|mut f| f.read_to_string(&mut input))?;

    let config: Config = toml::from_str(&input).unwrap();
    Ok(config)
}

fn main() {
    let config = load_config().unwrap();

    for exchange in config.exchanges {
        if let Err(err) = export(&exchange) {
            println!("{}", err);
            process::exit(1);
        }
    }
}
