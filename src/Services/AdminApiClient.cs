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
    [JsonPropertyName("service_rules")] public List<string>? ServiceRules { get; set; }
    [JsonPropertyName("created_at_unix_ms")] public long CreatedAtUnixMs { get; set; }
    [JsonPropertyName("last_login_unix_ms")] public long LastLoginUnixMs { get; set; }
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

public class AdminApiClient
{
    private static readonly Lazy<AdminApiClient> _instance = new(() => new AdminApiClient());
    public static AdminApiClient Instance => _instance.Value;

    private readonly NativeClientService _client = NativeClientService.Instance;
    private readonly PanelSessionService _session = PanelSessionService.Instance;

    public async Task<T> RequestAsync<T>(string path, System.Text.Json.Serialization.Metadata.JsonTypeInfo<T> typeInfo, string method = "GET", string? bodyJson = null)
    {
        var headers = new Dictionary<string, string>
        {
            ["Accept"] = "application/json"
        };

        if (!string.IsNullOrWhiteSpace(_session.Token))
        {
            headers["Authorization"] = $"Bearer {_session.Token.Trim()}";
        }

        if (bodyJson != null)
        {
            headers["Content-Type"] = "application/json";
        }

        var req = new AdminHttpRequest(
            BaseUrl: _session.BaseUrl.TrimEnd('/'),
            Path: path,
            Method: method.ToUpperInvariant(),
            Headers: headers,
            Body: bodyJson
        );

        var resp = await _client.AdminRequestAsync(req);
        if (resp.Status < 200 || resp.Status >= 300)
        {
            throw new Exception($"Admin API error {resp.Status}: {resp.Body}");
        }

        var result = JsonSerializer.Deserialize(resp.Body, typeInfo);
        if (result == null)
        {
            throw new Exception("Empty or invalid response from server.");
        }
        return result;
    }

    public async Task<List<AdminConnectionRecord>> GetConnectionsAsync()
    {
        return await RequestAsync("/conns", AdminJsonContext.Default.ListAdminConnectionRecord);
    }

    public async Task<List<AdminServiceSnapshot>> GetTunnelServicesAsync()
    {
        return await RequestAsync("/tunnel/services", AdminJsonContext.Default.ListAdminServiceSnapshot);
    }

    public async Task<List<AdminUserRecord>> GetUsersAsync()
    {
        return await RequestAsync("/auth/users", AdminJsonContext.Default.ListAdminUserRecord);
    }

    public async Task<AdminHealthResponse> GetHealthAsync()
    {
        return await RequestAsync("/health", AdminJsonContext.Default.AdminHealthResponse);
    }

    public async Task<AdminConfigPathResponse> GetConfigPathAsync()
    {
        return await RequestAsync("/config", AdminJsonContext.Default.AdminConfigPathResponse);
    }

    public async Task<AdminReloadResponse> TriggerReloadAsync()
    {
        return await RequestAsync("/reload", AdminJsonContext.Default.AdminReloadResponse, "POST");
    }

    public async Task CloseConnectionAsync(string id)
    {
        try
        {
            var req = new AdminHttpRequest(
                BaseUrl: _session.BaseUrl.TrimEnd('/'),
                Path: $"/conns/{Uri.EscapeDataString(id)}",
                Method: "DELETE",
                Headers: new Dictionary<string, string> { ["Authorization"] = $"Bearer {_session.Token.Trim()}" },
                Body: null
            );
            await _client.AdminRequestAsync(req);
        }
        catch
        {
            var rpcReq = new AdminRpcRequest(
                Method: "connections.close",
                Params: new Dictionary<string, string> { ["id"] = id },
                Token: string.IsNullOrWhiteSpace(_session.Token) ? null : _session.Token
            );
            await _client.ControlRpcAsync(rpcReq);
        }
    }
}
