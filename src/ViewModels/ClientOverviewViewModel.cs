using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Input.Platform;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Common;
using Prism.I18n;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public class DiscoveredServiceItem : ObservableObject
{
    public string Name { get; set; } = "";
    public string Proto { get; set; } = "";
    public string TargetAddress { get; set; } = "";
    public string MasqueradeHost { get; set; } = "";

    private bool _isCopied;
    public bool IsCopied
    {
        get => _isCopied;
        set => SetProperty(ref _isCopied, value);
    }
}

public partial class ClientOverviewViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;
    private readonly PanelSessionService _session = PanelSessionService.Instance;

    public PanelSessionService Session => _session;

    public List<string> SupportedProtocols { get; } = new()
    {
        "prism://",
        "prism-kcp://",
        "prism-quic://",
        "prism-ws://",
        "prism-h3://"
    };

    [ObservableProperty]
    private string _selectedProtocol = "prism://";

    [ObservableProperty]
    private string _remoteLinkInput = "";

    [ObservableProperty]
    private bool _isRunning;

    [ObservableProperty]
    private bool _isConnected;

    [ObservableProperty]
    private bool _isConnecting;

    [ObservableProperty]
    private string _profileName = "Default";

    [ObservableProperty]
    private string _activeTransport = "AUTO";

    [ObservableProperty]
    private string _serverAddress = "";

    [ObservableProperty]
    private string _statsViewMode = "session"; // "session" or "lifetime"

    [ObservableProperty]
    private string _uptimeText = "0s";

    [ObservableProperty]
    private string _rawBytesText = "0 B";

    [ObservableProperty]
    private string _wireBytesText = "0 B";

    [ObservableProperty]
    private string _savedRatioText = "0.0%";

    [ObservableProperty]
    private string _uplinkStatsText = "0 B (0%)";

    [ObservableProperty]
    private string _downlinkStatsText = "0 B (0%)";

    [ObservableProperty]
    private string _throughputText = "0 B/s";

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _showOAuthExchanging;

    [ObservableProperty]
    private bool _showOAuthWaiting;

    [ObservableProperty]
    private string _manualCallbackInput = "";

    public ObservableCollection<DiscoveredServiceItem> DiscoveredServices { get; } = new();

    public ClientOverviewViewModel()
    {
        _client.StatusUpdated += OnStatusUpdated;
        _client.ThroughputSampleAdded += OnThroughputSample;

        RefreshInitial();
    }

    private void RefreshInitial()
    {
        try
        {
            var cfg = _client.GetConfig();
            ProfileName = cfg.ActiveProfileId ?? "Default";
            ServerAddress = cfg.ActiveConfig.ServerAddr;
            ActiveTransport = (cfg.ActiveConfig.Transport ?? "AUTO").ToUpperInvariant();
        }
        catch
        {
        }
    }

    private void OnThroughputSample(ulong bps)
    {
        ThroughputText = $"{Formatters.FormatBytes(bps)}/s";
    }

    private void OnStatusUpdated(ClientStatusResponse status)
    {
        IsRunning = status.Running;
        IsConnected = status.Running && !string.IsNullOrWhiteSpace(status.ServerAddr);
        IsConnecting = status.Running && string.IsNullOrWhiteSpace(status.ServerAddr);

        if (!string.IsNullOrWhiteSpace(status.ServerAddr))
        {
            ServerAddress = status.ServerAddr;
        }

        ActiveTransport = (status.ActualTransport ?? status.Transport ?? "AUTO").ToUpperInvariant();

        if (StatsViewMode == "session")
        {
            UptimeText = Formatters.FormatUptime(_client.UptimeSeconds);
            RawBytesText = Formatters.FormatBytes(status.Stats.RawBytes);
            WireBytesText = Formatters.FormatBytes(status.Stats.WireBytes);
            SavedRatioText = Formatters.FormatPercentage(status.Stats.SavedRatio);
        }
        else
        {
            var cum = status.CumulativeStats;
            if (cum != null)
            {
                UptimeText = $"{cum.SessionsCount + 1}";
                RawBytesText = Formatters.FormatBytes(cum.RawBytes + status.Stats.RawBytes);
                WireBytesText = Formatters.FormatBytes(cum.WireBytes + status.Stats.WireBytes);
                SavedRatioText = Formatters.FormatPercentage(cum.SavedRatio);
            }
            else
            {
                UptimeText = Formatters.FormatUptime(_client.UptimeSeconds);
                RawBytesText = Formatters.FormatBytes(status.Stats.RawBytes);
                WireBytesText = Formatters.FormatBytes(status.Stats.WireBytes);
                SavedRatioText = Formatters.FormatPercentage(status.Stats.SavedRatio);
            }
        }

        UplinkStatsText = $"{Formatters.FormatBytes(status.Stats.WireBytes)} ({Formatters.FormatPercentage(status.Stats.SavedRatio)})";
        DownlinkStatsText = status.Stats.LinkRateMeasured ? Formatters.FormatBitRate(status.Stats.LinkRateBps) : "--";

        // Update discovered services
        DiscoveredServices.Clear();
        int idx = 0;
        foreach (var s in status.KnownServices)
        {
            string loopback = GetLoopbackTarget(idx, status.ListenAddr);
            DiscoveredServices.Add(new DiscoveredServiceItem
            {
                Name = s.Name,
                Proto = s.Proto,
                TargetAddress = loopback,
                MasqueradeHost = s.MasqueradeHost
            });
            idx++;
        }
    }

    private static string GetLoopbackTarget(int idx, string listenAddr)
    {
        string port = "25565";
        if (!string.IsNullOrWhiteSpace(listenAddr) && listenAddr.Contains(':'))
        {
            port = listenAddr[(listenAddr.LastIndexOf(':') + 1)..];
        }

        string ip = idx <= 254 ? $"127.0.0.{idx + 1}" : $"127.0.{idx / 256}.{idx % 256}";
        return port == "25565" ? ip : $"{ip}:{port}";
    }

    [RelayCommand]
    public async Task ToggleTunnelAsync()
    {
        try
        {
            ErrorMessage = null;
            if (IsRunning)
            {
                await _client.StopAsync();
            }
            else
            {
                await _client.StartAsync();
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
    }

    [RelayCommand]
    public async Task ConnectFromLinkAsync()
    {
        if (string.IsNullOrWhiteSpace(RemoteLinkInput)) return;

        try
        {
            ErrorMessage = null;
            string fullUrl = RemoteLinkInput.Contains("://") ? RemoteLinkInput : $"{SelectedProtocol}{RemoteLinkInput}";
            
            // Start with link address override
            await _client.StartAsync(serverAddr: fullUrl);
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
    }

    [RelayCommand]
    public void SetStatsMode(string mode)
    {
        StatsViewMode = mode;
        if (_client.CurrentStatus != null)
        {
            OnStatusUpdated(_client.CurrentStatus);
        }
    }

    [RelayCommand]
    public void ResetStats()
    {
        _client.ResetStats();
    }

    [RelayCommand]
    public void SignOut()
    {
        _session.SignOut();
    }

    [RelayCommand]
    public void DismissError()
    {
        ErrorMessage = null;
    }

    [RelayCommand]
    public async Task CopyTargetAsync(DiscoveredServiceItem item)
    {
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(item.TargetAddress);
            item.IsCopied = true;
            await Task.Delay(1500);
            item.IsCopied = false;
        }
    }
}
