using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Input.Platform;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Common;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public partial class AdminOverviewViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty]
    private int _connectionCount = 0;

    [ObservableProperty]
    private int _serviceCount = 0;

    [ObservableProperty]
    private string _configPath = "/etc/prism/prism.toml";

    [ObservableProperty]
    private string? _reloadMessage;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _isHealthy;

    public AdminOverviewViewModel()
    {
        _ = LoadDataAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = LoadDataAsync();
        }
    }

    public void OnNavigatedFrom() { }

    [RelayCommand]
    public async Task LoadDataAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Prism tunnel is disconnected. Connect in the Overview tab to view server telemetry via in-band $control.";
            IsHealthy = false;
            ConnectionCount = 0;
            ServiceCount = 0;
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;

            var healthTask = _api.GetHealthAsync();
            var connsTask = _api.GetConnectionsAsync();
            var servicesTask = _api.GetTunnelServicesAsync();
            var configTask = _api.GetConfigPathAsync();

            await Task.WhenAll(healthTask, connsTask, servicesTask, configTask);

            IsHealthy = (await healthTask).Ok;
            ConnectionCount = (await connsTask).Count;
            ServiceCount = (await servicesTask).Count;
            ConfigPath = (await configTask).Path ?? "Managed via in-band $control";
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load admin overview: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public async Task ReloadServerAsync()
    {
        try
        {
            ReloadMessage = "Sending reload signal...";
            var res = await _api.TriggerReloadAsync();
            ReloadMessage = $"Server configuration reloaded (seq {res.Seq}).";
            AppServices.ShowSuccess($"Server configuration reloaded (seq {res.Seq}).", "Config Reloaded");
        }
        catch (Exception ex)
        {
            ReloadMessage = $"Reload failed: {ex.Message}";
            AppServices.ShowError(ex.Message, "Reload Failed");
        }
    }
}

public class ConnectionItem : ObservableObject
{
    public string Id { get; set; } = "";
    public string PeerAddr { get; set; } = "";
    public string Proto { get; set; } = "TCP";
    public string Duration { get; set; } = "0s";
    public string RawBytes { get; set; } = "0 B";
    public string WireBytes { get; set; } = "0 B";
}

public partial class AdminConnectionsViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    private readonly List<ConnectionItem> _allConnections = new();
    public ObservableCollection<ConnectionItem> Connections { get; } = new();

    [ObservableProperty]
    private string _searchQuery = "";

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    public AdminConnectionsViewModel()
    {
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
    }

    public void OnNavigatedFrom() { }

    partial void OnSearchQueryChanged(string value)
    {
        ApplyFilter();
    }

    private void ApplyFilter()
    {
        Connections.Clear();
        var q = SearchQuery.Trim();
        var matches = string.IsNullOrWhiteSpace(q)
            ? _allConnections
            : _allConnections.Where(c => c.Id.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                                         c.PeerAddr.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                                         c.Proto.Contains(q, StringComparison.OrdinalIgnoreCase));
        foreach (var c in matches)
        {
            Connections.Add(c);
        }
    }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view active connections via in-band $control.";
            _allConnections.Clear();
            Connections.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var list = await _api.GetConnectionsAsync();

            _allConnections.Clear();
            long nowMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

            foreach (var r in list)
            {
                long durMs = Math.Max(0, nowMs - r.StartedAtUnixMs);
                var duration = Formatters.FormatUptime((ulong)(durMs / 1000));
                var raw = Formatters.FormatBytes(r.RawBytes);
                var wire = Formatters.FormatBytes(r.WireBytes);

                _allConnections.Add(new ConnectionItem
                {
                    Id = r.Id,
                    PeerAddr = string.IsNullOrWhiteSpace(r.Client) ? r.Host : r.Client,
                    Proto = "TCP",
                    Duration = duration,
                    RawBytes = raw,
                    WireBytes = wire
                });
            }

            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to fetch connections: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public async Task DisconnectAsync(ConnectionItem? item)
    {
        if (item == null) return;
        try
        {
            await _api.CloseConnectionAsync(item.Id);
            _allConnections.Remove(item);
            Connections.Remove(item);
            AppServices.ShowSuccess($"Session {item.Id} closed.", "Disconnected");
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to close connection: {ex.Message}";
            AppServices.ShowError(ex.Message, "Disconnect Failed");
        }
    }
}

