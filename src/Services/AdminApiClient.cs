using System;
using System.Collections.Generic;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading.Tasks;
using Prism.Native;

namespace Prism.Services;

public class AdminConnectionRecord
{
    [JsonPropertyName("id")] public string Id { get; set; } = "";
    [JsonPropertyName("client")] public string Client { get; set; } = "";
    [JsonPropertyName("host")] public string Host { get; set; } = "";
    [JsonPropertyName("upstream")] public string Upstream { get; set; } = "";
    [JsonPropertyName("started_at_unix_ms")] public long StartedAtUnixMs { get; set; }
    [JsonPropertyName("raw_bytes")] public ulong RawBytes { get; set; }
    [JsonPropertyName("wire_bytes")] public ulong WireBytes { get; set; }
    [JsonPropertyName("uplink_raw_bytes")] public ulong UplinkRawBytes { get; set; }
    [JsonPropertyName("uplink_wire_bytes")] public ulong UplinkWireBytes { get; set; }
    [JsonPropertyName("downlink_raw_bytes")] public ulong DownlinkRawBytes { get; set; }
    [JsonPropertyName("downlink_wire_bytes")] public ulong DownlinkWireBytes { get; set; }
}

public class AdminRegisteredService
{
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("proto")] public string Proto { get; set; } = "TCP";
    [JsonPropertyName("local_addr")] public string LocalAddr { get; set; } = "";
    [JsonPropertyName("remote_addr")] public string RemoteAddr { get; set; } = "";
    [JsonPropertyName("masquerade_host")] public string MasqueradeHost { get; set; } = "";
    [JsonPropertyName("route_only")] public bool RouteOnly { get; set; }
}

public class AdminServiceSnapshot
{
    [JsonPropertyName("service")] public AdminRegisteredService Service { get; set; } = new();
    [JsonPropertyName("client_id")] public string ClientId { get; set; } = "";
    [JsonPropertyName("remote")] public string Remote { get; set; } = "";
    [JsonPropertyName("primary")] public bool Primary { get; set; }
}

public class AdminUserRecord
{
    [JsonPropertyName("id")] public string Id { get; set; } = "";
    [JsonPropertyName("username")] public string Username { get; set; } = "";
    [JsonPropertyName("display_name")] public string? DisplayName { get; set; }
    [JsonPropertyName("avatar_url")] public string? AvatarUrl { get; set; }
    [JsonPropertyName("role")] public string Role { get; set; } = "member";
    [JsonPropertyName("service_rules")] public List<string> ServiceRules { get; set; } = new();
    [JsonPropertyName("created_at_unix_ms")] public long CreatedAtUnixMs { get; set; }
    [JsonPropertyName("last_login_unix_ms")] public long LastLoginUnixMs { get; set; }
}

public class AdminTokenRecord
{
    [JsonPropertyName("id")] public string Id { get; set; } = "";
    [JsonPropertyName("user_id")] public string UserId { get; set; } = "";
    [JsonPropertyName("token_type")] public string TokenType { get; set; } = "client";
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("device_id")] public string? DeviceId { get; set; }
    [JsonPropertyName("service_rules")] public List<string>? ServiceRules { get; set; }
    [JsonPropertyName("created_at_unix_ms")] public long CreatedAtUnixMs { get; set; }
    [JsonPropertyName("expires_at_unix_ms")] public long? ExpiresAtUnixMs { get; set; }
    [JsonPropertyName("last_used_unix_ms")] public long LastUsedUnixMs { get; set; }
}

public class AdminCreateTokenResponse
{
    [JsonPropertyName("raw_token")] public string RawToken { get; set; } = "";
    [JsonPropertyName("token")] public AdminTokenRecord Token { get; set; } = new();
}

public class AdminRevokeTokenResponse
{
    [JsonPropertyName("revoked")] public bool Revoked { get; set; }
}

public class AdminHealthResponse
{
    [JsonPropertyName("ok")] public bool Ok { get; set; }
}

public class AdminConfigPathResponse
{
    [JsonPropertyName("path")] public string Path { get; set; } = "";
}

public class AdminReloadResponse
{
    [JsonPropertyName("seq")] public long Seq { get; set; }
}

public class AdminSessionResponse
{
    [JsonPropertyName("authenticated")] public bool Authenticated { get; set; }
    [JsonPropertyName("user_id")] public string? UserId { get; set; }
    [JsonPropertyName("username")] public string? Username { get; set; }
    [JsonPropertyName("display_name")] public string? DisplayName { get; set; }
    [JsonPropertyName("avatar_url")] public string? AvatarUrl { get; set; }
    [JsonPropertyName("role")] public string? Role { get; set; }
    [JsonPropertyName("service_rules")] public List<string>? ServiceRules { get; set; }
    [JsonPropertyName("is_admin")] public bool IsAdmin { get; set; }
}

