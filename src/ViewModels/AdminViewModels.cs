using System;
using System.Collections.ObjectModel;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Services;

namespace Prism.ViewModels;

public partial class AdminOverviewViewModel : ViewModelBase
{
    private readonly PanelSessionService _session = PanelSessionService.Instance;

    [ObservableProperty]
    private int _connectionCount = 0;

    [ObservableProperty]
    private int _serviceCount = 0;

    [ObservableProperty]
    private string _configPath = "/etc/prism/prism.toml";

    [ObservableProperty]
    private string? _reloadMessage;

    [RelayCommand]
    public void ReloadServer()
    {
        ReloadMessage = "Server configuration reload signal sent.";
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
    public ObservableCollection<ConnectionItem> Connections { get; } = new();

    public AdminConnectionsViewModel()
    {
        Connections.Add(new ConnectionItem { Id = "conn-101", PeerAddr = "192.168.1.100:54321", Proto = "KCP", Duration = "12m 40s", RawBytes = "4.2 MB", WireBytes = "1.8 MB" });
        Connections.Add(new ConnectionItem { Id = "conn-102", PeerAddr = "10.0.0.15:49152", Proto = "QUIC", Duration = "3m 12s", RawBytes = "1.1 MB", WireBytes = "450 KB" });
    }

    [RelayCommand]
    public void Disconnect(ConnectionItem item)
    {
        Connections.Remove(item);
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

public partial class AdminTunnelServicesViewModel : ViewModelBase
{
    public ObservableCollection<ServiceRowItem> Services { get; } = new();

    public AdminTunnelServicesViewModel()
    {
        Services.Add(new ServiceRowItem { Name = "minecraft", Proto = "TCP", LocalAddr = "127.0.0.1:25565", RemoteAddr = ":25565", Masquerade = "mc.example.com" });
        Services.Add(new ServiceRowItem { Name = "web-panel", Proto = "HTTP", LocalAddr = "127.0.0.1:8080", RemoteAddr = ":8080", Masquerade = "panel.example.com" });
    }

    [RelayCommand]
    public void DeleteService(ServiceRowItem item)
    {
        Services.Remove(item);
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
    public ObservableCollection<UserRowItem> Users { get; } = new();

    public AdminUsersViewModel()
    {
        Users.Add(new UserRowItem { Username = "admin", Role = "admin", GithubId = "github:12345", CreatedAt = "2026-09-01" });
        Users.Add(new UserRowItem { Username = "guest_user", Role = "member", GithubId = "github:67890", CreatedAt = "2026-09-15" });
    }
}