public class ServiceRowItem : ObservableObject
{
    public string Name { get; set; } = "";
    public string Proto { get; set; } = "TCP";
    public string LocalAddr { get; set; } = "";
    public string RemoteAddr { get; set; } = "";
    public string Masquerade { get; set; } = "";
}

public partial class AdminTunnelServicesViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<ServiceRowItem> Services { get; } = new();

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    public AdminTunnelServicesViewModel()
    {
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
    }

    public void OnNavigatedFrom() { }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect in the Overview tab to view registered tunnel services.";
            Services.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var list = await _api.GetTunnelServicesAsync();

            Services.Clear();
            foreach (var snap in list)
            {
                var s = snap.Service;
                Services.Add(new ServiceRowItem
                {
                    Name = s.Name,
                    Proto = s.Proto.ToUpperInvariant(),
                    LocalAddr = s.LocalAddr,
                    RemoteAddr = s.RemoteAddr,
                    Masquerade = string.IsNullOrWhiteSpace(s.MasqueradeHost) ? "-" : s.MasqueradeHost
                });
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load services: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }
}

public class UserRowItem : ObservableObject
{
    public string Id { get; set; } = "";
    public string Username { get; set; } = "";

    private string _role = "member";
    public string Role
    {
        get => _role;
        set => SetProperty(ref _role, value);
    }

    public string GithubId { get; set; } = "";
    public string CreatedAt { get; set; } = "";
    public List<string> ServiceRules { get; set; } = new();
}

public class TokenRowItem : ObservableObject
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string TokenType { get; set; } = "client";
    public string CreatedAt { get; set; } = "";
    public string ExpiresAt { get; set; } = "";
}

public partial class AdminUsersViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<UserRowItem> Users { get; } = new();
    public ObservableCollection<TokenRowItem> Tokens { get; } = new();

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _showTokenDialog;

    [ObservableProperty]
    private string _newTokenName = "";

    [ObservableProperty]
    private string _selectedUserId = "";

    [ObservableProperty]
    private int _selectedExpirationDays = 30;

    public static readonly int[] ExpirationOptions = [1, 7, 30, 90, 365];

    [ObservableProperty]
    private string? _createdRawToken;

    public AdminUsersViewModel()
    {
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
    }

    public void OnNavigatedFrom() { }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect in the Overview tab to view and manage users and tokens.";
            Users.Clear();
            Tokens.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;

            var usersTask = _api.GetUsersAsync();
            var tokensTask = _api.GetTokensAsync();

            await Task.WhenAll(usersTask, tokensTask);

            Users.Clear();
            foreach (var u in await usersTask)
            {
                string created = DateTimeOffset.FromUnixTimeMilliseconds(u.CreatedAtUnixMs).ToString("yyyy-MM-dd HH:mm");
                Users.Add(new UserRowItem
                {
                    Id = u.Id,
                    Username = string.IsNullOrWhiteSpace(u.DisplayName) ? u.Username : $"{u.DisplayName} ({u.Username})",
                    Role = u.Role.ToUpperInvariant(),
                    GithubId = u.Username,
                    CreatedAt = created,
                    ServiceRules = u.ServiceRules ?? new List<string>()
                });
            }

            if (string.IsNullOrWhiteSpace(SelectedUserId) && Users.Count > 0)
            {
                SelectedUserId = Users[0].Id;
            }

            Tokens.Clear();
            foreach (var t in await tokensTask)
            {
                string created = DateTimeOffset.FromUnixTimeMilliseconds(t.CreatedAtUnixMs).ToString("yyyy-MM-dd HH:mm");
                string expires = t.ExpiresAtUnixMs.HasValue
                    ? DateTimeOffset.FromUnixTimeMilliseconds(t.ExpiresAtUnixMs.Value).ToString("yyyy-MM-dd HH:mm")
                    : "Never";

                Tokens.Add(new TokenRowItem
                {
                    Id = t.Id,
                    Name = t.Name,
                    TokenType = t.TokenType.ToUpperInvariant(),
                    CreatedAt = created,
                    ExpiresAt = expires
                });
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to fetch users or tokens: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public void OpenCreateTokenDialog()
    {
        ShowTokenDialog = true;
        NewTokenName = "";
        CreatedRawToken = null;
        if (Users.Count > 0 && string.IsNullOrWhiteSpace(SelectedUserId))
        {
            SelectedUserId = Users[0].Id;
        }
    }

    [RelayCommand]
    public void CloseCreateTokenDialog()
    {
        ShowTokenDialog = false;
        CreatedRawToken = null;
    }

    [RelayCommand]
    public async Task SubmitCreateTokenAsync()
    {
        if (string.IsNullOrWhiteSpace(NewTokenName)) return;

        try
        {
            ErrorMessage = null;
            string uid = string.IsNullOrWhiteSpace(SelectedUserId) ? "admin-user" : SelectedUserId;
            var resp = await _api.CreateTokenAsync(NewTokenName.Trim(), uid, SelectedExpirationDays > 0 ? SelectedExpirationDays : null);
            CreatedRawToken = resp.RawToken;

            var t = resp.Token;
            string created = DateTimeOffset.FromUnixTimeMilliseconds(t.CreatedAtUnixMs).ToString("yyyy-MM-dd HH:mm");
            string expires = t.ExpiresAtUnixMs.HasValue
                ? DateTimeOffset.FromUnixTimeMilliseconds(t.ExpiresAtUnixMs.Value).ToString("yyyy-MM-dd HH:mm")
                : "Never";

            Tokens.Insert(0, new TokenRowItem
            {
                Id = t.Id,
                Name = t.Name,
                TokenType = t.TokenType.ToUpperInvariant(),
                CreatedAt = created,
                ExpiresAt = expires
            });

            AppServices.ShowSuccess($"Token '{t.Name}' created successfully.", "Token Created");
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to create token: {ex.Message}";
            AppServices.ShowError(ex.Message, "Token Creation Failed");
        }
    }

    [RelayCommand]
    public async Task ToggleUserRoleAsync(UserRowItem? item)
    {
        if (item == null) return;
        string newRole = item.Role == "ADMIN" ? "member" : "admin";
        try
        {
            await _api.PutUserAsync(item.Id, newRole, item.ServiceRules);
            item.Role = newRole.ToUpperInvariant();
            AppServices.ShowSuccess($"Role for {item.Username} changed to {item.Role}.", "Role Updated");
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to update role: {ex.Message}";
            AppServices.ShowError(ex.Message, "Role Update Failed");
        }
    }

    [RelayCommand]
    public async Task RevokeTokenAsync(TokenRowItem? item)
    {
        if (item == null) return;

        try
        {
            ErrorMessage = null;
            await _api.RevokeTokenAsync(item.Id);
            Tokens.Remove(item);
            AppServices.ShowSuccess($"Token '{item.Name}' revoked.", "Token Revoked");
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to revoke token: {ex.Message}";
            AppServices.ShowError(ex.Message, "Revoke Failed");
        }
    }

    [RelayCommand]
    public async Task CopyRawTokenAsync()
    {
        if (!string.IsNullOrWhiteSpace(CreatedRawToken) &&
            Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(CreatedRawToken);
            AppServices.ShowSuccess("Token copied to clipboard.", "Copied");
        }
    }
}

