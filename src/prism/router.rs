use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use rand::{rng, Rng};
use regex::Regex;

use crate::prism::config;

#[derive(Debug, Clone)]
pub struct Resolution {
    pub upstreams: Vec<String>,
    pub cache_ping_ttl: Option<Duration>,
    pub matched_host: String,
}

#[derive(Debug)]
pub struct Router {
    compiled: ArcSwap<CompiledRoutes>,
}

#[derive(Debug, Default)]
struct CompiledRoutes {
    routes: Vec<CompiledRoute>,
}

#[derive(Debug)]
struct CompiledRoute {
    patterns: Vec<CompiledPattern>,
    upstreams: Vec<String>,
    strategy: Strategy,
    cache_ping_ttl: Option<Duration>,
    rr: AtomicU64,
}

#[derive(Debug)]
struct CompiledPattern {
    pattern: String,
    exact: bool,
    re: Option<Regex>,
}

#[derive(Debug, Clone, Copy)]
enum Strategy {
    Sequential,
    Random,
    RoundRobin,
}

impl Router {
    pub fn new(routes: Vec<config::RouteConfig>) -> Self {
        let r = Self {
            compiled: ArcSwap::from_pointee(CompiledRoutes::default()),
        };
        r.update(routes);
        r
    }

    pub fn update(&self, routes: Vec<config::RouteConfig>) {
        let mut out = Vec::new();
        for rt in routes {
            if let Ok(c) = compile_route(&rt) {
                out.push(c);
            }
        }
        self.compiled.store(Arc::new(CompiledRoutes { routes: out }));
    }

    pub fn resolve(&self, host: &str) -> Option<Resolution> {
        let cr = self.compiled.load();
        if cr.routes.is_empty() {
            return None;
        }

        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return None;
        }

        for rt in &cr.routes {
            for p in &rt.patterns {
                let (matched, groups) = match_host(&host, p);
                if !matched {
                    continue;
                }

                let mut candidates = Vec::with_capacity(rt.upstreams.len());
                for u in &rt.upstreams {
                    candidates.push(substitute_params(u, &groups));
                }
                let candidates = order_candidates(rt, candidates);

                return Some(Resolution {
                    upstreams: candidates,
                    cache_ping_ttl: rt.cache_ping_ttl,
                    matched_host: p.pattern.clone(),
                });
            }
        }

        None
    }
}

fn compile_route(rt: &config::RouteConfig) -> anyhow::Result<CompiledRoute> {
    let mut patterns = Vec::new();
    for h in &rt.host {
        let h = h.trim().to_ascii_lowercase();
        if h.is_empty() {
            continue;
        }
        if !h.contains('*') && !h.contains('?') {
            patterns.push(CompiledPattern {
                pattern: h,
                exact: true,
                re: None,
            });
            continue;
        }

        let re = compile_wildcard_pattern(&h)?;
        patterns.push(CompiledPattern {
            pattern: h,
            exact: false,
            re: Some(re),
        });
    }
    if patterns.is_empty() {
        anyhow::bail!("router: route missing host patterns");
    }

    let upstreams: Vec<String> = rt
        .upstreams
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if upstreams.is_empty() {
        anyhow::bail!("router: route missing upstreams");
    }

    Ok(CompiledRoute {
        patterns,
        upstreams,
        strategy: parse_strategy(&rt.strategy),
        cache_ping_ttl: rt.cache_ping_ttl,
        rr: AtomicU64::new(0),
    })
}

fn parse_strategy(s: &str) -> Strategy {
    let mut s = s.trim().to_ascii_lowercase();
    s = s.replace('_', "-");
    s = s.replace(' ', "-");
    while s.contains("--") {
        s = s.replace("--", "-");
    }

    match s.as_str() {
        "" | "sequential" => Strategy::Sequential,
        "random" => Strategy::Random,
        "round-robin" | "roundrobin" => Strategy::RoundRobin,
        _ => Strategy::Sequential,
    }
}

fn compile_wildcard_pattern(pattern: &str) -> anyhow::Result<Regex> {
    let pattern = pattern.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        anyhow::bail!("router: empty pattern");
    }

    let mut out = String::with_capacity(pattern.len() + 16);
    out.push('^');

    let mut escape_next = false;
    for ch in pattern.chars() {
        if escape_next {
            out.push(ch);
            escape_next = false;
            continue;
        }
        match ch {
            '*' => out.push_str("(.*?)"),
            '?' => out.push_str("(.)"),
            '\\' => {
                escape_next = true;
                out.push('\\');
            }
            other => {
                if ".^$+()[]{}|".contains(other) {
                    out.push('\\');
                }
                out.push(other);
            }
        }
    }

    out.push('$');
    Ok(Regex::new(&out)?)
}

fn match_host(host: &str, p: &CompiledPattern) -> (bool, Vec<String>) {
    if p.exact {
        return (host == p.pattern, Vec::new());
    }
    let Some(re) = &p.re else {
        return (false, Vec::new());
    };

    let Some(caps) = re.captures(host) else {
        return (false, Vec::new());
    };

    let mut groups = Vec::new();
    for i in 1..caps.len() {
        if let Some(m) = caps.get(i) {
            groups.push(m.as_str().to_string());
        }
    }

    (true, groups)
}

fn substitute_params(template: &str, groups: &[String]) -> String {
    if template.is_empty() || groups.is_empty() {
        return template.to_string();
    }

    // Replace from the end so $10 doesn't interfere with $1.
    let mut res = template.to_string();
    for i in (1..=groups.len()).rev() {
        res = res.replace(&format!("${i}"), &groups[i - 1]);
    }
    res
}

fn order_candidates(rt: &CompiledRoute, candidates: Vec<String>) -> Vec<String> {
    if candidates.len() <= 1 {
        return candidates;
    }

    match rt.strategy {
        Strategy::Sequential => candidates,
        Strategy::Random => {
            let start = rng().random_range(0..candidates.len());
            rotate(candidates, start)
        }
        Strategy::RoundRobin => {
            let start = (rt.rr.fetch_add(1, Ordering::Relaxed) as usize) % candidates.len();
            rotate(candidates, start)
        }
    }
}

fn rotate(mut in_vec: Vec<String>, start: usize) -> Vec<String> {
    let n = in_vec.len();
    if n == 0 {
        return in_vec;
    }
    let start = start % n;
    if start == 0 {
        return in_vec;
    }

    let mut out = Vec::with_capacity(n);
    out.extend(in_vec.drain(start..));
    out.extend(in_vec.into_iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_substitution() {
        let cfg = config::RouteConfig {
            host: vec!["*.labs.example.com".into()],
            upstreams: vec!["$1.backend:25565".into()],
            strategy: "sequential".into(),
            cache_ping_ttl: Some(Duration::from_secs(1)),
        };
        let r = Router::new(vec![cfg]);
        let res = r.resolve("play.labs.example.com").expect("match");
        assert_eq!(res.upstreams[0], "play.backend:25565");
    }
}
