using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Prism.Services;

public class OAuthExchangeRequest
{
    [JsonPropertyName("code")] public string Code { get; set; } = "";
    [JsonPropertyName("state")] public string? State { get; set; }
}

[JsonSourceGenerationOptions(WriteIndented = false, PropertyNameCaseInsensitive = true)]
[JsonSerializable(typeof(OAuthExchangeRequest))]
[JsonSerializable(typeof(AdminHealthResponse))]
[JsonSerializable(typeof(AdminConfigPathResponse))]
[JsonSerializable(typeof(AdminReloadResponse))]
[JsonSerializable(typeof(AdminConnectionRecord))]
[JsonSerializable(typeof(List<AdminConnectionRecord>))]
[JsonSerializable(typeof(AdminServiceSnapshot))]
[JsonSerializable(typeof(List<AdminServiceSnapshot>))]
[JsonSerializable(typeof(AdminUserRecord))]
[JsonSerializable(typeof(List<AdminUserRecord>))]
public partial class AdminJsonContext : JsonSerializerContext
{
}