public partial class AdminTrafficViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty] private string _rawBytesText = "0 B";
    [ObservableProperty] private string _wireBytesText = "0 B";
    [ObservableProperty] private string _savedBytesText = "0 B";
    [ObservableProperty] private string _savedRatioText = "0.0%";
    [ObservableProperty] private int _liveSessionsCount = 0;
    [ObservableProperty] private string _liveThroughputText = "0 B -> 0 B";

    [ObservableProperty] private string _uplinkRawWire = "0 B / 0 B (0%)";
    [ObservableProperty] private string _uplinkP50 = "0 ms";
    [ObservableProperty] private string _uplinkP90 = "0 ms";
    [ObservableProperty] private string _uplinkP99 = "0 ms";
    [ObservableProperty] private string _uplinkMax = "0 ms";

    [ObservableProperty] private string _downlinkRawWire = "0 B / 0 B (0%)";
    [ObservableProperty] private string _downlinkP50 = "0 ms";
    [ObservableProperty] private string _downlinkP90 = "0 ms";
    [ObservableProperty] private string _downlinkP99 = "0 ms";
    [ObservableProperty] private string _downlinkMax = "0 ms";

    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;

    public AdminTrafficViewModel()
    {
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
    }

    public void OnNavigatedFrom() { }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect in the Overview tab to view server telemetry.";
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;

            var statsTask = _api.GetOptimizerStatsAsync();
            var connsTask = _api.GetConnectionsAsync();

            await Task.WhenAll(statsTask, connsTask);

            var stats = (await statsTask).Global;
            var conns = await connsTask;

            RawBytesText = Formatters.FormatBytes(stats.RawBytes);
            WireBytesText = Formatters.FormatBytes(stats.WireBytes);
            SavedBytesText = Formatters.FormatBytes(stats.SavedBytes);
            SavedRatioText = Formatters.FormatPercentage(stats.SavedRatio);
            LiveSessionsCount = conns.Count;

            ulong connsRaw = 0;
            ulong connsWire = 0;
            foreach (var c in conns)
            {
                connsRaw += c.RawBytes;
                connsWire += c.WireBytes;
            }
            LiveThroughputText = $"{Formatters.FormatBytes(connsRaw)} -> {Formatters.FormatBytes(connsWire)}";

            if (stats.Uplink != null)
            {
                var up = stats.Uplink;
                UplinkRawWire = $"{Formatters.FormatBytes(up.RawBytes)} / {Formatters.FormatBytes(up.WireBytes)} ({Formatters.FormatPercentage(up.SavedRatio)})";
                UplinkP50 = $"{up.CompressionTime?.P50Us / 1000.0:0.##} ms";
                UplinkP90 = $"{up.CompressionTime?.P90Us / 1000.0:0.##} ms";
                UplinkP99 = $"{up.CompressionTime?.P99Us / 1000.0:0.##} ms";
                UplinkMax = $"{up.CompressionTime?.MaxUs / 1000.0:0.##} ms";
            }

            if (stats.Downlink != null)
            {
                var down = stats.Downlink;
                DownlinkRawWire = $"{Formatters.FormatBytes(down.RawBytes)} / {Formatters.FormatBytes(down.WireBytes)} ({Formatters.FormatPercentage(down.SavedRatio)})";
                DownlinkP50 = $"{down.CompressionTime?.P50Us / 1000.0:0.##} ms";
                DownlinkP90 = $"{down.CompressionTime?.P90Us / 1000.0:0.##} ms";
                DownlinkP99 = $"{down.CompressionTime?.P99Us / 1000.0:0.##} ms";
                DownlinkMax = $"{down.CompressionTime?.MaxUs / 1000.0:0.##} ms";
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load traffic stats: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }
}

