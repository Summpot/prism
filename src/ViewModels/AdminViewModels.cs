using System;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Common;
using Prism.Services;

namespace Prism.ViewModels;

public partial class AdminOverviewViewModel : ViewModelBase
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

public partial class AdminConnectionsViewModel : ViewModelBase
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<ConnectionItem> Connections { get; } = new();

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    public AdminConnectionsViewModel()
    {
        _ = RefreshAsync();
    }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view active connections via in-band $control.";
            Connections.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var list = await _api.GetConnectionsAsync();
            Connections.Clear();

            var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            foreach (var c in list)
            {
                long elapsedSeconds = Math.Max(0, (now - c.StartedAtUnixMs) / 1000);
                Connections.Add(new ConnectionItem
                {
                    Id = c.Id,
                    PeerAddr = string.IsNullOrWhiteSpace(c.Client) ? c.Host : c.Client,
                    Proto = string.IsNullOrWhiteSpace(c.Upstream) ? "TCP" : c.Upstream,
                    Duration = Formatters.FormatUptime((ulong)elapsedSeconds),
                    RawBytes = Formatters.FormatBytes(c.RawBytes),
                    WireBytes = Formatters.FormatBytes(c.WireBytes)
                });
            }
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

public partial class AdminTunnelServicesViewModel : ViewModelBase
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

    [RelayCommand]
    public void DeleteService(ServiceRowItem? item)
    {
        if (item != null) Services.Remove(item);
    }
}

public class UserRowItem : ObservableObject
{
    public string Username { get; set; } = "";
    public string Role { get; set; } = "member";
    public string GithubId { get; set; } = "";
    public string CreatedAt { get; set; } = "";
}

public partial class AdminUsersViewModel : ViewModelBase
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<UserRowItem> Users { get; } = new();

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    public AdminUsersViewModel()
    {
        _ = RefreshAsync();
    }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ErrorMessage = "Tunnel is disconnected. Connect to a server in Overview to view users via in-band $control.";
            Users.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            var list = await _api.GetUsersAsync();
            Users.Clear();
            foreach (var u in list)
            {
                string created = DateTimeOffset.FromUnixTimeMilliseconds(u.CreatedAtUnixMs).ToString("yyyy-MM-dd HH:mm");
                Users.Add(new UserRowItem
                {
                    Username = string.IsNullOrWhiteSpace(u.DisplayName) ? u.Username : $"{u.DisplayName} ({u.Username})",
                    Role = u.Role,
                    GithubId = u.Id,
                    CreatedAt = created
                });
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to load users: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }
}
