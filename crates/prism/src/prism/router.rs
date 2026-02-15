use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use arc_swap::ArcSwap;
use rand::{RngExt, rng};
use regex::Regex;

use crate::prism::config;
use crate::prism::middleware::{MiddlewareError, SharedMiddlewareChain};

#[derive(Clone)]
pub struct Resolution {
    pub host: String,
    pub upstreams: Vec<String>,
    #[allow(dead_code)]
    pub matched_host: String,
    pub captures: Vec<String>,
    pub middleware: SharedMiddlewareChain,
    pub prelude_override: Option<Vec<u8>>,
}

pub struct Router {
    compiled: ArcSwap<CompiledRoutes>,
}

#[derive(Default)]
struct CompiledRoutes {
    routes: Vec<CompiledRoute>,
}

struct CompiledRoute {
    patterns: Vec<CompiledPattern>,
    upstreams: Vec<String>,
    strategy: Strategy,
    rr: AtomicU64,
    middleware: SharedMiddlewareChain,
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
    pub fn new(routes: Vec<(config::RouteConfig, SharedMiddlewareChain)>) -> Self {
        let r = Self {
            compiled: ArcSwap::from_pointee(CompiledRoutes::default()),
        };
        r.update(routes);
        r
    }

    pub fn update(&self, routes: Vec<(config::RouteConfig, SharedMiddlewareChain)>) {
        let mut out = Vec::new();
        for (rt, middleware) in routes {
            if let Ok(c) = compile_route(&rt, middleware) {
                out.push(c);
            }
        }
        self.compiled
            .store(Arc::new(CompiledRoutes { routes: out }));
    }

    /// Resolve an incoming connection by repeatedly trying each route's configured parser chain.
    ///
    /// Returns:
    /// - Ok(Some(resolution)) when a route parses and matches
    /// - Ok(None) when no routes can match this prelude (and no route needs more data)
    /// - Err(NeedMoreData) when at least one route needs more bytes to decide
    pub fn resolve_prelude(&self, prelude: &[u8]) -> Result<Option<Resolution>, MiddlewareError> {
        let cr = self.compiled.load();
        if cr.routes.is_empty() {
            return Ok(None);
        }

        let mut need_more = false;
        for rt in &cr.routes {
            match rt.middleware.parse(prelude) {
                Ok((host, prelude_override)) => {
                    if let Some(mut res) = resolve_route_for_host(rt, &host) {
                        res.prelude_override = prelude_override;
                        return Ok(Some(res));
                    }
                }
                Err(MiddlewareError::NeedMoreData) => {
                    need_more = true;
                }
                Err(MiddlewareError::NoMatch) => {}
                Err(MiddlewareError::Fatal(_)) => {
                    // Treat per-route middleware failures as non-matches so other routes can still win.
                }
            }
        }

        if need_more {
            Err(MiddlewareError::NeedMoreData)
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)]
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
            if let Some(res) = resolve_route_for_host(rt, &host) {
                return Some(res);
            }
        }

        None
    }
}

fn compile_route(
    rt: &config::RouteConfig,
    middleware: SharedMiddlewareChain,
) -> anyhow::Result<CompiledRoute> {
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
        rr: AtomicU64::new(0),
        middleware,
    })
}

fn resolve_route_for_host(rt: &CompiledRoute, host: &str) -> Option<Resolution> {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }

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
            host: host.to_string(),
            upstreams: candidates,
            matched_host: p.pattern.clone(),
            captures: groups,
            middleware: rt.middleware.clone(),
            prelude_override: None,
        });
    }

    None
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

pub(crate) fn substitute_params(template: &str, groups: &[String]) -> String {
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
    out.extend(in_vec);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn wildcard_substitution() {
        let cfg = config::RouteConfig {
            host: vec!["*.labs.example.com".into()],
            upstreams: vec!["$1.backend:25565".into()],
            strategy: "sequential".into(),
            middlewares: vec!["noop".into()],
        };

        struct NoopChain;
        impl crate::prism::middleware::MiddlewareChain for NoopChain {
            fn name(&self) -> &str {
                "noop"
            }

            fn parse(
                &self,
                _prelude: &[u8],
            ) -> Result<(String, Option<Vec<u8>>), crate::prism::middleware::MiddlewareError>
            {
                Err(crate::prism::middleware::MiddlewareError::NoMatch)
            }

            fn rewrite(&self, _prelude: &[u8], _selected_upstream: &str) -> Option<Vec<u8>> {
                None
            }
        }

        let chain = Arc::new(NoopChain) as crate::prism::middleware::SharedMiddlewareChain;
        let r = Router::new(vec![(cfg, chain)]);
        let res = r.resolve("play.labs.example.com").expect("match");
        assert_eq!(res.upstreams[0], "play.backend:25565");
    }
}