public class ConnectorGroupItem : ObservableObject
{
    public string ClientId { get; set; } = "";
    public string RemoteAddr { get; set; } = "";
    public bool Primary { get; set; }
    public List<AdminRegisteredService> Services { get; set; } = new();
    public int ServiceCount => Services.Count;
}

public partial class AdminConnectorsViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    private readonly List<ConnectorGroupItem> _allConnectors = new();
    public ObservableCollection<ConnectorGroupItem> Connectors { get; } = new();

    [ObservableProperty] private string _searchQuery = "";
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;

    public AdminConnectorsViewModel()
    {
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
    }

    public void OnNavigatedFrom() { }

    partial void OnSearchQueryChanged(string value) => ApplyFilter();

    private void ApplyFilter()
    {
        Connectors.Clear();
        var q = SearchQuery.Trim().ToLowerInvariant();
        var matches = string.IsNullOrWhiteSpace(q)
            ? _allConnectors
            : _allConnectors.Where(c => c.ClientId.ToLowerInvariant().Contains(q) ||
                                       c.RemoteAddr.ToLowerInvariant().Contains(q) ||
                                       c.Services.Any(s => s.Name.ToLowerInvariant().Contains(q)));
        foreach (var c in matches)
        {
            Connectors.Add(c);
        }
    }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view connectors.";
            Connectors.Clear();
            _allConnectors.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var snapshots = await _api.GetTunnelServicesAsync();

            var groups = new Dictionary<string, ConnectorGroupItem>();
            foreach (var s in snapshots)
            {
                string key = string.IsNullOrWhiteSpace(s.ClientId) ? "default-connector" : s.ClientId;
                if (!groups.TryGetValue(key, out var grp))
                {
                    grp = new ConnectorGroupItem
                    {
                        ClientId = key,
                        RemoteAddr = s.Remote,
                        Primary = s.Primary,
                        Services = new List<AdminRegisteredService>()
                    };
                    groups[key] = grp;
                }
                grp.Services.Add(s.Service);
            }

            _allConnectors.Clear();
            _allConnectors.AddRange(groups.Values);
            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load connectors: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }
}

