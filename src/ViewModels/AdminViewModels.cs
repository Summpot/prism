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
        }
        catch (Exception ex)
        {
            ReloadMessage = $"Reload failed: {ex.Message}";
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

            var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            foreach (var c in list)
            {
                long elapsedSeconds = Math.Max(0, (now - c.StartedAtUnixMs) / 1000);
                _allConnections.Add(new ConnectionItem
                {
                    Id = c.Id,
                    PeerAddr = string.IsNullOrWhiteSpace(c.Client) ? c.Host : c.Client,
                    Proto = string.IsNullOrWhiteSpace(c.Upstream) ? "TCP" : c.Upstream,
                    Duration = Formatters.FormatUptime((ulong)elapsedSeconds),
                    RawBytes = Formatters.FormatBytes(c.RawBytes),
                    WireBytes = Formatters.FormatBytes(c.WireBytes)
                });
            }
            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load connections: {ex.Message}";
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
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to close connection {item.Id}: {ex.Message}";
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
    public bool Primary { get; set; }
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

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view tunnel services via in-band $control.";
            Services.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var list = await _api.GetTunnelServicesAsync();
            Services.Clear();
            foreach (var s in list)
            {
                Services.Add(new ServiceRowItem
                {
                    Name = s.Service.Name,
                    Proto = s.Service.Proto,
                    LocalAddr = s.Service.LocalAddr,
                    RemoteAddr = s.Service.RemoteAddr,
                    Masquerade = s.Service.MasqueradeHost,
                    Primary = s.Primary
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
    public string Role { get; set; } = "member";
    public string GithubId { get; set; } = "";
    public string CreatedAt { get; set; } = "";
}

public class TokenRowItem : ObservableObject
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string TokenType { get; set; } = "client";
    public string CreatedAt { get; set; } = "";
    public string ExpiresAt { get; set; } = "Never";
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
    private string _newTokenName = "";

    [ObservableProperty]
    private string? _createdRawToken;

    [ObservableProperty]
    private bool _showTokenDialog;

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

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view users and tokens via in-band $control.";
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
                    Role = u.Role,
                    GithubId = u.Id,
                    CreatedAt = created
                });
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
                    TokenType = t.TokenType,
                    CreatedAt = created,
                    ExpiresAt = expires
                });
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load access control: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public void OpenCreateTokenDialog()
    {
        NewTokenName = "";
        CreatedRawToken = null;
        ShowTokenDialog = true;
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
            string userId = Users.FirstOrDefault()?.Id ?? "admin";
            var resp = await _api.CreateTokenAsync(NewTokenName.Trim(), userId, 30);
            CreatedRawToken = resp.RawToken;

            var t = resp.Token;
            string created = DateTimeOffset.FromUnixTimeMilliseconds(t.CreatedAtUnixMs).ToString("yyyy-MM-dd HH:mm");
            string expires = t.ExpiresAtUnixMs.HasValue
                ? DateTimeOffset.FromUnixTimeMilliseconds(t.ExpiresAtUnixMs.Value).ToString("yyyy-MM-dd HH:mm")
                : "30 days";

            Tokens.Insert(0, new TokenRowItem
            {
                Id = t.Id,
                Name = t.Name,
                TokenType = t.TokenType,
                CreatedAt = created,
                ExpiresAt = expires
            });
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to create token: {ex.Message}";
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
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to revoke token: {ex.Message}";
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
        }
    }
}
