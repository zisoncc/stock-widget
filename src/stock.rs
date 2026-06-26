//! Stock data model and fetcher - Tencent Finance API

/// Represents a single stock quote
#[derive(Debug, Clone)]
pub struct StockInfo {
    pub symbol: String,
    pub name: String,
    pub price: f64,
    pub change: f64,
    pub change_percent: f64,
    /// true = up (red in Chinese convention), false = down (green)
    pub is_up: bool,
}

/// Map a stock symbol to Tencent Finance format
pub fn to_tencent_symbol(symbol: &str) -> String {
    let s = symbol.trim().to_uppercase();
    
    // Chinese A-shares (6-digit codes)
    if s.len() == 6 && s.parse::<u32>().is_ok() {
        let n = s.parse::<u32>().unwrap();
        if n >= 500000 {
            return format!("sh{}", s);
        }
        return format!("sz{}", s);
    }
    
    // US stocks - Tencent uses sh prefix with lowercase
    if s.chars().all(|c| c.is_ascii_alphabetic()) && s.len() <= 5 && !s.is_empty() {
        return format!("us_{}", s.to_lowercase());
    }
    
    s
}

fn parse_tencent_line(line: &str) -> Option<StockInfo> {
    let line = line.trim();
    let dq = b'"' as char;
    if line.is_empty() || !line.contains(dq) {
        return None;
    }
    
    let qpos = line.find(dq)?;
    let rest = &line[qpos+1..];
    let content = rest.split(dq).next().unwrap_or("");
    
    let fields: Vec<&str> = content.split('~').collect();
    if fields.len() < 45 {
        return None;
    }
    
    // Tencent qt.gtimg.cn field positions:
    // [1]=name, [2]=code, [31]=change, [32]=change_percent, [33]=price
    let name = fields.get(1).unwrap_or(&"").to_string();
    let code = fields.get(2).unwrap_or(&"").to_string();
    let price = fields.get(33).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let change = fields.get(31).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let change_pct = fields.get(32).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    
    if price <= 0.0 && name.is_empty() {
        return None;
    }
    
    let display_symbol = code.strip_prefix("sh")
        .or_else(|| code.strip_prefix("sz"))
        .or_else(|| code.strip_prefix("us_"))
        .unwrap_or(&code);
    
    Some(StockInfo {
        symbol: display_symbol.to_string(),
        name,
        price,
        change,
        change_percent: change_pct,
        is_up: change >= 0.0,
    })
}

pub fn fetch_stocks(symbols: &[String]) -> Vec<StockInfo> {
    if symbols.is_empty() {
        return Vec::new();
    }
    
    let tencent_symbols: Vec<String> = symbols.iter()
        .map(|s| to_tencent_symbol(s))
        .collect();
    let query = tencent_symbols.join(",");
    
    let url = format!("https://qt.gtimg.cn/q={}", query);
    
    let client = match reqwest::blocking::Client::builder()
        .user_agent("StockWidget/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {}", e);
            return Vec::new();
        }
    };
    
    let resp = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("HTTP request failed: {}", e);
            return Vec::new();
        }
    };
    
    let body = match resp.text() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read response: {}", e);
            return Vec::new();
        }
    };
    
    let mut stocks = Vec::new();
    
    for line in body.lines() {
        if let Some(stock) = parse_tencent_line(line) {
            stocks.push(stock);
        }
    }
    
    stocks
}