public partial class AdminMiddlewareViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<Prism.Native.MiddlewareItem> Middlewares { get; } = new();
    public ObservableCollection<MiddlewareFieldViewModel> ConfigFields { get; } = new();

    [ObservableProperty] private Prism.Native.MiddlewareItem? _selectedMiddleware;
    [ObservableProperty] private bool _isMiddlewareEnabled = true;
    [ObservableProperty] private string? _statusMessage;

    public AdminMiddlewareViewModel()
    {
        LoadData();
    }

    public void OnNavigatedTo() => LoadData(SelectedMiddleware?.Name);
    public void OnNavigatedFrom() { }

    public void LoadData(string? preserveName = null)
    {
        Middlewares.Clear();
        try
        {
            var list = _client.ListMiddlewares();
            foreach (var item in list)
            {
                Middlewares.Add(item);
            }
            SelectedMiddleware = (!string.IsNullOrWhiteSpace(preserveName)
                ? Middlewares.FirstOrDefault(m => string.Equals(m.Name, preserveName, StringComparison.OrdinalIgnoreCase))
                : null) ?? Middlewares.FirstOrDefault();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    partial void OnSelectedMiddlewareChanged(Prism.Native.MiddlewareItem? value)
    {
        ConfigFields.Clear();
        if (value == null) return;

        if (value.EffectiveConfig.TryGetValue("enabled", out var en))
        {
            IsMiddlewareEnabled = !string.Equals(en, "false", StringComparison.OrdinalIgnoreCase);
        }
        else
        {
            IsMiddlewareEnabled = true;
        }

        if (value.Schema == null) return;

        foreach (var f in value.Schema.Fields)
        {
            if (f.Key.Equals("enabled", StringComparison.OrdinalIgnoreCase)) continue;

            string currentVal = value.EffectiveConfig.TryGetValue(f.Key, out var v) ? v : f.DefaultValue;

            ConfigFields.Add(MiddlewareFieldViewModel.Create(
                key: f.Key,
                label: string.IsNullOrWhiteSpace(f.Label) ? f.Key : f.Label,
                description: f.Description,
                fieldType: f.FieldType,
                initialValue: currentVal
            ));
        }
    }

    [RelayCommand]
    public void SaveConfig()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            var dict = ConfigFields.ToDictionary(f => f.Key, f => f.GetStringValue());
            dict["enabled"] = IsMiddlewareEnabled ? "true" : "false";

            string currentName = SelectedMiddleware.Name;
            var updated = _client.UpdateMiddlewareConfig(currentName, dict);
            StatusMessage = $"Config for {updated.Name} saved successfully.";
            AppServices.ShowSuccess($"Middleware {updated.Name} updated.", "Saved");
            LoadData(currentName);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
            AppServices.ShowError(ex.Message, "Save Failed");
        }
    }

    [RelayCommand]
    public void ResetConfig()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            string currentName = SelectedMiddleware.Name;
            var reset = _client.ResetMiddlewareConfig(currentName);
            StatusMessage = $"Config for {reset.Name} reset to default.";
            AppServices.ShowSuccess($"Middleware {reset.Name} reset to defaults.", "Reset Defaults");
            LoadData(currentName);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }
}

public partial class AdminRuntimeViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty] private bool _isHealthy;
    [ObservableProperty] private string _configPath = "/etc/prism/prism.toml";
    [ObservableProperty] private string? _reloadResult;
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;

    public AdminRuntimeViewModel()
    {
        _ = LoadDataAsync();
    }

    public void OnNavigatedTo()
    {
        if (_client.CurrentStatus?.Running == true)
        {
            _ = LoadDataAsync();
        }
    }

    public void OnNavigatedFrom() { }

    [RelayCommand]
    public async Task LoadDataAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect in the Overview tab to view server runtime.";
            IsHealthy = false;
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;

            var healthTask = _api.GetHealthAsync();
            var pathTask = _api.GetConfigPathAsync();

            await Task.WhenAll(healthTask, pathTask);

            IsHealthy = (await healthTask).Ok;
            ConfigPath = (await pathTask).Path ?? "Managed via in-band $control";
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load runtime telemetry: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public async Task TriggerReloadAsync()
    {
        try
        {
            var res = await _api.TriggerReloadAsync();
            ReloadResult = $"Reload command executed successfully (seq {res.Seq}).";
            AppServices.ShowSuccess(ReloadResult, "Server Reloaded");
            await LoadDataAsync();
        }
        catch (Exception ex)
        {
            ReloadResult = $"Reload failed: {ex.Message}";
            AppServices.ShowError(ex.Message, "Reload Failed");
        }
    }
}
