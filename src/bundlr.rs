use std::collections::HashMap;
use std::str::FromStr;

use crate::error::BundlrError;
use crate::tags::Tag;
use crate::utils::check_and_return;
use crate::BundlrTx;
use crate::{currency::Currency, transaction::poll::ConfirmationPoll};
use num::{BigRational, BigUint, ToPrimitive};
use num_traits::Zero;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(unused)]
pub struct Bundlr<'a> {
    url: Url,
    currency: &'a dyn Currency,
    client: reqwest::Client,
    pub_info: PubInfo,
}
#[derive(Deserialize)]
pub struct BalanceResData {
    balance: String,
}
#[allow(unused)]
#[derive(Deserialize)]
pub struct PubInfo {
    version: String,
    gateway: String,
    addresses: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub struct FundBody {
    tx_id: String,
}

impl Bundlr<'_> {
    pub async fn new(url: Url, currency: &dyn Currency) -> Bundlr {
        let pub_info = Bundlr::get_pub_info(&url)
            .await
            .unwrap_or_else(|_| panic!("Could not fetch public info from url: {}", url));

        Bundlr {
            url,
            currency,
            client: reqwest::Client::new(),
            pub_info,
        }
    }

    pub async fn get_pub_info(url: &Url) -> Result<PubInfo, BundlrError> {
        let client = reqwest::Client::new();
        let response = client
            .get(url.join("info").expect("Could not join url with /info"))
            .header("Content-Type", "application/json")
            .send()
            .await;

        check_and_return::<PubInfo>(response).await
    }

    pub fn create_transaction_with_tags(&self, data: Vec<u8>, tags: Vec<Tag>) -> BundlrTx {
        BundlrTx::create_with_tags(data, tags, self.currency.get_signer())
    }

    pub async fn send_transaction(&self, tx: BundlrTx) -> Result<Value, BundlrError> {
        let tx = tx.into_inner();

        let response = self
            .client
            .post(
                self.url
                    .join(&format!("tx/{}", self.currency.get_type()))
                    .expect("Could not join url with /tx/{}"),
            )
            .header("Content-Type", "application/octet-stream")
            .body(tx)
            .send()
            .await;

        check_and_return::<Value>(response).await
    }

    pub async fn get_balance_public(
        url: &Url,
        currency: &dyn Currency,
        address: &str,
        client: &reqwest::Client,
    ) -> Result<BigUint, BundlrError> {
        let response = client
            .get(
                url.join(&format!("account/balance/{}", currency.get_type()))
                    .expect("Could not join url with /account/balance/{}"),
            )
            .query(&[("address", address)])
            .header("Content-Type", "application/json")
            .send()
            .await;

        check_and_return::<BalanceResData>(response)
            .await
            .map(|d| BigUint::from_str(&d.balance).expect("Error converting from u128 to BigUint"))
    }

    pub async fn get_balance(&self, address: String) -> Result<BigUint, BundlrError> {
        Bundlr::get_balance_public(&self.url, self.currency, &address, &self.client).await
    }

    pub async fn fund(&self, amount: u64, multiplier: Option<f64>) -> Result<bool, BundlrError> {
        let multiplier = multiplier.unwrap_or(1.0);
        let curr_str = &self.currency.get_type().to_string().to_lowercase();
        let to = self
            .pub_info
            .addresses
            .get(curr_str)
            .expect("Address should not be empty");
        let fee: u64 = match self.currency.needs_fee() {
            true => self.currency.get_fee(amount, to, multiplier).await,
            false => Zero::zero(),
        };

        let tx = self.currency.create_tx(amount, to, fee).await;
        let tx_res = self
            .currency
            .send_tx(tx)
            .await
            .expect("Error while sending transaction");

        ConfirmationPoll::await_confirmation(&tx_res.tx_id, self.currency).await;
        let post_tx_res = self
            .client
            .post(
                self.url
                    .join(&format!("account/balance/{}", self.currency.get_type()))
                    .expect("Could not join url with /account/balance/{}"),
            )
            .body(format!("{{\"tx_id\":{}}}", &tx_res.tx_id))
            .send()
            .await;

        check_and_return::<String>(post_tx_res).await.map(|_| true)
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use crate::{currency::arweave::Arweave, tags::Tag, Bundlr};
    use httpmock::{
        Method::{GET, POST},
        MockServer,
    };
    use num::BigUint;
    use reqwest::Url;

    #[tokio::test]
    async fn should_send_transactions_correctly() {
        /*
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/tx/arweave");
            then.status(200)
                .header("Content-Type", "application/octet-stream")
                .body("{}");
        });
        let mock_2 = server.mock(|when, then| {
            when.method(GET)
                .path("/info");
            then.status(200)
                .body("{ \"version\": \"0\", \"gateway\": \"gateway\", \"addresses\": { \"arweave\": \"address\" }}");
        });

        let url = Url::from_str(&server.url("")).unwrap();
        let path = PathBuf::from_str("res/test_wallet.json").unwrap();
        let currency = Arweave::new(path, url.clone());
        let bundler = &Bundlr::new(url, &currency).await;
        let tx = bundler.create_transaction_with_tags(
            Vec::from("hello"),
            vec![Tag::new("name".to_string(), "value".to_string())],
        );
        let value = bundler.send_transaction(tx).await.unwrap();

        mock.assert();
        mock_2.assert();
        assert_eq!(value.to_string(), "{}");
        */
    }

    #[tokio::test]
    async fn should_fetch_balance_correctly() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/account/balance/arweave")
                .query_param("address", "address");
            then.status(200)
                .header("content-type", "application/json")
                .body("{ \"balance\": \"10\" }");
        });
        let mock_2 = server.mock(|when, then| {
            when.method(GET)
                .path("/info");
            then.status(200)
                .body("{ \"version\": \"0\", \"gateway\": \"gateway\", \"addresses\": { \"arweave\": \"address\" }}");  
        });

        let url = Url::from_str(&server.url("")).unwrap();
        let address = "address";
        let path = PathBuf::from_str("res/test_wallet.json").unwrap();
        println!("{:?}", &path);
        let currency = Arweave::new(path, Some(url.clone()));
        let bundler = &Bundlr::new(url, &currency).await;
        let balance = bundler.get_balance(address.to_string()).await.unwrap();

        mock.assert();
        mock_2.assert();
        assert_eq!(balance, "10".parse::<BigUint>().unwrap());
    }

    #[tokio::test]
    async fn should_fund_address_correctly() {}
}
