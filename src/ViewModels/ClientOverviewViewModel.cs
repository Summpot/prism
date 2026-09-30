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

    [ObservableProperty]
    private bool _isSessionAuthenticated;

    [ObservableProperty]
    private string _sessionUsername = "";

    [ObservableProperty]
    private bool _sessionIsAdmin;

    [ObservableProperty]
    private string _sessionRole = "";

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

    public ObservableCollection<Prism.Native.ClientProfile> AvailableProfiles { get; } = new();

    [ObservableProperty]
    private Prism.Native.ClientProfile? _selectedProfile;

    private static readonly System.Net.Http.HttpClient _httpClient = new();
    private bool _isSwitchingProfile;

    public ObservableCollection<DiscoveredServiceItem> DiscoveredServices { get; } = new();

    public ClientOverviewViewModel()
    {
        _client.StatusUpdated += OnStatusUpdated;
        _client.ThroughputSampleAdded += OnThroughputSample;
        DesktopService.DeepLinkReceived += OnDeepLinkReceived;

        RefreshInitial();
    }

    private void RefreshInitial()
    {
        try
        {
            var cfg = _client.GetConfig();
            ProfileName = cfg.ActiveConfig.ProfileName ?? cfg.ActiveProfileId ?? "Default";
            ServerAddress = cfg.ActiveConfig.ServerAddr;
            ActiveTransport = (cfg.ActiveConfig.Transport ?? "AUTO").ToUpperInvariant();

            if (!string.IsNullOrWhiteSpace(cfg.ActiveConfig.Username) || !string.IsNullOrWhiteSpace(cfg.ActiveConfig.AuthToken))
            {
                IsSessionAuthenticated = true;
                SessionUsername = string.IsNullOrWhiteSpace(cfg.ActiveConfig.Username) ? "User" : cfg.ActiveConfig.Username;
            }

            _isSwitchingProfile = true;
            AvailableProfiles.Clear();
            var profiles = _client.GetProfiles();
            foreach (var p in profiles)
            {
                AvailableProfiles.Add(p);
            }
            SelectedProfile = AvailableProfiles.FirstOrDefault(p => p.Id == cfg.ActiveProfileId) ?? AvailableProfiles.FirstOrDefault();
            _isSwitchingProfile = false;
        }
        catch
        {
            _isSwitchingProfile = false;
        }
    }

    partial void OnSelectedProfileChanged(Prism.Native.ClientProfile? value)
    {
        if (_isSwitchingProfile || value == null) return;

        try
        {
            var patch = new ClientConfigPatch(
                ProfileName: value.Name,
                ServerAddr: value.ServerAddr,
                Transport: value.Transport,
                AuthToken: value.AuthToken,
                ListenAddr: value.ListenAddr,
                FakeLanBroadcast: value.FakeLanBroadcast,
                AutoConnectPanel: null,
                AutoConnect: null,
                ManagementUrl: null,
                TokenId: null,
                TokenType: null,
                UserId: null,
                Username: null,
                ExpiresAt: null,
                AutoCheckUpdate: null,
                UpdateChannel: null,
                Autostart: null,
                SilentAutostart: null,
                OptimizerEnabled: null,
                OptimizerZstdLevel: null,
                OptimizerAdaptiveFlush: null,
                OptimizerFlushIntervalMs: null,
                OptimizerBufferThreshold: null
            );

            _client.SaveConfig(new SaveConfigRequest(
                ActiveProfileId: value.Id,
                ActiveConfig: patch
            ));

            ProfileName = value.Name;
            ServerAddress = value.ServerAddr;
            ActiveTransport = (value.Transport ?? "AUTO").ToUpperInvariant();
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to switch profile: {ex.Message}";
        }
    }

    private void OnDeepLinkReceived(string link)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(async () =>
        {
            await HandleDeepLinkAsync(link);
        });
    }

    public async Task HandleDeepLinkAsync(string link)
    {
        if (string.IsNullOrWhiteSpace(link)) return;

        var result = PrismLinkService.ParseDeepLink(link);
        if (result.Kind == "auth" && !string.IsNullOrWhiteSpace(result.Token))
        {
            try
            {
                var patch = new ClientConfigPatch(
                    ProfileName: null,
                    ServerAddr: null,
                    Transport: null,
                    AuthToken: result.Token,
                    ListenAddr: null,
                    FakeLanBroadcast: null,
                    AutoConnectPanel: null,
                    AutoConnect: null,
                    ManagementUrl: null,
                    TokenId: null,
                    TokenType: null,
                    UserId: null,
                    Username: result.Username,
                    ExpiresAt: null,
                    AutoCheckUpdate: null,
                    UpdateChannel: null,
                    Autostart: null,
                    SilentAutostart: null,
                    OptimizerEnabled: null,
                    OptimizerZstdLevel: null,
                    OptimizerAdaptiveFlush: null,
                    OptimizerFlushIntervalMs: null,
                    OptimizerBufferThreshold: null
                );
                _client.SaveConfig(new SaveConfigRequest(
                    ActiveProfileId: null,
                    ActiveConfig: patch
                ));
            }
            catch { }

            IsSessionAuthenticated = true;
            SessionUsername = result.Username ?? "User";
            SessionIsAdmin = string.Equals(result.Role, "admin", StringComparison.OrdinalIgnoreCase);
            ShowOAuthWaiting = false;
            ShowOAuthExchanging = false;
            ManualCallbackInput = "";
        }
        else if (result.Kind == "auth-code" && !string.IsNullOrWhiteSpace(result.Code))
        {
            await ExchangeAuthCodeAsync(result.Code, result.State);
        }
        else if (result.Kind == "profile" && result.Profile != null)
        {
            RemoteLinkInput = result.Profile.ServerAddr;
            SelectedProtocol = "prism://";
            await ConnectFromLinkAsync();
        }
    }

    [RelayCommand]
    public void StartGitHubLogin()
    {
        try
        {
            ErrorMessage = null;
            string? baseUrl = ResolveManagementBaseUrl();
            if (string.IsNullOrWhiteSpace(baseUrl))
            {
                ErrorMessage = "GitHub OAuth requires a Management URL. Please configure it in Settings or import a profile with Management URL.";
                return;
            }

            string authUrl = $"{baseUrl}/auth/github/login";
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = authUrl,
                UseShellExecute = true
            });

            ShowOAuthWaiting = true;
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Failed to open browser: {ex.Message}";
        }
    }

    private string? ResolveManagementBaseUrl()
    {
        try
        {
            var cfg = _client.GetConfig();
            if (!string.IsNullOrWhiteSpace(cfg?.ActiveConfig?.ManagementUrl))
            {
                return cfg.ActiveConfig.ManagementUrl.Trim().TrimEnd('/');
            }
        }
        catch { }
        return null;
    }

    [RelayCommand]
    public async Task VerifyManualCallbackAsync()
    {
        if (string.IsNullOrWhiteSpace(ManualCallbackInput)) return;

        try
        {
            ErrorMessage = null;
            ShowOAuthExchanging = true;
            await HandleDeepLinkAsync(ManualCallbackInput.Trim());
        }
        catch (Exception ex)
        {
            ErrorMessage = $"Verification failed: {ex.Message}";
        }
        finally
        {
            ShowOAuthExchanging = false;
        }
    }

    [RelayCommand]
    public void CancelOAuth()
    {
        ShowOAuthWaiting = false;
        ShowOAuthExchanging = false;
        ManualCallbackInput = "";
    }

    private async Task ExchangeAuthCodeAsync(string code, string? state)
    {
        ShowOAuthExchanging = true;
        try
        {
            string? baseUrl = ResolveManagementBaseUrl();
            if (string.IsNullOrWhiteSpace(baseUrl))
            {
                ErrorMessage = "Management URL is not configured. Cannot exchange OAuth token.";
                return;
            }

            string exchangeUrl = $"{baseUrl}/auth/github/exchange";
            var payload = new OAuthExchangeRequest { Code = code, State = state };
            var json = System.Text.Json.JsonSerializer.Serialize(payload, AdminJsonContext.Default.OAuthExchangeRequest);
            var content = new System.Net.Http.StringContent(json, System.Text.Encoding.UTF8, "application/json");

            var resp = await _httpClient.PostAsync(exchangeUrl, content);
            if (resp.IsSuccessStatusCode)
            {
                var body = await resp.Content.ReadAsStringAsync();
                using var doc = System.Text.Json.JsonDocument.Parse(body);
                string token = doc.RootElement.GetProperty("token").GetString() ?? "";
                string username = "GitHub User";
                bool isAdmin = false;

                if (doc.RootElement.TryGetProperty("user", out var userElem))
                {
                    if (userElem.TryGetProperty("username", out var u) || userElem.TryGetProperty("login", out u))
                    {
                        username = u.GetString() ?? username;
                    }
                    if (userElem.TryGetProperty("role", out var r))
                    {
                        isAdmin = string.Equals(r.GetString(), "admin", StringComparison.OrdinalIgnoreCase);
                    }
                }

                try
                {
                    var patch = new ClientConfigPatch(
                        ProfileName: null,
                        ServerAddr: null,
                        Transport: null,
                        AuthToken: token,
                        ListenAddr: null,
                        FakeLanBroadcast: null,
                        AutoConnectPanel: null,
                        AutoConnect: null,
                        ManagementUrl: null,
                        TokenId: null,
                        TokenType: null,
                        UserId: null,
                        Username: username,
                        ExpiresAt: null,
                        AutoCheckUpdate: null,
                        UpdateChannel: null,
                        Autostart: null,
                        SilentAutostart: null,
                        OptimizerEnabled: null,
                        OptimizerZstdLevel: null,
                        OptimizerAdaptiveFlush: null,
                        OptimizerFlushIntervalMs: null,
                        OptimizerBufferThreshold: null
                    );
                    _client.SaveConfig(new SaveConfigRequest(
                        ActiveProfileId: null,
                        ActiveConfig: patch
                    ));
                }
                catch { }

                IsSessionAuthenticated = true;
                SessionUsername = username;
                SessionIsAdmin = isAdmin;
                ShowOAuthWaiting = false;
                ManualCallbackInput = "";
            }
            else
            {
                ErrorMessage = $"OAuth exchange failed with HTTP {resp.StatusCode}.";
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = $"OAuth exchange failed: {ex.Message}";
        }
        finally
        {
            ShowOAuthExchanging = false;
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

        UplinkStatsText = $"{Formatters.FormatBytes(status.Stats.Uplink.WireBytes)} ({Formatters.FormatPercentage(status.Stats.Uplink.SavedRatio)})";
        DownlinkStatsText = $"{Formatters.FormatBytes(status.Stats.Downlink.WireBytes)} ({Formatters.FormatPercentage(status.Stats.Downlink.SavedRatio)})";

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

        if (status.Running)
        {
            _ = RefreshControlSessionAsync();
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

    private async Task RefreshControlSessionAsync()
    {
        try
        {
            var session = await AdminApiClient.Instance.GetSessionAsync();
            if (session.Authenticated)
            {
                IsSessionAuthenticated = true;
                SessionUsername = session.Username ?? session.DisplayName ?? "User";
                SessionIsAdmin = session.IsAdmin;
                SessionRole = session.Role ?? "";
            }
        }
        catch
        {
            // Control RPC is only accessible once tunnel is ready
        }
    }

    [RelayCommand]
    public async Task SignOutAsync()
    {
        try
        {
            var patch = new ClientConfigPatch(
                ProfileName: null,
                ServerAddr: null,
                Transport: null,
                AuthToken: "",
                ListenAddr: null,
                FakeLanBroadcast: null,
                AutoConnectPanel: null,
                AutoConnect: null,
                ManagementUrl: null,
                TokenId: null,
                TokenType: null,
                UserId: null,
                Username: "",
                ExpiresAt: null,
                AutoCheckUpdate: null,
                UpdateChannel: null,
                Autostart: null,
                SilentAutostart: null,
                OptimizerEnabled: null,
                OptimizerZstdLevel: null,
                OptimizerAdaptiveFlush: null,
                OptimizerFlushIntervalMs: null,
                OptimizerBufferThreshold: null
            );
            _client.SaveConfig(new SaveConfigRequest(
                ActiveProfileId: null,
                ActiveConfig: patch
            ));
            IsSessionAuthenticated = false;
            SessionUsername = "";
            SessionIsAdmin = false;
            SessionRole = "";
            if (IsRunning)
            {
                await _client.StopAsync();
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
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