public class AdminAuthProvidersResponse
{
    [JsonPropertyName("github_enabled")] public bool GithubEnabled { get; set; }
    [JsonPropertyName("github_client_id")] public string? GithubClientId { get; set; }
    [JsonPropertyName("mode")] public string Mode { get; set; } = "token";
    [JsonPropertyName("providers")] public List<string> Providers { get; set; } = new();
}

public class Quantiles
{
    [JsonPropertyName("p50_us")] public ulong P50Us { get; set; }
    [JsonPropertyName("p90_us")] public ulong P90Us { get; set; }
    [JsonPropertyName("p99_us")] public ulong P99Us { get; set; }
    [JsonPropertyName("max_us")] public ulong MaxUs { get; set; }
}

public class DirectionStatsSnapshot
{
    [JsonPropertyName("raw_bytes")] public ulong RawBytes { get; set; }
    [JsonPropertyName("wire_bytes")] public ulong WireBytes { get; set; }
    [JsonPropertyName("saved_bytes")] public ulong SavedBytes { get; set; }
    [JsonPropertyName("saved_ratio")] public double SavedRatio { get; set; }
    [JsonPropertyName("batches")] public ulong Batches { get; set; }
    [JsonPropertyName("batching_delay_us")] public ulong BatchingDelayUs { get; set; }
    [JsonPropertyName("compression_time_us")] public ulong CompressionTimeUs { get; set; }
    [JsonPropertyName("decompression_time_us")] public ulong DecompressionTimeUs { get; set; }
    [JsonPropertyName("link_rate_bps")] public double LinkRateBps { get; set; }
    [JsonPropertyName("batching_delay")] public Quantiles? BatchingDelay { get; set; }
    [JsonPropertyName("compression_time")] public Quantiles? CompressionTime { get; set; }
    [JsonPropertyName("window")] public WindowSnapshot? Window { get; set; }
}

public class WindowSnapshot
{
    [JsonPropertyName("window_ms")] public ulong WindowMs { get; set; }
    [JsonPropertyName("raw_bytes")] public ulong RawBytes { get; set; }
    [JsonPropertyName("wire_bytes")] public ulong WireBytes { get; set; }
    [JsonPropertyName("saved_bytes")] public ulong SavedBytes { get; set; }
    [JsonPropertyName("saved_ratio")] public double SavedRatio { get; set; }
    [JsonPropertyName("batches")] public ulong Batches { get; set; }
}

public class OptimizerStatsSnapshot
{
    [JsonPropertyName("raw_bytes")] public ulong RawBytes { get; set; }
    [JsonPropertyName("wire_bytes")] public ulong WireBytes { get; set; }
    [JsonPropertyName("saved_bytes")] public ulong SavedBytes { get; set; }
    [JsonPropertyName("saved_ratio")] public double SavedRatio { get; set; }
    [JsonPropertyName("urgent_batches")] public ulong UrgentBatches { get; set; }
    [JsonPropertyName("timer_batches")] public ulong TimerBatches { get; set; }
    [JsonPropertyName("threshold_batches")] public ulong ThresholdBatches { get; set; }
    [JsonPropertyName("link_rate_bps")] public double LinkRateBps { get; set; }
    [JsonPropertyName("link_rate_measured")] public bool LinkRateMeasured { get; set; }
    [JsonPropertyName("link_rate_bytes")] public ulong LinkRateBytes { get; set; }
    [JsonPropertyName("link_rate_busy_us")] public ulong LinkRateBusyUs { get; set; }
    [JsonPropertyName("batching_delay_us")] public ulong BatchingDelayUs { get; set; }
    [JsonPropertyName("compression_time_us")] public ulong CompressionTimeUs { get; set; }
    [JsonPropertyName("decompression_time_us")] public ulong DecompressionTimeUs { get; set; }
    [JsonPropertyName("uplink")] public DirectionStatsSnapshot? Uplink { get; set; }
    [JsonPropertyName("downlink")] public DirectionStatsSnapshot? Downlink { get; set; }
    [JsonPropertyName("window")] public WindowSnapshot? Window { get; set; }
}

public class AdminOptimizerOverviewResponse
{
    [JsonPropertyName("global")] public OptimizerStatsSnapshot Global { get; set; } = new();
    [JsonPropertyName("services")] public Dictionary<string, OptimizerStatsSnapshot> Services { get; set; } = new();
}

public class AdminGitHubLoginUrlResponse
{
    [JsonPropertyName("url")] public string Url { get; set; } = "";
    [JsonPropertyName("state")] public string? State { get; set; }
}

