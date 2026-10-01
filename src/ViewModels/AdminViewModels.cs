using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Input.Platform;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Common;
using Prism.I18n;
using Prism.Native;
using Prism.Services;
using Prism.Views;
using ShadUI;

namespace Prism.ViewModels;

internal static class AutoRefreshHelper
{
    public static CancellationTokenSource? Start(Func<Task> action, TimeSpan interval, Func<bool> canRun)
    {
        var cts = new CancellationTokenSource();
        var token = cts.Token;
        _ = Task.Run(async () =>
        {
            using var timer = new PeriodicTimer(interval);
            try
            {
                while (await timer.WaitForNextTickAsync(token))
                {
                    if (canRun())
                    {
                        await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(async () =>
                        {
                            try
                            {
                                await action();
                            }
                            catch { }
                        });
                    }
                }
            }
            catch (OperationCanceledException) { }
        }, token);
        return cts;
    }

    public static void Stop(ref CancellationTokenSource? cts)
    {
        cts?.Cancel();
        cts?.Dispose();
        cts = null;
    }
}

public partial class AdminOverviewViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty]
    private int _connectionCount = 0;

    [ObservableProperty]
    private int _serviceCount = 0;

    [ObservableProperty]
    private string _configPath = "";

    [ObservableProperty]
    private string _endpoint = "";

    [ObservableProperty]
    private string _savedRatioText = "0.0%";

    public ObservableCollection<ServiceRowItem> PreviewServices { get; } = new();

    [ObservableProperty]
    private string? _reloadMessage;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private AdminReadyState _readyState = AdminReadyState.Loading;

    [ObservableProperty]
    private bool _isReady;

    [ObservableProperty]
    private bool _isHealthy;

    [ObservableProperty]
    private bool _autoRefresh = true;

    [ObservableProperty]
    private string _autoRefreshText = "";

    private CancellationTokenSource? _autoRefreshCts;

    public AdminOverviewViewModel()
    {
        UpdateAutoRefreshText();
        _ = LoadDataAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = LoadDataAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    [RelayCommand]
    public void OpenConnections() => NavigationService.Instance.NavigateTo("admin.connections");

    [RelayCommand]
    public void OpenServices() => NavigationService.Instance.NavigateTo("admin.services");

    [RelayCommand]
    public void OpenTraffic() => NavigationService.Instance.NavigateTo("admin.traffic");

    [RelayCommand]
    public void OpenConnectors() => NavigationService.Instance.NavigateTo("admin.connectors");

    [RelayCommand]
    public void DismissError() => ErrorMessage = null;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(LoadDataAsync, TimeSpan.FromSeconds(8), () => IsReady && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    [RelayCommand]
    public async Task LoadDataAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            IsHealthy = false;
            ConnectionCount = 0;
            ServiceCount = 0;
            ConfigPath = "";
            Endpoint = "";
            PreviewServices.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;

            var healthTask = _api.GetHealthAsync();
            var connsTask = _api.GetConnectionsAsync();
            var servicesTask = _api.GetTunnelServicesAsync();
            var configTask = _api.GetConfigPathAsync();
            var statsTask = _api.GetOptimizerStatsAsync();

            await Task.WhenAll(healthTask, connsTask, servicesTask, configTask, statsTask);

            IsHealthy = (await healthTask).Ok;
            var services = await servicesTask;
            ConnectionCount = (await connsTask).Count;
            ServiceCount = services.Count;
            var path = (await configTask).Path;
            ConfigPath = string.IsNullOrWhiteSpace(path) ? "" : path;
            Endpoint = ResolveEndpoint();
            PreviewServices.Clear();
            foreach (var row in services.Take(6).Select(ServiceRowItem.FromSnapshot))
            {
                PreviewServices.Add(row);
            }
            try
            {
                SavedRatioText = Formatters.FormatPercentage((await statsTask).Global.SavedRatio);
            }
            catch
            {
                SavedRatioText = "0.0%";
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
        finally
        {
            IsLoading = false;
        }
    }

    private string ResolveEndpoint()
    {
        try
        {
            var cfg = _client.GetConfig();
            var management = cfg.ActiveConfig.ManagementUrl;
            if (!string.IsNullOrWhiteSpace(management)) return management;
        }
        catch { }
        return _client.CurrentStatus?.ServerAddr ?? "";
    }

    [RelayCommand]
    public async Task ReloadServerAsync()
    {
        try
        {
            ReloadMessage = I18nText.T("common_loading");
            var res = await _api.TriggerReloadAsync();
            ReloadMessage = I18nText.Format("reload_ok", ("seq", res.Seq));
            AppServices.ShowSuccess(ReloadMessage);
        }
        catch (Exception ex)
        {
            ReloadMessage = ex.Message;
            AppServices.ShowError(ex.Message, I18nText.T("reload_failed"));
        }
    }
}

public class ConnectionItem : ObservableObject
{
    public string Id { get; set; } = "";
    public string Client { get; set; } = "";
    public string Host { get; set; } = "";
    public string Upstream { get; set; } = "";
    public string PeerAddr { get; set; } = "";
    public string Proto { get; set; } = "";
    public string Duration { get; set; } = "0s";
    public string StartedAt { get; set; } = "";
    public string RawBytes { get; set; } = "0 B";
    public string WireBytes { get; set; } = "0 B";
    public string UplinkText { get; set; } = "";
    public string DownlinkText { get; set; } = "";
    public string OptimizerText { get; set; } = "";
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

    [ObservableProperty]
    private AdminReadyState _readyState = AdminReadyState.Loading;

    [ObservableProperty]
    private bool _isReady;

    [ObservableProperty]
    private bool _autoRefresh = true;

    [ObservableProperty]
    private string _autoRefreshText = "";

    private CancellationTokenSource? _autoRefreshCts;

    public AdminConnectionsViewModel()
    {
        UpdateAutoRefreshText();
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(RefreshAsync, TimeSpan.FromSeconds(3), () => _client.CurrentStatus?.Running == true && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

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
                                         c.Client.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                                         c.Host.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                                         c.Upstream.Contains(q, StringComparison.OrdinalIgnoreCase) ||
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
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            _allConnections.Clear();
            Connections.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;
            var list = await _api.GetConnectionsAsync();

            _allConnections.Clear();
            long nowMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();

            foreach (var r in list)
            {
                long durMs = Math.Max(0, nowMs - r.StartedAtUnixMs);
                var duration = Formatters.FormatUptime((ulong)(durMs / 1000));
                var raw = Formatters.FormatBytes(r.RawBytes);
                var wire = Formatters.FormatBytes(r.WireBytes);
                string started = r.StartedAtUnixMs > 0
                    ? DateTimeOffset.FromUnixTimeMilliseconds(r.StartedAtUnixMs).ToLocalTime().ToString("HH:mm:ss")
                    : "";

                _allConnections.Add(new ConnectionItem
                {
                    Id = r.Id,
                    Client = r.Client,
                    Host = r.Host,
                    Upstream = r.Upstream,
                    PeerAddr = string.IsNullOrWhiteSpace(r.Client) ? r.Host : r.Client,
                    Proto = "",
                    Duration = duration,
                    StartedAt = started,
                    RawBytes = raw,
                    WireBytes = wire,
                    OptimizerText = r.RawBytes == 0
                        ? "—"
                        : Formatters.FormatPercentage(r.RawBytes > r.WireBytes
                            ? (double)(r.RawBytes - r.WireBytes) / r.RawBytes
                            : 0),
                    UplinkText = $"{Formatters.FormatBytes(r.UplinkRawBytes)} → {Formatters.FormatBytes(r.UplinkWireBytes)}",
                    DownlinkText = $"{Formatters.FormatBytes(r.DownlinkRawBytes)} → {Formatters.FormatBytes(r.DownlinkWireBytes)}"
                });
            }

            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    [RelayCommand]
    public async Task DisconnectAsync(ConnectionItem? item)
    {
        if (item == null) return;
        if (!await AppServices.ConfirmAsync(
                I18nText.T("common_confirm"),
                I18nText.T("confirm_disconnect_session"),
                I18nText.T("session_disconnect"),
                destructive: true))
        {
            return;
        }
        try
        {
            await _api.CloseConnectionAsync(item.Id);
            _allConnections.Remove(item);
            Connections.Remove(item);
            AppServices.ShowSuccess(item.Id);
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
            AppServices.ShowError(ex.Message);
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
    public string ClientId { get; set; } = "";
    public string Remote { get; set; } = "";
    public bool Primary { get; set; }
    public bool RouteOnly { get; set; }
    public bool IsActive { get; set; } = true;
    public string RoleText { get; set; } = "";
    public string RouteText { get; set; } = "";
    public string ClientDisplay { get; set; } = "";

    public static ServiceRowItem FromSnapshot(AdminServiceSnapshot snap)
    {
        var s = snap.Service ?? new AdminRegisteredService();
        return new ServiceRowItem
        {
            Name = s.Name,
            Proto = string.IsNullOrWhiteSpace(s.Proto) ? "" : s.Proto.ToUpperInvariant(),
            LocalAddr = s.LocalAddr,
            RemoteAddr = s.RemoteAddr,
            Masquerade = string.IsNullOrWhiteSpace(s.MasqueradeHost) ? "—" : s.MasqueradeHost,
            ClientId = snap.ClientId ?? "",
            Remote = snap.Remote ?? "",
            Primary = snap.Primary,
            RouteOnly = s.RouteOnly,
            IsActive = true,
            RoleText = snap.Primary ? I18nText.T("services_primary") : I18nText.T("services_secondary"),
            RouteText = s.RouteOnly ? I18nText.T("services_route_only") : "",
            ClientDisplay = string.IsNullOrWhiteSpace(snap.ClientId) ? I18nText.T("admin_unknown") : snap.ClientId
        };
    }
}

public partial class AdminTunnelServicesViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    private readonly List<ServiceRowItem> _allServices = new();
    public ObservableCollection<ServiceRowItem> Services { get; } = new();

    [ObservableProperty] private string _searchQuery = "";
    [ObservableProperty] private bool _primaryOnly;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private AdminReadyState _readyState = AdminReadyState.Loading;

    [ObservableProperty]
    private bool _isReady;

    [ObservableProperty]
    private bool _autoRefresh = true;

    [ObservableProperty]
    private string _autoRefreshText = "";

    private CancellationTokenSource? _autoRefreshCts;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    public AdminTunnelServicesViewModel()
    {
        UpdateAutoRefreshText();
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    [RelayCommand]
    public void TogglePrimaryOnly() => PrimaryOnly = !PrimaryOnly;

    partial void OnSearchQueryChanged(string value) => ApplyFilter();
    partial void OnPrimaryOnlyChanged(bool value) => ApplyFilter();

    private void ApplyFilter()
    {
        Services.Clear();
        var q = SearchQuery.Trim();
        foreach (var row in _allServices)
        {
            if (PrimaryOnly && !row.Primary) continue;
            if (!string.IsNullOrWhiteSpace(q) &&
                !row.Name.Contains(q, StringComparison.OrdinalIgnoreCase) &&
                !row.ClientDisplay.Contains(q, StringComparison.OrdinalIgnoreCase) &&
                !row.Remote.Contains(q, StringComparison.OrdinalIgnoreCase) &&
                !row.LocalAddr.Contains(q, StringComparison.OrdinalIgnoreCase) &&
                !row.RemoteAddr.Contains(q, StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }
            Services.Add(row);
        }
    }

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(RefreshAsync, TimeSpan.FromSeconds(5), () => _client.CurrentStatus?.Running == true && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            _allServices.Clear();
            Services.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;
            var list = await _api.GetTunnelServicesAsync();

            _allServices.Clear();
            _allServices.AddRange(list.Select(ServiceRowItem.FromSnapshot));
            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
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
    public string LastUsed { get; set; } = "";
}

public sealed class TokenDialogContext
{
    public TokenDialogContext(AdminUsersViewModel owner) => Owner = owner;
    public AdminUsersViewModel Owner { get; }
}

public sealed class EditUserDialogContext
{
    public EditUserDialogContext(AdminUsersViewModel owner) => Owner = owner;
    public AdminUsersViewModel Owner { get; }
}

public partial class AdminUsersViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    private readonly List<UserRowItem> _allUsers = new();
    private readonly List<TokenRowItem> _allTokens = new();
    public ObservableCollection<UserRowItem> Users { get; } = new();
    public ObservableCollection<TokenRowItem> Tokens { get; } = new();

    [ObservableProperty] private string _searchQuery = "";
    [ObservableProperty] private string _sessionName = "";
    [ObservableProperty] private string _sessionDetail = "";
    [ObservableProperty] private bool _hasSession;
    [ObservableProperty] private bool _sessionIsAdmin;
    [ObservableProperty] private string _newTokenType = "client";
    public string[] TokenTypeValues { get; } = ["client", "connector", "admin"];

    private TokenDialogContext? _tokenDialog;
    private EditUserDialogContext? _editDialog;
    private static bool _dialogsRegistered;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private AdminReadyState _readyState = AdminReadyState.Loading;

    [ObservableProperty]
    private bool _isReady;

    [ObservableProperty]
    private bool _autoRefresh = true;

    [ObservableProperty]
    private string _autoRefreshText = "";

    private CancellationTokenSource? _autoRefreshCts;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

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

    [ObservableProperty]
    private UserRowItem? _editingUser;

    [ObservableProperty]
    private string _editUserTitle = "";

    [ObservableProperty]
    private string _editRole = "member";

    [ObservableProperty]
    private string _editServiceRules = "";

    [ObservableProperty]
    private bool _showEditUserDialog;

    public static readonly string[] AvailableRoles = ["admin", "member", "disabled"];

    public AdminUsersViewModel()
    {
        EnsureDialogs();
        UpdateAutoRefreshText();
        _ = RefreshAsync();
    }

    private static void EnsureDialogs()
    {
        if (_dialogsRegistered) return;
        _dialogsRegistered = true;
        AppServices.DialogManager.Register<CreateTokenDialog, TokenDialogContext>();
        AppServices.DialogManager.Register<EditUserDialog, EditUserDialogContext>();
    }

    partial void OnSearchQueryChanged(string value) => ApplyUserFilter();

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(RefreshAsync, TimeSpan.FromSeconds(5), () => _client.CurrentStatus?.Running == true && !IsLoading && !ShowEditUserDialog && !ShowTokenDialog);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            Users.Clear();
            Tokens.Clear();
            _allUsers.Clear();
            _allTokens.Clear();
            HasSession = false;
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;

            var usersTask = _api.GetUsersAsync();
            var tokensTask = _api.GetTokensAsync();
            var sessionTask = _api.GetSessionAsync();

            await Task.WhenAll(usersTask, tokensTask, sessionTask);

            _allUsers.Clear();
            foreach (var u in await usersTask)
            {
                string created = u.CreatedAtUnixMs > 0
                    ? DateTimeOffset.FromUnixTimeMilliseconds(u.CreatedAtUnixMs).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                    : "";
                _allUsers.Add(new UserRowItem
                {
                    Id = u.Id,
                    Username = string.IsNullOrWhiteSpace(u.DisplayName) ? u.Username : $"{u.DisplayName} ({u.Username})",
                    Role = u.Role.ToUpperInvariant(),
                    GithubId = string.IsNullOrWhiteSpace(u.Id) ? u.Username : u.Id,
                    CreatedAt = created,
                    ServiceRules = u.ServiceRules ?? new List<string>()
                });
            }

            if (string.IsNullOrWhiteSpace(SelectedUserId) && _allUsers.Count > 0)
            {
                SelectedUserId = _allUsers[0].Id;
            }

            _allTokens.Clear();
            foreach (var t in await tokensTask)
            {
                string created = t.CreatedAtUnixMs > 0
                    ? DateTimeOffset.FromUnixTimeMilliseconds(t.CreatedAtUnixMs).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                    : "";
                string expires = t.ExpiresAtUnixMs.HasValue
                    ? DateTimeOffset.FromUnixTimeMilliseconds(t.ExpiresAtUnixMs.Value).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                    : I18nText.T("common_never");
                string lastUsed = t.LastUsedUnixMs > 0
                    ? DateTimeOffset.FromUnixTimeMilliseconds(t.LastUsedUnixMs).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                    : I18nText.T("users_never");

                _allTokens.Add(new TokenRowItem
                {
                    Id = t.Id,
                    Name = t.Name,
                    TokenType = t.TokenType.ToUpperInvariant(),
                    CreatedAt = created,
                    ExpiresAt = expires,
                    LastUsed = lastUsed
                });
            }

            var session = await sessionTask;
            HasSession = session.Authenticated;
            SessionIsAdmin = session.IsAdmin;
            SessionName = session.DisplayName ?? session.Username ?? "";
            SessionDetail = session.Username ?? "";
            ApplyUserFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
        }
        finally
        {
            IsLoading = false;
        }
    }

    private void ApplyUserFilter()
    {
        Users.Clear();
        Tokens.Clear();
        var q = SearchQuery.Trim();
        foreach (var user in _allUsers)
        {
            if (string.IsNullOrWhiteSpace(q) ||
                user.Username.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                user.Role.Contains(q, StringComparison.OrdinalIgnoreCase))
            {
                Users.Add(user);
            }
        }
        foreach (var token in _allTokens)
        {
            if (string.IsNullOrWhiteSpace(q) ||
                token.Name.Contains(q, StringComparison.OrdinalIgnoreCase) ||
                token.TokenType.Contains(q, StringComparison.OrdinalIgnoreCase))
            {
                Tokens.Add(token);
            }
        }
    }

    [RelayCommand]
    public void OpenCreateTokenDialog()
    {
        ShowTokenDialog = true;
        NewTokenName = "";
        NewTokenType = "client";
        CreatedRawToken = null;
        if (_allUsers.Count > 0 && string.IsNullOrWhiteSpace(SelectedUserId))
        {
            SelectedUserId = _allUsers[0].Id;
        }
        _tokenDialog = new TokenDialogContext(this);
        AppServices.DialogManager
            .CreateDialog(_tokenDialog)
            .Dismissible()
            .WithCancelCallback(() => ShowTokenDialog = false)
            .WithMinWidth(520)
            .WithMaxWidth(680)
            .Show();
    }

    [RelayCommand]
    public void CloseCreateTokenDialog()
    {
        ShowTokenDialog = false;
        if (_tokenDialog != null)
        {
            AppServices.DialogManager.Close(_tokenDialog);
            _tokenDialog = null;
        }
    }

    [RelayCommand]
    public async Task SubmitCreateTokenAsync()
    {
        if (string.IsNullOrWhiteSpace(NewTokenName)) return;

        try
        {
            ErrorMessage = null;
            if (string.IsNullOrWhiteSpace(SelectedUserId))
            {
                ErrorMessage = I18nText.T("common_error");
                return;
            }
            string uid = SelectedUserId;
            var resp = await _api.CreateTokenAsync(NewTokenName.Trim(), uid, NewTokenType, SelectedExpirationDays > 0 ? SelectedExpirationDays : null);
            CreatedRawToken = resp.RawToken;

            var t = resp.Token;
            string created = t.CreatedAtUnixMs > 0
                ? DateTimeOffset.FromUnixTimeMilliseconds(t.CreatedAtUnixMs).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                : "";
            string expires = t.ExpiresAtUnixMs.HasValue
                ? DateTimeOffset.FromUnixTimeMilliseconds(t.ExpiresAtUnixMs.Value).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                : I18nText.T("common_never");
            string lastUsed = t.LastUsedUnixMs > 0
                ? DateTimeOffset.FromUnixTimeMilliseconds(t.LastUsedUnixMs).ToLocalTime().ToString("yyyy-MM-dd HH:mm")
                : I18nText.T("users_never");

            var row = new TokenRowItem
            {
                Id = t.Id,
                Name = t.Name,
                TokenType = string.IsNullOrWhiteSpace(t.TokenType) ? NewTokenType.ToUpperInvariant() : t.TokenType.ToUpperInvariant(),
                CreatedAt = created,
                ExpiresAt = expires,
                LastUsed = lastUsed
            };
            _allTokens.Insert(0, row);
            ApplyUserFilter();

            AppServices.ShowSuccess(I18nText.Format("token_created_detail", ("name", t.Name)), I18nText.T("token_created"));
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
            AppServices.ShowError(ex.Message, I18nText.T("token_create_failed"));
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
            AppServices.ShowSuccess(
                I18nText.Format("users_role_updated", ("username", item.Username), ("role", item.Role)),
                I18nText.T("users_role_update_title"));
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
            AppServices.ShowError(ex.Message, I18nText.T("users_role_update_failed"));
        }
    }

    [RelayCommand]
    public async Task RevokeTokenAsync(TokenRowItem? item)
    {
        if (item == null) return;
        if (!await AppServices.ConfirmAsync(
                I18nText.T("common_confirm"),
                I18nText.T("confirm_revoke_token"),
                I18nText.T("common_delete"),
                destructive: true))
        {
            return;
        }

        try
        {
            ErrorMessage = null;
            await _api.RevokeTokenAsync(item.Id);
            _allTokens.Remove(item);
            Tokens.Remove(item);
            AppServices.ShowSuccess(I18nText.Format("token_revoked_detail", ("name", item.Name)), I18nText.T("token_revoked"));
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
            AppServices.ShowError(ex.Message, I18nText.T("token_revoke_failed"));
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
            AppServices.ShowSuccess(I18nText.T("token_copied"), I18nText.T("common_copied"));
        }
    }

    [RelayCommand]
    public void OpenEditUserDialog(UserRowItem? item)
    {
        if (item == null) return;
        EditingUser = item;
        EditUserTitle = I18nText.Format("users_edit_title", ("username", item.Username));
        EditRole = item.Role.ToLowerInvariant();
        EditServiceRules = string.Join(Environment.NewLine, item.ServiceRules);
        ShowEditUserDialog = true;
        _editDialog = new EditUserDialogContext(this);
        AppServices.DialogManager
            .CreateDialog(_editDialog)
            .Dismissible()
            .WithCancelCallback(() => ShowEditUserDialog = false)
            .WithMinWidth(480)
            .WithMaxWidth(640)
            .Show();
    }

    [RelayCommand]
    public void CloseEditUserDialog()
    {
        ShowEditUserDialog = false;
        EditingUser = null;
        if (_editDialog != null)
        {
            AppServices.DialogManager.Close(_editDialog);
            _editDialog = null;
        }
    }

    [RelayCommand]
    public async Task SaveUserChangesAsync()
    {
        if (EditingUser == null) return;

        try
        {
            ErrorMessage = null;
            var rules = EditServiceRules
                .Split(new[] { "\r\n", "\r", "\n" }, StringSplitOptions.RemoveEmptyEntries)
                .Select(r => r.Trim())
                .Where(r => !string.IsNullOrWhiteSpace(r))
                .ToList();

            await _api.PutUserAsync(EditingUser.Id, EditRole, rules);

            EditingUser.Role = EditRole.ToUpperInvariant();
            EditingUser.ServiceRules = rules;

            AppServices.ShowSuccess(
                I18nText.Format("user_updated", ("name", EditingUser.Username)),
                I18nText.T("user_saved"));
            CloseEditUserDialog();
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
            AppServices.ShowError(ex.Message, I18nText.T("common_save_failed"));
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
    [ObservableProperty] private string _uplinkBatching = "0 ms";

    [ObservableProperty] private string _downlinkRawWire = "0 B / 0 B (0%)";
    [ObservableProperty] private string _downlinkP50 = "0 ms";
    [ObservableProperty] private string _downlinkP90 = "0 ms";
    [ObservableProperty] private string _downlinkP99 = "0 ms";
    [ObservableProperty] private string _downlinkMax = "0 ms";
    [ObservableProperty] private string _downlinkBatching = "0 ms";

    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private AdminReadyState _readyState = AdminReadyState.Loading;
    [ObservableProperty] private bool _isReady;

    [ObservableProperty] private bool _autoRefresh = true;
    [ObservableProperty] private string _autoRefreshText = "";
    private CancellationTokenSource? _autoRefreshCts;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    [RelayCommand]
    public void OpenConnectors() => NavigationService.Instance.NavigateTo("admin.connectors");

    private static string FormatBatching(Quantiles? q)
    {
        if (q == null) return "—";
        return $"{q.P50Us / 1000.0:0.##} / {q.P99Us / 1000.0:0.##} ms";
    }

    public AdminTrafficViewModel()
    {
        UpdateAutoRefreshText();
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(RefreshAsync, TimeSpan.FromSeconds(3), () => _client.CurrentStatus?.Running == true && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;

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
                UplinkBatching = FormatBatching(up.BatchingDelay);
            }

            if (stats.Downlink != null)
            {
                var down = stats.Downlink;
                DownlinkRawWire = $"{Formatters.FormatBytes(down.RawBytes)} / {Formatters.FormatBytes(down.WireBytes)} ({Formatters.FormatPercentage(down.SavedRatio)})";
                DownlinkP50 = $"{down.CompressionTime?.P50Us / 1000.0:0.##} ms";
                DownlinkP90 = $"{down.CompressionTime?.P90Us / 1000.0:0.##} ms";
                DownlinkP99 = $"{down.CompressionTime?.P99Us / 1000.0:0.##} ms";
                DownlinkMax = $"{down.CompressionTime?.MaxUs / 1000.0:0.##} ms";
                DownlinkBatching = FormatBatching(down.BatchingDelay);
            }
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
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
    public ObservableCollection<ServiceRowItem> Unattached { get; } = new();

    [ObservableProperty] private string _searchQuery = "";
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private AdminReadyState _readyState = AdminReadyState.Loading;
    [ObservableProperty] private bool _isReady;

    [ObservableProperty] private bool _autoRefresh = true;
    [ObservableProperty] private string _autoRefreshText = "";
    private CancellationTokenSource? _autoRefreshCts;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    public AdminConnectorsViewModel()
    {
        UpdateAutoRefreshText();
        _ = RefreshAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = RefreshAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(RefreshAsync, TimeSpan.FromSeconds(4), () => _client.CurrentStatus?.Running == true && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

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
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            Connectors.Clear();
            _allConnectors.Clear();
            Unattached.Clear();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;
            var snapshots = await _api.GetTunnelServicesAsync();

            var groups = new Dictionary<string, ConnectorGroupItem>();
            var unattached = new List<ServiceRowItem>();
            foreach (var s in snapshots)
            {
                if (string.IsNullOrWhiteSpace(s.ClientId))
                {
                    unattached.Add(ServiceRowItem.FromSnapshot(s));
                    continue;
                }
                if (!groups.TryGetValue(s.ClientId, out var grp))
                {
                    grp = new ConnectorGroupItem
                    {
                        ClientId = s.ClientId,
                        RemoteAddr = s.Remote,
                        Primary = s.Primary,
                        Services = new List<AdminRegisteredService>()
                    };
                    groups[s.ClientId] = grp;
                }
                grp.Services.Add(s.Service);
            }

            _allConnectors.Clear();
            _allConnectors.AddRange(groups.Values);
            Unattached.Clear();
            foreach (var row in unattached)
            {
                Unattached.Add(row);
            }
            ApplyFilter();
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
        }
        finally
        {
            IsLoading = false;
        }
    }
}

public partial class AdminMiddlewareViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<AdminMiddlewareItem> Middlewares { get; } = new();
    public ObservableCollection<MiddlewareFieldViewModel> ConfigFields { get; } = new();

    [ObservableProperty] private AdminMiddlewareItem? _selectedMiddleware;
    [ObservableProperty] private bool _isMiddlewareEnabled = true;
    [ObservableProperty] private string? _statusMessage;
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private AdminReadyState _readyState = AdminReadyState.Loading;
    [ObservableProperty] private bool _isReady;
    [ObservableProperty] private string? _errorMessage;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    public AdminMiddlewareViewModel()
    {
        _ = LoadDataAsync();
    }

    public void OnNavigatedTo() => _ = LoadDataAsync(SelectedMiddleware?.Name);
    public void OnNavigatedFrom() { }

    public async Task LoadDataAsync(string? preserveName = null)
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            StatusMessage = null;
            Middlewares.Clear();
            SelectedMiddleware = null;
            return;
        }

        Middlewares.Clear();
        try
        {
            IsLoading = true;
            StatusMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;
            var list = await _api.GetMiddlewaresAsync();
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
        finally
        {
            IsLoading = false;
        }
    }

    partial void OnSelectedMiddlewareChanged(AdminMiddlewareItem? value)
    {
        ConfigFields.Clear();
        if (value == null) return;

        if (value.EffectiveConfig.TryGetValue("enabled", out var en))
        {
            IsMiddlewareEnabled = !string.Equals(en?.ToString(), "false", StringComparison.OrdinalIgnoreCase);
        }
        else
        {
            IsMiddlewareEnabled = true;
        }

        if (value.Schema == null) return;

        foreach (var f in value.Schema.Fields)
        {
            if (f.Key.Equals("enabled", StringComparison.OrdinalIgnoreCase)) continue;

            string currentVal = (value.EffectiveConfig.TryGetValue(f.Key, out var v) ? v?.ToString() : f.DefaultValue?.ToString()) ?? "";

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
    public async Task SaveConfigAsync()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            IsLoading = true;
            var dict = ConfigFields.ToDictionary(f => f.Key, f => f.GetStringValue());
            dict["enabled"] = IsMiddlewareEnabled ? "true" : "false";

            string currentName = SelectedMiddleware.Name;
            var updated = await _api.UpdateMiddlewareConfigAsync(currentName, dict);
            StatusMessage = I18nText.Format("middleware_saved", ("name", currentName));
            AppServices.ShowSuccess(StatusMessage, I18nText.T("common_success"));
            await LoadDataAsync(currentName);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
            AppServices.ShowError(ex.Message, I18nText.T("common_save_failed"));
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    public async Task ResetConfigAsync()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            IsLoading = true;
            string currentName = SelectedMiddleware.Name;
            var reset = await _api.ResetMiddlewareConfigAsync(currentName);
            StatusMessage = I18nText.Format("middleware_reset_ok", ("name", currentName));
            AppServices.ShowSuccess(StatusMessage, I18nText.T("common_success"));
            await LoadDataAsync(currentName);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
            AppServices.ShowError(ex.Message, I18nText.T("common_reset_failed"));
        }
        finally
        {
            IsLoading = false;
        }
    }
}

public partial class AdminRuntimeViewModel : ViewModelBase, INavigationAware
{
    private readonly AdminApiClient _api = AdminApiClient.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty] private bool _isHealthy;
    [ObservableProperty] private string _healthText = "";
    [ObservableProperty] private string _configPath = "";
    [ObservableProperty] private string? _reloadResult;
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private AdminReadyState _readyState = AdminReadyState.Loading;
    [ObservableProperty] private bool _isReady;

    [RelayCommand]
    public void GoToConnect() => NavigationService.Instance.NavigateTo("client.overview");

    [ObservableProperty] private bool _autoRefresh = true;
    [ObservableProperty] private string _autoRefreshText = "";
    private CancellationTokenSource? _autoRefreshCts;

    public AdminRuntimeViewModel()
    {
        UpdateAutoRefreshText();
        _ = LoadDataAsync();
    }

    public void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        if (_client.CurrentStatus?.Running == true)
        {
            _ = LoadDataAsync();
        }
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    private void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? (LocalizationManager.Instance["admin_on"] ?? "开")
            : (LocalizationManager.Instance["admin_off"] ?? "关");
        string template = LocalizationManager.Instance["admin_auto_refresh"] ?? "自动刷新：{state}";
        AutoRefreshText = template.Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    private void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(LoadDataAsync, TimeSpan.FromSeconds(5), () => _client.CurrentStatus?.Running == true && !IsLoading);
        }
    }

    private void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    [RelayCommand]
    public async Task LoadDataAsync()
    {
        if (_client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            IsHealthy = false;
            HealthText = "";
            ConfigPath = "";
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            ReadyState = AdminReadyState.Ready;
            IsReady = true;

            var healthTask = _api.GetHealthAsync();
            var pathTask = _api.GetConfigPathAsync();

            await Task.WhenAll(healthTask, pathTask);

            IsHealthy = (await healthTask).Ok;
            HealthText = IsHealthy ? I18nText.T("runtime_health_ok") : I18nText.T("runtime_health_down");
            var path = (await pathTask).Path;
            ConfigPath = string.IsNullOrWhiteSpace(path) ? "" : path;
        }
        catch (Exception ex)
        {
            ErrorMessage = I18nText.Format("admin_load_failed", ("error", ex.Message));
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
            ReloadResult = I18nText.Format("admin_reload_sent", ("seq", res.Seq));
            AppServices.ShowSuccess(ReloadResult, I18nText.T("admin_reload"));
            await LoadDataAsync();
        }
        catch (Exception ex)
        {
            ReloadResult = I18nText.Format("admin_reload_failed", ("message", ex.Message));
            AppServices.ShowError(ex.Message, I18nText.T("reload_failed"));
        }
    }
}
