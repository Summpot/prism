using System;
using System.Collections.Specialized;
using System.Text.Encodings.Web;
using System.Web;
using Prism.Native;

namespace Prism.Services;

public class ParsedPrismLink
{
    public string Name { get; set; } = "";
    public string ServerAddr { get; set; } = "";
    public string Transport { get; set; } = "auto";
    public string AuthToken { get; set; } = "";
    public string ListenAddr { get; set; } = "127.0.0.1:25565";
    public bool FakeLanBroadcast { get; set; } = true;
    public string? ManagementUrl { get; set; }
}

public static class PrismLinkService
{
    public static string Encode(string name, string serverAddr, string transport, string listenAddr, bool fakeLanBroadcast, string? authToken = null)
    {
        var server = string.IsNullOrWhiteSpace(serverAddr) ? "127.0.0.1:7000" : serverAddr.Trim();
        var query = HttpUtility.ParseQueryString(string.Empty);

        if (!string.IsNullOrWhiteSpace(name))
        {
            query["name"] = name.Trim();
        }
        if (!string.IsNullOrWhiteSpace(transport) && !transport.Equals("auto", StringComparison.OrdinalIgnoreCase))
        {
            query["transport"] = transport.ToLowerInvariant();
        }
        if (!string.IsNullOrWhiteSpace(listenAddr) && !listenAddr.Equals("127.0.0.1:25565", StringComparison.OrdinalIgnoreCase))
        {
            query["listen"] = listenAddr.Trim();
        }
        if (!fakeLanBroadcast)
        {
            query["fake_lan"] = "0";
        }
        if (!string.IsNullOrWhiteSpace(authToken))
        {
            query["token"] = authToken.Trim();
        }

        var qs = query.ToString();
        return string.IsNullOrEmpty(qs) ? $"prism://{server}" : $"prism://{server}?{qs}";
    }

    public static ParsedPrismLink? Parse(string raw)
    {
        if (string.IsNullOrWhiteSpace(raw)) return null;
        var trimmed = raw.Trim();

        // 1. prism:// or scheme:// link
        var match = System.Text.RegularExpressions.Regex.Match(trimmed, @"^(prism|auto|wt|webtransport|quic|tcp|kcp|ws|wss)://(?<rest>.+)$", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        if (match.Success)
        {
            var scheme = match.Groups[1].Value.ToLowerInvariant();
            var rest = match.Groups["rest"].Value;

            string hostAndPort = rest;
            string queryString = "";
            int qIndex = rest.IndexOf('?');
            if (qIndex >= 0)
            {
                hostAndPort = rest.Substring(0, qIndex).TrimEnd('/');
                queryString = rest.Substring(qIndex + 1);
            }
            else
            {
                hostAndPort = rest.TrimEnd('/');
            }

            var query = HttpUtility.ParseQueryString(queryString);

            var defaultTransport = scheme switch
            {
                "prism" or "auto" => "auto",
                "wt" or "webtransport" => "webtransport",
                "ws" or "wss" => "ws",
                _ => scheme
            };

            var transport = query["transport"] ?? defaultTransport;
            var name = query["name"] ?? hostAndPort;
            var listen = query["listen"] ?? "127.0.0.1:25565";
            var token = query["token"] ?? "";
            var fakeLanStr = query["fake_lan"];
            bool fakeLan = fakeLanStr != "0" && !string.Equals(fakeLanStr, "false", StringComparison.OrdinalIgnoreCase);

            var mgmt = query["management_url"] ?? query["mgmt"];

            return new ParsedPrismLink
            {
                Name = name,
                ServerAddr = hostAndPort,
                Transport = transport,
                AuthToken = token,
                ListenAddr = listen,
                FakeLanBroadcast = fakeLan,
                ManagementUrl = !string.IsNullOrWhiteSpace(mgmt) ? mgmt : null
            };
        }

        // 2. http:// or https:// URL
        if (trimmed.StartsWith("http://", StringComparison.OrdinalIgnoreCase) || trimmed.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
        {
            try
            {
                var uri = new Uri(trimmed);
                return new ParsedPrismLink
                {
                    Name = uri.Host,
                    ServerAddr = uri.Host,
                    Transport = "auto",
                    ListenAddr = "127.0.0.1:25565",
                    FakeLanBroadcast = true,
                    ManagementUrl = trimmed.TrimEnd('/')
                };
            }
            catch
            {
                return null;
            }
        }

        // 3. raw host:port or host
        if (!trimmed.Contains(' '))
        {
            return new ParsedPrismLink
            {
                Name = trimmed,
                ServerAddr = trimmed,
                Transport = "auto",
                ListenAddr = "127.0.0.1:25565",
                FakeLanBroadcast = true,
                ManagementUrl = null
            };
        }

        return null;
    }

    public static DeepLinkResult ParseDeepLink(string rawUrl)
    {
        if (string.IsNullOrWhiteSpace(rawUrl)) return new DeepLinkResult { Kind = "unknown" };
        var trimmed = rawUrl.Trim();

        string target = trimmed;
        bool isAuth = false;

        if (trimmed.StartsWith("prism://", StringComparison.OrdinalIgnoreCase))
        {
            var rest = trimmed.Substring("prism://".Length);
            var path = rest.Split(new[] { '?', '#' })[0].TrimEnd('/').ToLowerInvariant();
            if (path is "auth" or "auth/callback" or "auth/github/callback" or "oauth" or "oauth/callback" or "login")
            {
                isAuth = true;
                target = rest;
            }
        }
        else if (trimmed.StartsWith("http://", StringComparison.OrdinalIgnoreCase) || trimmed.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
        {
            try
            {
                var u = new Uri(trimmed);
                var path = u.AbsolutePath.Trim('/').ToLowerInvariant();
                if (path is "auth/callback" or "auth/github/callback" or "oauth/callback" or "login")
                {
                    isAuth = true;
                    target = u.PathAndQuery;
                }
            }
            catch { }
        }

        if (isAuth)
        {
            int qIndex = target.IndexOf('?');
            var queryString = qIndex >= 0 ? target.Substring(qIndex + 1) : "";
            var query = HttpUtility.ParseQueryString(queryString);

            var token = query["token"];
            var code = query["code"];
            var state = query["state"];
            var user = query["username"] ?? query["user_id"];
            var role = query["role"];

            if (!string.IsNullOrWhiteSpace(token))
            {
                return new DeepLinkResult
                {
                    Kind = "auth",
                    Token = token,
                    Username = user,
                    Role = role
                };
            }

            if (!string.IsNullOrWhiteSpace(code))
            {
                return new DeepLinkResult
                {
                    Kind = "auth-code",
                    Code = code,
                    State = state
                };
            }
        }

        // Check if it's a profile
        var profile = Parse(trimmed);
        if (profile != null && !string.IsNullOrWhiteSpace(profile.ServerAddr))
        {
            return new DeepLinkResult
            {
                Kind = "profile",
                Profile = profile
            };
        }

        return new DeepLinkResult { Kind = "unknown" };
    }
}

public class DeepLinkResult
{
    public string Kind { get; set; } = "unknown";
    public string? Token { get; set; }
    public string? Code { get; set; }
    public string? State { get; set; }
    public string? Username { get; set; }
    public string? Role { get; set; }
    public ParsedPrismLink? Profile { get; set; }
}