public class AdminGitHubExchangeResponse
{
    [JsonPropertyName("token")] public string Token { get; set; } = "";
    [JsonPropertyName("token_id")] public string TokenId { get; set; } = "";
    [JsonPropertyName("user")] public AdminUserRecord? User { get; set; }
    [JsonPropertyName("expires_at_unix_ms")] public long? ExpiresAtUnixMs { get; set; }
}

public class AdminMiddlewareConfigField
{
    [JsonPropertyName("key")] public string Key { get; set; } = "";
    [JsonPropertyName("label")] public string Label { get; set; } = "";
    [JsonPropertyName("field_type")] public string FieldType { get; set; } = "string";
    [JsonPropertyName("default_value")] public object? DefaultValue { get; set; }
    [JsonPropertyName("description")] public string Description { get; set; } = "";
}

public class AdminMiddlewareConfigSchema
{
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("fields")] public List<AdminMiddlewareConfigField> Fields { get; set; } = new();
}

public class AdminMiddlewareItem
{
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("schema")] public AdminMiddlewareConfigSchema? Schema { get; set; }
    [JsonPropertyName("effective_config")] public Dictionary<string, object> EffectiveConfig { get; set; } = new();
}

public class AdminUpdateMiddlewareResponse
{
    [JsonPropertyName("status")] public string Status { get; set; } = "";
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("config")] public Dictionary<string, object> Config { get; set; } = new();
}

public class AdminResetMiddlewareResponse
{
    [JsonPropertyName("status")] public string Status { get; set; } = "";
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("reset")] public bool Reset { get; set; }
}

public class AdminApiClient
{
    private static readonly Lazy<AdminApiClient> _instance = new(() => new AdminApiClient());
    public static AdminApiClient Instance => _instance.Value;

    private readonly NativeClientService _client = NativeClientService.Instance;

    public async Task<T> InvokeControlRpcAsync<T>(string method, Dictionary<string, string>? parameters, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> typeInfo)
    {
        string? token = null;
        try
        {
            var cfg = _client.GetConfig();
            token = cfg?.ActiveConfig?.AuthToken;
        }
        catch { }

        var rpcReq = new AdminRpcRequest(
            Method: method,
            Params: parameters ?? new Dictionary<string, string>(),
            Token: string.IsNullOrWhiteSpace(token) ? null : token.Trim()
        );

        var resp = await _client.ControlRpcAsync(rpcReq);
        if (!resp.Ok)
        {
            throw new Exception(resp.Message ?? $"$control RPC '{method}' failed with status {resp.Status}");
        }

        var result = JsonSerializer.Deserialize(resp.Body, typeInfo);
        if (result == null)
        {
            throw new Exception($"Empty or invalid response from $control RPC '{method}'.");
        }
        return result;
    }

    public async Task<List<AdminConnectionRecord>> GetConnectionsAsync()
    {
        return await InvokeControlRpcAsync("connections", null, AdminJsonContext.Default.ListAdminConnectionRecord);
    }

    public async Task<List<AdminServiceSnapshot>> GetTunnelServicesAsync()
    {
        return await InvokeControlRpcAsync("tunnel_services", null, AdminJsonContext.Default.ListAdminServiceSnapshot);
    }

    public async Task<AdminOptimizerOverviewResponse> GetOptimizerStatsAsync()
    {
        return await InvokeControlRpcAsync("optimizer_stats", null, AdminJsonContext.Default.AdminOptimizerOverviewResponse);
    }

    public async Task<List<AdminUserRecord>> GetUsersAsync()
    {
        return await InvokeControlRpcAsync("auth.users", null, AdminJsonContext.Default.ListAdminUserRecord);
    }

    public async Task<AdminHealthResponse> GetHealthAsync()
    {
        return await InvokeControlRpcAsync("health", null, AdminJsonContext.Default.AdminHealthResponse);
    }

    public async Task<AdminConfigPathResponse> GetConfigPathAsync()
    {
        return await InvokeControlRpcAsync("config_path", null, AdminJsonContext.Default.AdminConfigPathResponse);
    }

    public async Task<AdminReloadResponse> TriggerReloadAsync()
    {
        return await InvokeControlRpcAsync("reload", null, AdminJsonContext.Default.AdminReloadResponse);
    }

    public AdminSessionResponse? CurrentSession { get; private set; }
    public event Action<AdminSessionResponse?>? SessionUpdated;

    public void ClearSession()
    {
        CurrentSession = null;
        SessionUpdated?.Invoke(null);
    }

    public async Task<AdminSessionResponse> GetSessionAsync()
    {
        try
        {
            var session = await InvokeControlRpcAsync("auth.session", null, AdminJsonContext.Default.AdminSessionResponse);
            CurrentSession = session;
            SessionUpdated?.Invoke(session);
            return session;
        }
        catch
        {
            CurrentSession = null;
            SessionUpdated?.Invoke(null);
            throw;
        }
    }

