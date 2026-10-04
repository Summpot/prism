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
    private bool _loginRequired;

    [ObservableProperty]
    private bool _showGitHubLogin;

    [ObservableProperty]
    private bool _showAnonymousLogin;

    [ObservableProperty]
    private string _loginMethodsBadgeText = "";

    [ObservableProperty]
    private string? _providersError;

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
            AppServices.ShowSuccess(Messages.CommonCopied(), Messages.CommonCopy());
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
            AppServices.ShowSuccess(Messages.CommonCopied(), Messages.CommonCopy());
        }
    }

    [ObservableProperty]
    private bool _isRunning;

    partial void OnIsRunningChanged(bool value)
    {
        OnPropertyChanged(nameof(TunnelButtonText));
    }

    [ObservableProperty]
    private bool _isConnected;

    [ObservableProperty]
    private bool _isConnecting;

    partial void OnIsConnectingChanged(bool value)
    {
        OnPropertyChanged(nameof(TunnelButtonText));
    }

    public string TunnelButtonText => IsConnecting
        ? Messages.ClientConnecting()
        : Messages.ClientConnect();

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

    partial void OnShowOAuthExchangingChanged(bool value) => UpdateComputedProperties();

    [ObservableProperty]
    private bool _showOAuthWaiting;

    partial void OnShowOAuthWaitingChanged(bool value) => UpdateComputedProperties();

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

            bool hasToken = !string.IsNullOrWhiteSpace(cfg.ActiveConfig.AuthToken);
            SessionUsername = string.IsNullOrWhiteSpace(cfg.ActiveConfig.Username) ? "User" : cfg.ActiveConfig.Username;
            IsSessionAuthenticated = hasToken;

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
        ShowLoggedInCard = IsSessionAuthenticated;
        ShowConnectAndLoginHero = !IsSessionAuthenticated;

        LoginRequired = IsConnected && !IsSessionAuthenticated && DiscoveredServices.Count == 0;
        ShowGitHubLogin = IsGithubAuthAvailable || LoginRequired;
        ShowAnonymousLogin = !LoginRequired;
        LoginMethodsBadgeText = LoginRequired
            ? Messages.ClientWaitingForLogin()
            : Messages.ClientOptionalLogin();

        ShowLoginMethods = !ShowOAuthWaiting && !ShowOAuthExchanging && !IsSessionAuthenticated
            && (LoginRequired || (!BypassLogin && (IsGithubAuthAvailable || IsConnected || !string.IsNullOrWhiteSpace(ProvidersError))));

        string host = !string.IsNullOrWhiteSpace(ServerAddress) ? ServerAddress : RemoteLinkInput;
        string listen = !string.IsNullOrWhiteSpace(ListenAddress) ? ListenAddress : "127.0.0.1:25565";
        MappingText = $"{host} -> {listen}";

        string activeWord = Messages.ClientActive();
        DiscoveredServicesBadgeText = $"{DiscoveredServices.Count} {activeWord}";

        if (IsConnected)
        {
            DiscoveredServicesEmptyTitle = Messages.ClientWaitingServices();
            DiscoveredServicesEmptyHint = "";
        }
        else
        {
            DiscoveredServicesEmptyTitle = Messages.ClientNotConnectedService();
            DiscoveredServicesEmptyHint = Messages.ClientServiceHint();
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

            bool hasToken = !string.IsNullOrWhiteSpace(value.AuthToken);
            IsSessionAuthenticated = hasToken;
            UpdateComputedProperties();
        }
        catch (Exception ex)
        {
            ErrorMessage = Messages.AdminLoadFailed(ex.Message);
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
                ErrorMessage = Messages.ClientOauthUnsolicited();
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

            IsSessionAuthenticated = true;
            SessionUsername = string.IsNullOrWhiteSpace(result.Username) ? (string.IsNullOrWhiteSpace(SessionUsername) ? "User" : SessionUsername) : result.Username;
            SessionIsAdmin = SessionIsAdmin || string.Equals(result.Role, "admin", StringComparison.OrdinalIgnoreCase);
            ShowOAuthWaiting = false;
            ShowOAuthExchanging = false;
            ManualCallbackInput = "";
            UpdateComputedProperties();
            await ReconnectTunnelWithTokenAsync(result.Token);
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

            // Request login URL via in-band control RPC
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
                catch (Exception ex)
                {
                    ErrorMessage = string.IsNullOrWhiteSpace(ex.Message)
                        ? Messages.ClientProbeFailed()
                        : ex.Message;
                    return;
                }
            }

            if (string.IsNullOrWhiteSpace(targetUrl))
            {
                ErrorMessage = Messages.ClientProbeFailed();
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
            ErrorMessage = Messages.AdminLoadFailed(ex.Message);
        }
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
            ErrorMessage = Messages.ClientOauthStateMismatch();
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
            string? userId = null;
            ulong? expiresAt = null;

            // Exchange on the current control channel before restarting the tunnel.
            if (IsConnected)
            {
                try
                {
                    var exchangeResp = await AdminApiClient.Instance.ExchangeGitHubCodeAsync(code, deviceId);
                    token = exchangeResp.Token;
                    tokenId = exchangeResp.TokenId;
                    if (exchangeResp.ExpiresAtUnixMs is long expiresMs && expiresMs > 0)
                    {
                        expiresAt = (ulong)expiresMs;
                    }
                    if (exchangeResp.User != null)
                    {
                        username = !string.IsNullOrWhiteSpace(exchangeResp.User.DisplayName) ? exchangeResp.User.DisplayName : exchangeResp.User.Username;
                        avatarUrl = exchangeResp.User.AvatarUrl;
                        role = exchangeResp.User.Role;
                        userId = exchangeResp.User.Id;
                        isAdmin = string.Equals(role, "admin", StringComparison.OrdinalIgnoreCase);
                    }
                }
                catch { }
            }

            if (string.IsNullOrWhiteSpace(token))
            {
                ErrorMessage = Messages.ClientGithubExchangeFailed();
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
                    UserId: string.IsNullOrWhiteSpace(userId) ? null : userId,
                    Username: username,
                    ExpiresAt: expiresAt,
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
            SessionAvatarUrl = avatarUrl;
            SessionRole = role ?? "";
            SessionIsAdmin = isAdmin;
            ShowOAuthWaiting = false;
            ShowOAuthExchanging = false;
            ManualCallbackInput = "";
            ProvidersError = null;
            UpdateComputedProperties();

            if (!string.IsNullOrWhiteSpace(avatarUrl))
            {
                _ = LoadAvatarBitmapAsync(avatarUrl);
            }

            await ReconnectTunnelWithTokenAsync(token);
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

        var sessionDue = !_wasRunning || (DateTime.UtcNow - _lastSessionCheck).TotalSeconds > 30;
        var probeDue = !IsSessionAuthenticated
            && !string.IsNullOrWhiteSpace(ProvidersError)
            && (DateTime.UtcNow - _lastSessionCheck).TotalSeconds > 3;
        if (status.Running && (sessionDue || probeDue))
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
        if (string.IsNullOrWhiteSpace(RemoteLinkInput)) return;

        try
        {
            ErrorMessage = null;
            if (IsRunning)
            {
                await _client.StopAsync();
            }
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
                ActiveProfileId: null,
                ActiveConfig: patch
            ));

            if (!string.IsNullOrWhiteSpace(token))
            {
                IsSessionAuthenticated = true;
                UpdateComputedProperties();
            }

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

    private async Task ReconnectTunnelWithTokenAsync(string token)
    {
        if (string.IsNullOrWhiteSpace(token))
        {
            return;
        }

        try
        {
            await _client.StartAsync(authToken: token);
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
            return;
        }

        _lastSessionCheck = DateTime.MinValue;
        for (var attempt = 0; attempt < 8; attempt++)
        {
            await Task.Delay(500);
            try
            {
                var session = await AdminApiClient.Instance.GetSessionAsync();
                if (session.Authenticated)
                {
                    ApplyControlSession(session);
                    return;
                }
            }
            catch
            {
                // The new control channel is still opening.
            }
        }
    }

    private void ApplyControlSession(AdminSessionResponse session)
    {
        IsSessionAuthenticated = true;
        var name = session.DisplayName ?? session.Username;
        SessionUsername = string.IsNullOrWhiteSpace(name) ? (string.IsNullOrWhiteSpace(SessionUsername) ? "User" : SessionUsername) : name;
        SessionIsAdmin = session.IsAdmin;
        SessionRole = session.Role ?? "";
        if (!string.IsNullOrWhiteSpace(session.AvatarUrl) && session.AvatarUrl != SessionAvatarUrl)
        {
            SessionAvatarUrl = session.AvatarUrl;
            _ = LoadAvatarBitmapAsync(session.AvatarUrl);
        }
        ProvidersError = null;
        UpdateComputedProperties();
    }

    private async Task RefreshControlSessionAsync()
    {
        try
        {
            var session = await AdminApiClient.Instance.GetSessionAsync();
            if (session.Authenticated)
            {
                ApplyControlSession(session);
            }
        }
        catch
        {
            // Control RPC is only accessible once the tunnel is ready.
        }

        try
        {
            var providers = await AdminApiClient.Instance.GetAuthProvidersAsync();
            IsGithubAuthAvailable = providers.GithubEnabled || providers.Providers.Contains("github");
            ProvidersError = null;
        }
        catch (Exception ex)
        {
            ProvidersError = string.IsNullOrWhiteSpace(ex.Message)
                ? Messages.ClientProbeFailed()
                : ex.Message;
        }
        UpdateComputedProperties();
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
