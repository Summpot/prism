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
        let mut skipped = 0usize;
        for (rt, middleware) in routes {
            match compile_route(&rt, middleware) {
                Ok(c) => {
                    tracing::info!(
                        patterns = ?c.patterns.iter().map(|p| p.pattern.as_str()).collect::<Vec<_>>(),
                        upstreams = ?c.upstreams,
                        strategy = ?c.strategy,
                        "router: loaded route"
                    );
                    out.push(c);
                }
                Err(err) => {
                    skipped += 1;
                    tracing::warn!(
                        err = %err,
                        hosts = ?rt.host,
                        upstreams = ?rt.upstreams,
                        "router: skipping invalid route"
                    );
                }
            }
        }
        tracing::info!(routes = out.len(), skipped, "router: route table updated");
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
            tracing::debug!(prelude_len = prelude.len(), "router: no routes configured");
            return Ok(None);
        }

        let mut need_more = false;
        let mut last_parsed_host: Option<String> = None;
        let mut parse_hits = 0usize;
        let mut pattern_misses = 0usize;

        for (idx, rt) in cr.routes.iter().enumerate() {
            match rt.middleware.parse(prelude) {
                Ok((host, prelude_override)) => {
                    parse_hits += 1;
                    let host = normalize_routing_host(&host);
                    if host.is_empty() {
                        tracing::debug!(
                            route_index = idx,
                            "router: middleware returned empty host"
                        );
                        continue;
                    }
                    last_parsed_host = Some(host.clone());

                    if let Some(mut res) = resolve_route_for_host(rt, &host) {
                        res.prelude_override = prelude_override;
                        tracing::info!(
                            route_index = idx,
                            host = %res.host,
                            matched_host = %res.matched_host,
                            captures = ?res.captures,
                            upstreams = ?res.upstreams,
                            prelude_len = prelude.len(),
                            "router: matched route"
                        );
                        return Ok(Some(res));
                    }

                    pattern_misses += 1;
                    tracing::debug!(
                        route_index = idx,
                        host = %host,
                        patterns = ?rt.patterns.iter().map(|p| p.pattern.as_str()).collect::<Vec<_>>(),
                        "router: parsed host did not match route patterns"
                    );
                }
                Err(MiddlewareError::NeedMoreData) => {
                    need_more = true;
                    tracing::trace!(
                        route_index = idx,
                        prelude_len = prelude.len(),
                        "router: middleware needs more data"
                    );
                }
                Err(MiddlewareError::NoMatch) => {
                    tracing::trace!(
                        route_index = idx,
                        prelude_len = prelude.len(),
                        "router: middleware no-match"
                    );
                }
                Err(MiddlewareError::Fatal(err)) => {
                    // Treat per-route middleware failures as non-matches so other routes can still win.
                    tracing::debug!(
                        route_index = idx,
                        err = %err,
                        "router: middleware fatal error treated as non-match"
                    );
                }
            }
        }

        if need_more {
            Err(MiddlewareError::NeedMoreData)
        } else {
            if let Some(host) = last_parsed_host.as_deref() {
                tracing::debug!(
                    host = %host,
                    routes = cr.routes.len(),
                    parse_hits,
                    pattern_misses,
                    prelude_len = prelude.len(),
                    "router: no route matched parsed host"
                );
            } else {
                tracing::debug!(
                    routes = cr.routes.len(),
                    parse_hits,
                    prelude_len = prelude.len(),
                    "router: no middleware produced a routing host"
                );
            }
            Ok(None)
        }
    }

    #[allow(dead_code)]
    pub fn resolve(&self, host: &str) -> Option<Resolution> {
        let cr = self.compiled.load();
        if cr.routes.is_empty() {
            return None;
        }

        let host = normalize_routing_host(host);
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
        let h = normalize_routing_host(h);
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
        tracing::debug!(
            pattern = %h,
            regex = %re.as_str(),
            "router: compiled wildcard pattern"
        );
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
    let host = normalize_routing_host(host);
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

/// Normalize a host used for route matching.
///
/// - trim + lowercase
/// - strip trailing FQDN dots
/// - strip a trailing `:port` when the port is all digits (Minecraft clients often
///   put `host:port` into the handshake address string)
/// - unwrap `[ipv6]:port` / `[ipv6]` brackets
pub(crate) fn normalize_routing_host(host: &str) -> String {
    let mut h = host.trim().trim_matches('\0').to_ascii_lowercase();
    while h.len() > 1 && h.ends_with('.') {
        h.pop();
    }
    if h.is_empty() {
        return h;
    }

    // Bracketed IPv6, optionally with :port.
    if h.starts_with('[') {
        if let Some(end) = h.find(']') {
            let inner = h[1..end].to_string();
            let rest = &h[end + 1..];
            if rest.is_empty() {
                return inner;
            }
            if let Some(port) = rest.strip_prefix(':')
                && !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit())
            {
                return inner;
            }
        }
        return h;
    }

    // host:port (not bare IPv6, which contains multiple colons)
    if let Some((name, port)) = h.rsplit_once(':')
        && !name.is_empty()
        && !name.contains(':')
        && !port.is_empty()
        && port.chars().all(|c| c.is_ascii_digit())
    {
        return name.to_string();
    }

    h
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
            // Non-greedy so multi-star patterns behave predictably; a single leading
            // `*.example.com` still captures the full left-hand label group.
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

    fn noop_router(cfg: config::RouteConfig) -> Router {
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
        Router::new(vec![(cfg, chain)])
    }

    #[test]
    fn wildcard_substitution() {
        let cfg = config::RouteConfig {
            host: vec!["*.labs.example.com".into()],
            upstreams: vec!["$1.backend:25565".into()],
            strategy: "sequential".into(),
            middlewares: vec!["noop".into()],
        };

        let r = noop_router(cfg);
        let res = r.resolve("play.labs.example.com").expect("match");
        assert_eq!(res.upstreams[0], "play.backend:25565");
        assert_eq!(res.captures, vec!["play".to_string()]);
    }

    #[test]
    fn wildcard_tunnel_service_substitution() {
        let cfg = config::RouteConfig {
            host: vec!["*.example.com".into()],
            upstreams: vec!["tunnel:$1".into()],
            strategy: "sequential".into(),
            middlewares: vec!["noop".into()],
        };

        let r = noop_router(cfg);
        let res = r.resolve("atm10sky.example.com").expect("match");
        assert_eq!(res.upstreams[0], "tunnel:atm10sky");
        assert_eq!(res.captures, vec!["atm10sky".to_string()]);

        let res = r.resolve("gto.example.com").expect("match");
        assert_eq!(res.upstreams[0], "tunnel:gto");
    }

    #[test]
    fn wildcard_strips_handshake_port_suffix() {
        let cfg = config::RouteConfig {
            host: vec!["*.example.com".into()],
            upstreams: vec!["tunnel:$1".into()],
            strategy: "sequential".into(),
            middlewares: vec!["noop".into()],
        };

        let r = noop_router(cfg);
        // Minecraft clients sometimes embed host:port in the handshake address string.
        let res = r
            .resolve("atm10sky.example.com:25565")
            .expect("match with port");
        assert_eq!(res.host, "atm10sky.example.com");
        assert_eq!(res.upstreams[0], "tunnel:atm10sky");
    }

    #[test]
    fn normalize_routing_host_variants() {
        assert_eq!(
            normalize_routing_host(" ATM10Sky.Example.COM. "),
            "atm10sky.example.com"
        );
        assert_eq!(
            normalize_routing_host("atm10sky.example.com:25565"),
            "atm10sky.example.com"
        );
        assert_eq!(normalize_routing_host("[::1]:25565"), "::1");
        assert_eq!(normalize_routing_host("2001:db8::1"), "2001:db8::1");
    }
}
