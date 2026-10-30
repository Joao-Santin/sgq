//! Client for the local e-Gestor gateway (`egestor-rust-server`).

use std::collections::HashMap;
use std::fs;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

pub const GATEWAY_URL: &str = "http://127.0.0.1:3000/api";
const PRODUCTS_CACHE: &str = "produtos_erp.json";

/// e-Gestor allows 60 requests/minute; on 429 we wait and retry.
const RATE_LIMIT_WAIT: Duration = Duration::from_secs(15);
const MAX_RETRIES: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub codigo: i64,
    #[serde(default)]
    pub descricao: String,
    #[serde(default, rename = "codigoProprio")]
    pub codigo_proprio: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situation {
    Open,
    Done,
}

impl Situation {
    pub fn label(self) -> &'static str {
        match self {
            Situation::Open => "Aberta",
            Situation::Done => "Concluída",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Production {
    pub code: i64,
    pub situation: Option<Situation>,
    pub start: String,
    pub items: Vec<ProductionItem>,
}

#[derive(Debug, Clone)]
pub struct ProductionItem {
    pub product: i64,
    pub description: String,
    pub own_code: String,
    pub quantity: f64,
    pub unit: String,
}

#[derive(Deserialize)]
struct Page<T> {
    last_page: u32,
    data: Vec<T>,
}

#[derive(Deserialize)]
struct ProductionSummary {
    codigo: i64,
    situacao: i64,
}

/// One entry of the `producoesDetalhadas` report. The report returns codes as
/// strings, so numeric fields accept both forms.
#[derive(Deserialize)]
struct ReportEntry {
    #[serde(deserialize_with = "int")]
    cod: i64,
    #[serde(default)]
    produto: Vec<ReportProduct>,
}

#[derive(Deserialize)]
struct ReportProduct {
    #[serde(rename = "codProduto", deserialize_with = "int")]
    cod_produto: i64,
    #[serde(rename = "IxProd", default)]
    descricao: String,
    #[serde(rename = "IcProd", default)]
    codigo_proprio: String,
    #[serde(rename = "IuCom", default)]
    unidade: String,
    #[serde(default, deserialize_with = "float")]
    quant: f64,
}

pub fn load_cached_products() -> Vec<Product> {
    fs::read_to_string(PRODUCTS_CACHE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Downloads every product registered in the ERP (all pages) and caches them on disk.
pub async fn fetch_products() -> Result<Vec<Product>, String> {
    let client = reqwest::Client::new();
    let mut products = Vec::new();
    let mut page = 1;

    loop {
        let url = format!("{GATEWAY_URL}/produtos?page={page}");
        let result: Page<Product> = send(client.get(&url)).await?;
        products.extend(result.data);

        if page >= result.last_page {
            break;
        }
        page += 1;
    }

    products.sort_by(|a, b| a.descricao.cmp(&b.descricao));

    if let Ok(json) = serde_json::to_string(&products) {
        let _ = fs::write(PRODUCTS_CACHE, json);
    }

    Ok(products)
}

/// Fetches the production orders (OPs) started between `from` and `to`
/// (YYYY-MM-DD), including every product of each OP.
pub async fn fetch_productions(from: String, to: String) -> Result<Vec<Production>, String> {
    let client = reqwest::Client::new();

    let report: Vec<ReportEntry> = send(
        client
            .post(format!("{GATEWAY_URL}/relatorios/producoesDetalhadas"))
            .json(&json!({ "de": from, "ate": to, "tipoData": "dtInicio" })),
    )
    .await?;

    // The report has no situation/date, so complement it with the OP list.
    let mut summaries: HashMap<i64, (i64, String)> = HashMap::new();
    let mut page = 1;
    loop {
        let url = format!("{GATEWAY_URL}/producao?dtIni={from}&dtFim={to}&page={page}");
        let result: Page<Value> = send(client.get(&url)).await?;
        for value in result.data {
            let start = value["dtInicio"].as_str().unwrap_or_default().to_owned();
            if let Ok(summary) = serde_json::from_value::<ProductionSummary>(value) {
                summaries.insert(summary.codigo, (summary.situacao, start));
            }
        }
        if page >= result.last_page {
            break;
        }
        page += 1;
    }

    let mut productions: Vec<Production> = report
        .into_iter()
        .map(|entry| {
            let (situation, start) = match summaries.remove(&entry.cod) {
                Some((50, start)) => (Some(Situation::Done), start),
                Some((10, start)) => (Some(Situation::Open), start),
                Some((_, start)) => (None, start),
                None => (None, String::new()),
            };

            Production {
                code: entry.cod,
                situation,
                start,
                items: entry
                    .produto
                    .into_iter()
                    .map(|p| ProductionItem {
                        product: p.cod_produto,
                        description: p.descricao.trim().to_owned(),
                        own_code: p.codigo_proprio,
                        quantity: p.quant,
                        unit: p.unidade,
                    })
                    .collect(),
            }
        })
        .collect();

    productions.sort_by_key(|p| std::cmp::Reverse(p.code));
    Ok(productions)
}

async fn send<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, String> {
    for _ in 0..MAX_RETRIES {
        let attempt = request
            .try_clone()
            .expect("requests have no streaming body")
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .map_err(connection_error)?;

        let status = attempt.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tokio::time::sleep(RATE_LIMIT_WAIT).await;
            continue;
        }

        let body = attempt.text().await.map_err(connection_error)?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v["errMsg"].as_str().map(str::to_owned))
                .unwrap_or(body);
            return Err(format!("O e-Gestor respondeu com erro {status}: {message}"));
        }

        return serde_json::from_str(&body)
            .map_err(|e| format!("Resposta inesperada do e-Gestor: {e}"));
    }

    Err("Limite de requisições do e-Gestor atingido. Tente novamente em um minuto.".into())
}

fn connection_error(e: reqwest::Error) -> String {
    if e.is_connect() {
        format!(
            "Não foi possível conectar ao gateway do e-Gestor ({GATEWAY_URL}). \
             Verifique se o egestor-rust-server está rodando."
        )
    } else if e.is_timeout() {
        "O e-Gestor demorou demais para responder. Tente novamente.".into()
    } else {
        format!("Erro de comunicação com o e-Gestor: {e}")
    }
}

fn int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64().ok_or_else(|| serde::de::Error::custom("inteiro inválido")),
        Value::String(s) => s.trim().parse().map_err(serde::de::Error::custom),
        other => Err(serde::de::Error::custom(format!("esperava número, veio {other}"))),
    }
}