    public async Task<AdminAuthProvidersResponse> GetAuthProvidersAsync()
    {
        return await InvokeControlRpcAsync("auth.providers", null, AdminJsonContext.Default.AdminAuthProvidersResponse);
    }

    public async Task<List<AdminTokenRecord>> GetTokensAsync()
    {
        return await InvokeControlRpcAsync("auth.tokens.list", null, AdminJsonContext.Default.ListAdminTokenRecord);
    }

    public async Task<AdminCreateTokenResponse> CreateTokenAsync(string name, string userId, int? expiresInDays = null)
    {
        var parameters = new Dictionary<string, string>
        {
            ["name"] = name,
            ["user_id"] = userId
        };
        if (expiresInDays.HasValue && expiresInDays.Value > 0)
        {
            parameters["expires_in_days"] = expiresInDays.Value.ToString();
        }
        return await InvokeControlRpcAsync("auth.tokens.create", parameters, AdminJsonContext.Default.AdminCreateTokenResponse);
    }

    public async Task<AdminRevokeTokenResponse> RevokeTokenAsync(string tokenId)
    {
        var parameters = new Dictionary<string, string>
        {
            ["token_id"] = tokenId
        };
        return await InvokeControlRpcAsync("auth.tokens.revoke", parameters, AdminJsonContext.Default.AdminRevokeTokenResponse);
    }

    public async Task PutUserAsync(string userId, string role, List<string>? serviceRules = null)
    {
        var parameters = new Dictionary<string, string>
        {
            ["user_id"] = userId,
            ["role"] = role
        };
        if (serviceRules != null && serviceRules.Count > 0)
        {
            parameters["service_rules"] = JsonSerializer.Serialize(serviceRules, AdminJsonContext.Default.ListString);
        }
        await InvokeControlRpcAsync<object>("auth.user.put", parameters, AdminJsonContext.Default.Object);
    }

    public async Task<AdminGitHubLoginUrlResponse> GetGitHubLoginUrlAsync(string? state = null)
    {
        var parameters = !string.IsNullOrWhiteSpace(state) ? new Dictionary<string, string> { ["state"] = state } : null;
        return await InvokeControlRpcAsync("auth.github.login", parameters, AdminJsonContext.Default.AdminGitHubLoginUrlResponse);
    }

    public async Task<AdminGitHubExchangeResponse> ExchangeGitHubCodeAsync(string code, string? deviceId = null)
    {
        var parameters = new Dictionary<string, string> { ["code"] = code };
        if (!string.IsNullOrWhiteSpace(deviceId))
        {
            parameters["device_id"] = deviceId;
        }
        return await InvokeControlRpcAsync("auth.github.exchange", parameters, AdminJsonContext.Default.AdminGitHubExchangeResponse);
    }

    public async Task<List<AdminMiddlewareItem>> GetMiddlewaresAsync()
    {
        return await InvokeControlRpcAsync("middlewares.list", null, AdminJsonContext.Default.ListAdminMiddlewareItem);
    }

    public async Task<AdminMiddlewareConfigSchema> GetMiddlewareSchemaAsync(string name)
    {
        return await InvokeControlRpcAsync("middlewares.schema", new Dictionary<string, string> { ["name"] = name }, AdminJsonContext.Default.AdminMiddlewareConfigSchema);
    }

    public async Task<AdminUpdateMiddlewareResponse> UpdateMiddlewareConfigAsync(string name, Dictionary<string, string> config)
    {
        var parameters = new Dictionary<string, string>
        {
            ["name"] = name,
            ["config"] = JsonSerializer.Serialize(config, AdminJsonContext.Default.DictionaryStringString)
        };
        return await InvokeControlRpcAsync("middlewares.config.put", parameters, AdminJsonContext.Default.AdminUpdateMiddlewareResponse);
    }

    public async Task<AdminResetMiddlewareResponse> ResetMiddlewareConfigAsync(string name)
    {
        return await InvokeControlRpcAsync("middlewares.config.reset", new Dictionary<string, string> { ["name"] = name }, AdminJsonContext.Default.AdminResetMiddlewareResponse);
    }

    public async Task CloseConnectionAsync(string id)
    {
        string? token = null;
        try
        {
            var cfg = _client.GetConfig();
            token = cfg?.ActiveConfig?.AuthToken;
        }
        catch { }

        var rpcReq = new AdminRpcRequest(
            Method: "connections.close",
            Params: new Dictionary<string, string> { ["id"] = id },
            Token: string.IsNullOrWhiteSpace(token) ? null : token.Trim()
        );
        var resp = await _client.ControlRpcAsync(rpcReq);
        if (!resp.Ok)
        {
            throw new Exception(string.IsNullOrWhiteSpace(resp.Message)
                ? Prism.I18n.I18nText.T("close_connection_failed", "Failed to close connection.")
                : resp.Message);
        }
    }
}
