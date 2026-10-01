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

    [ObservableProperty]
    private string? _sessionAvatarUrl;

    [ObservableProperty]
    private Avalonia.Media.Imaging.Bitmap? _userAvatarBitmap;

    private async Task LoadAvatarBitmapAsync(string url)
    {
        try
        {
            var bytes = await _httpClient.GetByteArrayAsync(url);
            using var ms = new System.IO.MemoryStream(bytes);
            UserAvatarBitmap = new Avalonia.Media.Imaging.Bitmap(ms);
        }
        catch
        {
            UserAvatarBitmap = null;
        }
    }

    public List<string> SupportedProtocols { get; } = new()
    {
        "auto://",
        "prism://",
        "wt://",
        "quic://",
        "tcp://",
        "kcp://",
        "ws://",
        "wss://"
    };

    [ObservableProperty]
    private string _selectedProtocol = "auto://";

    [ObservableProperty]
    private string _remoteLinkInput = "";

    [ObservableProperty]
    private string _listenAddress = "127.0.0.1:25565";

    [ObservableProperty]
    private string _mappingText = "";

    [ObservableProperty]
    private bool _showLoggedInCard;

    [ObservableProperty]
    private bool _showConnectAndLoginHero = true;

    [ObservableProperty]
    private bool _showLoginMethods = true;

    [ObservableProperty]
    private bool _bypassLogin;

    [RelayCommand]
    public void DismissLoginMethods()
    {
        BypassLogin = true;
        UpdateComputedProperties();
    }

    [ObservableProperty]
    private string _discoveredServicesBadgeText = "";

    [ObservableProperty]
    private string _discoveredServicesEmptyTitle = "";

    [ObservableProperty]
    private string _discoveredServicesEmptyHint = "";

    [ObservableProperty]
    private bool _hasCumulativeHistory;

    [ObservableProperty]
    private string _cumulativeHistoryText = "";

    partial void OnRemoteLinkInputChanged(string value)
    {
        if (string.IsNullOrWhiteSpace(value)) return;
        var idx = value.IndexOf("://", StringComparison.Ordinal);
        if (idx > 0)
        {
            var proto = value.Substring(0, idx + 3);
            if (SupportedProtocols.Contains(proto))
            {
                SelectedProtocol = proto;
                RemoteLinkInput = value.Substring(idx + 3);
            }
        }
    }

    [RelayCommand]
    public async Task PasteToRemoteLinkInputAsync()
    {
        if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            var text = await desktop.MainWindow.Clipboard.TryGetTextAsync();
            if (!string.IsNullOrWhiteSpace(text))
            {
                RemoteLinkInput = text.Trim();
            }
        }
    }

    [RelayCommand]
    public void ClearRemoteLinkInput()
    {
        RemoteLinkInput = "";
    }

    [RelayCommand]
    public async Task CopyServerAddressAsync()
    {
        if (string.IsNullOrWhiteSpace(ServerAddress)) return;
        if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(ServerAddress);
            AppServices.ShowSuccess(I18nText.T("common_copied"), I18nText.T("common_copy"));
        }
    }

    [RelayCommand]
    public async Task CopyConnectionAsync()
    {
        string fullUrl = RemoteLinkInput.Contains("://") ? RemoteLinkInput : $"{SelectedProtocol}{RemoteLinkInput}";
        if (string.IsNullOrWhiteSpace(fullUrl)) return;
        if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(fullUrl);
            AppServices.ShowSuccess(I18nText.T("common_copied"), I18nText.T("common_copy"));
        }
    }

    [ObservableProperty]
    private bool _isRunning;

    partial void OnIsRunningChanged(bool value)
    {
        OnPropertyChanged(nameof(TunnelButtonText));
    }

    public string TunnelButtonText => IsRunning
        ? (I18nText.T("client_disconnect_tunnel", "断开隧道"))
        : (I18nText.T("client_connect", "连接"));

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

    public ObservableCollection<ulong> ThroughputSamples { get; } = new();

    [ObservableProperty]
    private bool _isGithubAuthAvailable;

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
    private bool _wasRunning;
    private DateTime _lastSessionCheck = DateTime.MinValue;

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
            ServerAddress = cfg.ActiveConfig.ServerAddr ?? "";
            ListenAddress = cfg.ActiveConfig.ListenAddr ?? "127.0.0.1:25565";
            ActiveTransport = (cfg.ActiveConfig.Transport ?? "AUTO").ToUpperInvariant();

            if (!string.IsNullOrWhiteSpace(ServerAddress))
            {
                var idx = ServerAddress.IndexOf("://", StringComparison.Ordinal);
                if (idx > 0)
                {
                    SelectedProtocol = ServerAddress.Substring(0, idx + 3);
                    RemoteLinkInput = ServerAddress.Substring(idx + 3);
                }
                else
                {
                    SelectedProtocol = "auto://";
                    RemoteLinkInput = ServerAddress;
                }
            }
            else
            {
                SelectedProtocol = "auto://";
                RemoteLinkInput = "";
            }

            // In accordance with Tauri selectClientAuthView:
            // Having a username in config does NOT mean session is live authenticated!
            // Live authenticated requires active tunnel connected AND session verified.
            SessionUsername = string.IsNullOrWhiteSpace(cfg.ActiveConfig.Username) ? "User" : cfg.ActiveConfig.Username;
            IsSessionAuthenticated = false;

            _isSwitchingProfile = true;
            AvailableProfiles.Clear();
            var profiles = _client.GetProfiles();
            foreach (var p in profiles)
            {
                AvailableProfiles.Add(p);
            }
            SelectedProfile = AvailableProfiles.FirstOrDefault(p => p.Id == cfg.ActiveProfileId) ?? AvailableProfiles.FirstOrDefault();
            _isSwitchingProfile = false;

            UpdateComputedProperties();
        }
        catch
        {
            _isSwitchingProfile = false;
            UpdateComputedProperties();
        }
    }

    protected override void OnLocaleChanged()
    {
        base.OnLocaleChanged();
        UpdateComputedProperties();
        OnPropertyChanged(nameof(TunnelButtonText));
    }

    public void UpdateComputedProperties()
    {
        bool liveAuth = IsConnected && IsSessionAuthenticated;
        ShowLoggedInCard = liveAuth;
        ShowConnectAndLoginHero = !liveAuth;

        ShowLoginMethods = !BypassLogin && !ShowOAuthWaiting && !ShowOAuthExchanging && !IsSessionAuthenticated && (IsGithubAuthAvailable || IsConnected);

        string host = !string.IsNullOrWhiteSpace(ServerAddress) ? ServerAddress : RemoteLinkInput;
        string listen = !string.IsNullOrWhiteSpace(ListenAddress) ? ListenAddress : "127.0.0.1:25565";
        MappingText = $"{host} -> {listen}";

        string activeWord = I18nText.T("client_active", "活动");
        DiscoveredServicesBadgeText = $"{DiscoveredServices.Count} {activeWord}";

        if (IsConnected)
        {
            DiscoveredServicesEmptyTitle = I18nText.T("client_waiting_services", "等待 Connector 发布远端服务…");
            DiscoveredServicesEmptyHint = "";
        }
        else
        {
            DiscoveredServicesEmptyTitle = I18nText.T("client_not_connected_service", "未连接远端服务");
            DiscoveredServicesEmptyHint = I18nText.T("client_service_hint", "连接后将在此展示远端 Connector 发布的本地映射端口和服务信息");
        }
        OnPropertyChanged(nameof(TunnelButtonText));
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
            ListenAddress = value.ListenAddr;
            ActiveTransport = (value.Transport ?? "AUTO").ToUpperInvariant();
            if (!string.IsNullOrWhiteSpace(value.ServerAddr))
            {
                var idx = value.ServerAddr.IndexOf("://", StringComparison.Ordinal);
                if (idx > 0)
                {
                    SelectedProtocol = value.ServerAddr.Substring(0, idx + 3);
                    RemoteLinkInput = value.ServerAddr.Substring(idx + 3);
                }
                else
                {
                    RemoteLinkInput = value.ServerAddr;
                }
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
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
            if (IsSessionAuthenticated)
            {
                ErrorMessage = I18nText.T("client_oauth_unsolicited");
                return;
            }
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

            if (IsConnected && !string.IsNullOrWhiteSpace(result.Token))
            {
                try
                {
                    var authedSession = await AdminApiClient.Instance.AuthenticateAsync(result.Token);
                    if (authedSession.Authenticated)
                    {
                        SessionUsername = authedSession.DisplayName ?? authedSession.Username ?? result.Username ?? "User";
                        SessionIsAdmin = authedSession.IsAdmin;
                        SessionRole = authedSession.Role ?? "";
                    }
                }
                catch { }
            }

            IsSessionAuthenticated = true;
            SessionUsername = result.Username ?? SessionUsername ?? "User";
            SessionIsAdmin = SessionIsAdmin || string.Equals(result.Role, "admin", StringComparison.OrdinalIgnoreCase);
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
            SelectedProtocol = string.IsNullOrWhiteSpace(result.Profile.Transport) || result.Profile.Transport == "auto"
                ? "auto://"
                : result.Profile.Transport + "://";
            await ConnectFromLinkAsync();
        }
        else
        {
            var trimmed = link.Trim();
            if (trimmed.Length is >= 16 and <= 64 && trimmed.All(char.IsLetterOrDigit))
            {
                await ExchangeAuthCodeAsync(trimmed, DesktopService.PendingOAuthState);
            }
        }
    }

    [RelayCommand]
    public async Task StartGitHubLoginAsync()
    {
        try
        {
            ErrorMessage = null;
            string state = Guid.NewGuid().ToString("N");
            DesktopService.PendingOAuthState = state;
            string? targetUrl = null;

            // 1. If connected, prefer in-band control RPC
            if (IsConnected)
            {
                try
                {
                    var resp = await AdminApiClient.Instance.GetGitHubLoginUrlAsync(state);
                    if (!string.IsNullOrWhiteSpace(resp.Url))
                    {
                        targetUrl = resp.Url;
                    }
                }
                catch { }
            }

            // 2. Fallback to Management URL if available
            if (string.IsNullOrWhiteSpace(targetUrl))
            {
                string? baseUrl = ResolveManagementBaseUrl();
                if (!string.IsNullOrWhiteSpace(baseUrl))
                {
                    targetUrl = $"{baseUrl}/auth/github/login?state={state}";
                }
            }

            if (string.IsNullOrWhiteSpace(targetUrl))
            {
                ErrorMessage = I18nText.T("client_probe_failed");
                return;
            }

            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = targetUrl,
                UseShellExecute = true
            });

            ShowOAuthWaiting = true;
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
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
        if (!string.IsNullOrWhiteSpace(DesktopService.PendingOAuthState) &&
            !string.Equals(DesktopService.PendingOAuthState, state, StringComparison.Ordinal))
        {
            ErrorMessage = I18nText.T("client_oauth_state_mismatch");
            return;
        }
        ShowOAuthExchanging = true;
        try
        {
            var cfg = _client.GetConfig();
            string? deviceId = cfg?.DeviceId;

            string token = "";
            string username = "GitHub User";
            string? avatarUrl = null;
            bool isAdmin = false;
            string? role = null;
            string? tokenId = null;

            // 1. If connected, exchange via in-band control RPC
            if (IsConnected)
            {
                try
                {
                    var exchangeResp = await AdminApiClient.Instance.ExchangeGitHubCodeAsync(code, deviceId);
                    token = exchangeResp.Token;
                    tokenId = exchangeResp.TokenId;
                    if (exchangeResp.User != null)
                    {
                        username = !string.IsNullOrWhiteSpace(exchangeResp.User.DisplayName) ? exchangeResp.User.DisplayName : exchangeResp.User.Username;
                        avatarUrl = exchangeResp.User.AvatarUrl;
                        role = exchangeResp.User.Role;
                        isAdmin = string.Equals(role, "admin", StringComparison.OrdinalIgnoreCase);
                    }
                }
                catch { }
            }

            // 2. Fallback to HTTP if token is still empty and management url exists
            if (string.IsNullOrWhiteSpace(token))
            {
                string? baseUrl = ResolveManagementBaseUrl();
                if (!string.IsNullOrWhiteSpace(baseUrl))
                {
                    string exchangeUrl = $"{baseUrl}/auth/github/exchange";
                    var payload = new OAuthExchangeRequest { Code = code, State = state };
                    var json = System.Text.Json.JsonSerializer.Serialize(payload, AdminJsonContext.Default.OAuthExchangeRequest);
                    var content = new System.Net.Http.StringContent(json, System.Text.Encoding.UTF8, "application/json");

                    var resp = await _httpClient.PostAsync(exchangeUrl, content);
                    if (resp.IsSuccessStatusCode)
                    {
                        var body = await resp.Content.ReadAsStringAsync();
                        using var doc = System.Text.Json.JsonDocument.Parse(body);
                        token = doc.RootElement.GetProperty("token").GetString() ?? "";
                        if (doc.RootElement.TryGetProperty("token_id", out var tid)) tokenId = tid.GetString();
                        if (doc.RootElement.TryGetProperty("user", out var userElem))
                        {
                            if (userElem.TryGetProperty("username", out var u) || userElem.TryGetProperty("login", out u))
                            {
                                username = u.GetString() ?? username;
                            }
                            if (userElem.TryGetProperty("avatar_url", out var av))
                            {
                                avatarUrl = av.GetString();
                            }
                            if (userElem.TryGetProperty("role", out var r))
                            {
                                role = r.GetString();
                                isAdmin = string.Equals(role, "admin", StringComparison.OrdinalIgnoreCase);
                            }
                        }
                    }
                }
            }

            if (string.IsNullOrWhiteSpace(token))
            {
                ErrorMessage = I18nText.T("client_github_exchange_failed");
                return;
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
                    TokenId: tokenId ?? cfg?.ActiveConfig?.TokenId,
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

            if (IsConnected && !string.IsNullOrWhiteSpace(token))
            {
                try
                {
                    var authedSession = await AdminApiClient.Instance.AuthenticateAsync(token);
                    if (authedSession.Authenticated)
                    {
                        username = authedSession.DisplayName ?? authedSession.Username ?? username;
                        avatarUrl = authedSession.AvatarUrl ?? avatarUrl;
                        role = authedSession.Role ?? role;
                        isAdmin = authedSession.IsAdmin;
                    }
                }
                catch { }
            }

            IsSessionAuthenticated = true;
            SessionUsername = username;
            SessionAvatarUrl = avatarUrl;
            SessionRole = role ?? "";
            SessionIsAdmin = isAdmin;
            ShowOAuthWaiting = false;
            ShowOAuthExchanging = false;
            ManualCallbackInput = "";

            if (!string.IsNullOrWhiteSpace(avatarUrl))
            {
                _ = LoadAvatarBitmapAsync(avatarUrl);
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
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            ThroughputSamples.Add(bps);
            while (ThroughputSamples.Count > 30)
            {
                ThroughputSamples.RemoveAt(0);
            }
        });
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

        if (!string.IsNullOrWhiteSpace(status.ListenAddr))
        {
            ListenAddress = status.ListenAddr;
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

        // Update cumulative history for disconnected state
        var cumStat = status.CumulativeStats;
        if (!IsConnected && cumStat != null && cumStat.RawBytes > 0)
        {
            HasCumulativeHistory = true;
            CumulativeHistoryText = $"{Formatters.FormatBytes(cumStat.RawBytes)} 原始 • {Formatters.FormatBytes(cumStat.WireBytes)} 线路 • {Formatters.FormatPercentage(cumStat.SavedRatio)} 节省 ({cumStat.SessionsCount} 会话)";
        }
        else
        {
            HasCumulativeHistory = false;
        }

        // Update discovered services with diffing to prevent UI flickering
        SyncDiscoveredServices(status.KnownServices, status.ListenAddr);

        UpdateComputedProperties();

        if (status.Running && (!_wasRunning || (DateTime.UtcNow - _lastSessionCheck).TotalSeconds > 30))
        {
            _lastSessionCheck = DateTime.UtcNow;
            _ = RefreshControlSessionAsync();
        }
        _wasRunning = status.Running;
    }

    private void SyncDiscoveredServices(IReadOnlyList<ClientRegisteredService> knownServices, string listenAddr)
    {
        bool identical = knownServices.Count == DiscoveredServices.Count;
        if (identical)
        {
            for (int i = 0; i < knownServices.Count; i++)
            {
                var s = knownServices[i];
                var existing = DiscoveredServices[i];
                string loopback = GetLoopbackTarget(i, listenAddr);
                if (existing.Name != s.Name || existing.Proto != s.Proto ||
                    existing.TargetAddress != loopback || existing.MasqueradeHost != s.MasqueradeHost)
                {
                    identical = false;
                    break;
                }
            }
        }

        if (identical) return;

        DiscoveredServices.Clear();
        int idx = 0;
        foreach (var s in knownServices)
        {
            string loopback = GetLoopbackTarget(idx, listenAddr);
            DiscoveredServices.Add(new DiscoveredServiceItem
            {
                Name = s.Name,
                Proto = s.Proto,
                TargetAddress = loopback,
                MasqueradeHost = s.MasqueradeHost
            });
            idx++;
        }

        UpdateComputedProperties();
    }

    private static string GetLoopbackTarget(int idx, string listenAddr)
    {
        string port = "25565";
        if (!string.IsNullOrWhiteSpace(listenAddr) && listenAddr.Contains(':'))
        {
            port = listenAddr[(listenAddr.LastIndexOf(':') + 1)..];
        }

        string ip = "127.0.0.1";
        if (idx <= 254)
        {
            ip = $"127.0.0.{idx + 1}";
        }
        else
        {
            int offset = idx - 255;
            int b = 1 + offset / 65536;
            if (b <= 7)
            {
                int rem = offset % 65536;
                int c = rem / 256;
                int d = rem % 256;
                ip = $"127.{b}.{c}.{d}";
            }
        }
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
        if (IsRunning)
        {
            await ToggleTunnelAsync();
            return;
        }
        if (string.IsNullOrWhiteSpace(RemoteLinkInput)) return;

        try
        {
            ErrorMessage = null;
            string raw = RemoteLinkInput.Contains("://") ? RemoteLinkInput : $"{SelectedProtocol}{RemoteLinkInput}";
            
            var parsed = PrismLinkService.Parse(raw);
            string serverAddr = parsed?.ServerAddr ?? raw;
            string transport = parsed?.Transport ?? "auto";
            string listen = parsed?.ListenAddr ?? ListenAddress;
            string? token = !string.IsNullOrWhiteSpace(parsed?.AuthToken) ? parsed.AuthToken : null;
            string? name = parsed?.Name;

            var patch = new ClientConfigPatch(
                ProfileName: name,
                ServerAddr: serverAddr,
                Transport: transport,
                AuthToken: token,
                ListenAddr: listen,
                FakeLanBroadcast: parsed?.FakeLanBroadcast,
                AutoConnectPanel: null,
                AutoConnect: null,
                ManagementUrl: parsed?.ManagementUrl,
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
                ActiveProfileId: null,
                ActiveConfig: patch
            ));

            await _client.StartAsync(
                serverAddr: serverAddr,
                transport: transport,
                authToken: token,
                listenAddr: listen,
                profileName: name
            );

            _ = Task.Run(async () =>
            {
                await Task.Delay(1000);
                await RefreshControlSessionAsync();
            });
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
                if (!string.IsNullOrWhiteSpace(session.AvatarUrl) && session.AvatarUrl != SessionAvatarUrl)
                {
                    SessionAvatarUrl = session.AvatarUrl;
                    _ = LoadAvatarBitmapAsync(session.AvatarUrl);
                }
            }

            try
            {
                var providers = await AdminApiClient.Instance.GetAuthProvidersAsync();
                IsGithubAuthAvailable = providers.GithubEnabled || providers.Providers.Contains("github");
            }
            catch { }
            UpdateComputedProperties();
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
            AdminApiClient.Instance.ClearSession();
            UserAvatarBitmap = null;
            SessionAvatarUrl = null;
            IsSessionAuthenticated = false;
            SessionUsername = "";
            SessionIsAdmin = false;
            SessionRole = "";
            UpdateComputedProperties();
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
        if (item == null) return;
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(item.TargetAddress);
            item.IsCopied = true;
            await Task.Delay(2000);
            item.IsCopied = false;
        }
    }
}