fn float<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => Ok(n.as_f64().unwrap_or_default()),
        Value::String(s) => s.trim().parse().map_err(serde::de::Error::custom),
        Value::Null => Ok(0.0),
        other => Err(serde::de::Error::custom(format!("esperava número, veio {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_report_entry() {
        let json = r#"[{"cod":"1326","insumos":[],"produto":[
            {"tipo":"Produto","codProduto":"658","IxProd":"LENTE RETANGULAR LARANJA ",
             "IcProd":"2100658","IuCom":"UN","quant":3500}]}]"#;
        let entries: Vec<ReportEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries[0].cod, 1326);
        assert_eq!(entries[0].produto[0].cod_produto, 658);
        assert_eq!(entries[0].produto[0].quant, 3500.0);
    }
}

/// Hits the real gateway (read-only). Run with: cargo test -- --ignored live_gateway
#[cfg(test)]
mod live {
    #[tokio::test(flavor = "current_thread")]
    #[ignore = "requires egestor-rust-server running"]
    async fn live_gateway() {
        let productions = super::fetch_productions("2026-10-01".into(), "2026-10-30".into())
            .await
            .unwrap();
        let unknown = productions.iter().filter(|p| p.situation.is_none()).count();
        let items: usize = productions.iter().map(|p| p.items.len()).sum();
        println!("OPs: {} · produtos nas OPs: {items} · sem situação: {unknown}", productions.len());

        let products = super::fetch_products().await.unwrap();
        println!("Produtos no ERP: {}", products.len());
    }
}
