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
[JsonSerializable(typeof(AdminSessionResponse))]
[JsonSerializable(typeof(AdminAuthProvidersResponse))]
[JsonSerializable(typeof(AdminTokenRecord))]
[JsonSerializable(typeof(List<AdminTokenRecord>))]
[JsonSerializable(typeof(AdminCreateTokenResponse))]
[JsonSerializable(typeof(AdminRevokeTokenResponse))]
[JsonSerializable(typeof(AdminOptimizerOverviewResponse))]
[JsonSerializable(typeof(OptimizerStatsSnapshot))]
[JsonSerializable(typeof(DirectionStatsSnapshot))]
[JsonSerializable(typeof(Quantiles))]
[JsonSerializable(typeof(AdminGitHubLoginUrlResponse))]
[JsonSerializable(typeof(AdminGitHubExchangeResponse))]
[JsonSerializable(typeof(AdminMiddlewareConfigField))]
[JsonSerializable(typeof(List<AdminMiddlewareConfigField>))]
[JsonSerializable(typeof(AdminMiddlewareConfigSchema))]
[JsonSerializable(typeof(AdminMiddlewareItem))]
[JsonSerializable(typeof(List<AdminMiddlewareItem>))]
[JsonSerializable(typeof(AdminUpdateMiddlewareResponse))]
[JsonSerializable(typeof(AdminResetMiddlewareResponse))]
[JsonSerializable(typeof(Dictionary<string, string>))]
[JsonSerializable(typeof(Dictionary<string, object>))]
[JsonSerializable(typeof(List<string>))]
[JsonSerializable(typeof(object))]
public partial class AdminJsonContext : JsonSerializerContext
{
}
